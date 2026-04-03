use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

/// A shadow workspace is a temporary copy of the current workspace.
/// Agent writes happen here first; the verified diff is only surfaced to the
/// user when `cargo check` + `cargo test` both pass inside the shadow.
///
/// Inspired by Claude Code's `--fork` + git worktree isolation, but fully
/// local and integrated with the Verification-First contract.
pub struct ShadowWorktree {
    /// Path to the temporary shadow directory.
    pub path: PathBuf,
    /// The ID of this shadow (used for tracking in session logs).
    pub id: String,
    /// The original workspace root.
    pub origin: PathBuf,
}

impl ShadowWorktree {
    /// Create a new shadow by copying the workspace into a temp dir.
    /// The copy is fast because we exclude `target/` and `.git/`.
    pub async fn create(workspace_root: &str) -> Result<Self> {
        let id = Uuid::new_v4().to_string()[..8].to_string();
        let shadow_path = std::env::temp_dir().join(format!("barq_shadow_{}", id));
        fs::create_dir_all(&shadow_path)
            .await
            .context("Failed to create shadow directory")?;

        let origin = Path::new(workspace_root)
            .canonicalize()
            .context("Failed to canonicalize workspace root")?;

        // Use rsync to copy workspace, excluding target/ and .git/ for speed.
        let rsync_result = Command::new("rsync")
            .args([
                "-a",
                "--exclude=target/",
                "--exclude=.git/",
                "--exclude=.barqcoder/",
            ])
            .arg(format!("{}/", origin.display()))
            .arg(shadow_path.to_string_lossy().as_ref())
            .output()
            .await;

        match rsync_result {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                // rsync failed — fall back to a Tokio recursive copy
                tracing::warn!(
                    "rsync failed: {}. Falling back to recursive copy.",
                    String::from_utf8_lossy(&out.stderr)
                );
                copy_dir_recursive(&origin, &shadow_path).await?;
            }
            Err(_) => {
                copy_dir_recursive(&origin, &shadow_path).await?;
            }
        }

        Ok(ShadowWorktree {
            path: shadow_path,
            id,
            origin,
        })
    }

    /// Write a file into the shadow workspace (relative path).
    pub async fn write_file(&self, relative_path: &str, content: &str) -> Result<()> {
        let abs = self.path.join(relative_path);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&abs, content).await.context(format!(
            "Failed to write shadow file: {}",
            abs.display()
        ))
    }

    /// Read a file from the shadow workspace (relative path).
    pub async fn read_file(&self, relative_path: &str) -> Result<String> {
        let abs = self.path.join(relative_path);
        fs::read_to_string(&abs)
            .await
            .context(format!("Failed to read shadow file: {}", abs.display()))
    }

    /// Run `cargo check` on the shadow workspace. Returns (passed, stderr).
    pub async fn cargo_check(&self) -> (bool, String) {
        let out = Command::new("cargo")
            .args(["check", "--message-format=short"])
            .current_dir(&self.path)
            .output()
            .await;

        match out {
            Ok(o) => (
                o.status.success(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            ),
            Err(e) => (false, format!("cargo check spawn error: {e}")),
        }
    }

    /// Run `cargo test` on the shadow workspace. Returns (passed, stderr).
    pub async fn cargo_test(&self) -> (bool, String) {
        let out = Command::new("cargo")
            .args(["test", "--no-fail-fast"])
            .current_dir(&self.path)
            .output()
            .await;

        match out {
            Ok(o) => (
                o.status.success(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            ),
            Err(e) => (false, format!("cargo test spawn error: {e}")),
        }
    }

    /// Produce a unified diff between the shadow and the origin for a given
    /// relative file path.
    pub async fn diff_file(&self, relative_path: &str) -> Result<String> {
        let origin_file = self.origin.join(relative_path);
        let shadow_file = self.path.join(relative_path);

        let out = Command::new("diff")
            .args(["-u", "--label", relative_path, "--label", relative_path])
            .arg(&origin_file)
            .arg(&shadow_file)
            .output()
            .await
            .context("Failed to run diff")?;

        // diff exits 1 when there are differences — that's not an error.
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// Produce a full workspace diff (all changed files between shadow and origin).
    pub async fn full_diff(&self) -> String {
        let out = Command::new("diff")
            .args([
                "-rqu",
                "--exclude=target",
                "--exclude=.barqcoder",
            ])
            .arg(&self.origin)
            .arg(&self.path)
            .output()
            .await;

        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(e) => format!("diff error: {e}"),
        }
    }

    /// Apply all files from the shadow workspace onto the origin workspace.
    /// Only call this after the verification gate passes.
    pub async fn apply_to_origin(&self) -> Result<()> {
        let out = Command::new("rsync")
            .args(["-a", "--exclude=target/", "--exclude=.git/"])
            .arg(format!("{}/", self.path.display()))
            .arg(self.origin.to_string_lossy().as_ref())
            .output()
            .await
            .context("Failed to rsync shadow back to origin")?;

        if !out.status.success() {
            anyhow::bail!(
                "rsync apply failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Ok(())
    }
}

impl Drop for ShadowWorktree {
    fn drop(&mut self) {
        // Best-effort cleanup of the temp directory on drop.
        let path = self.path.clone();
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(path);
        });
    }
}

/// Recursive async directory copy (fallback when rsync is unavailable).
#[async_recursion::async_recursion]
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).await?;
    let mut entries = fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        // Skip build artifacts and git history.
        if name == "target" || name == ".git" || name == ".barqcoder" {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&file_name);
        let ft = entry.file_type().await?;
        if ft.is_dir() {
            copy_dir_recursive(&src_path, &dst_path).await?;
        } else {
            fs::copy(&src_path, &dst_path).await?;
        }
    }
    Ok(())
}
