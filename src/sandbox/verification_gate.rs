use crate::sandbox::worktree::ShadowWorktree;
use crate::verifier::Verifier;
use crate::barq::BarqIndex;
use std::sync::Arc;

/// Result produced by the VerificationGate after checking the shadow workspace.
#[derive(Debug, Clone)]
pub struct GateResult {
    /// Whether `cargo check` passed inside the shadow workspace.
    pub check_pass: bool,
    /// Whether `cargo test` passed inside the shadow workspace.
    pub test_pass: bool,
    /// Diagnostic output from the compiler (stderr).
    pub diagnostics: Vec<String>,
    /// Full unified diff between shadow and origin for all changed files.
    pub patch: String,
    /// Whether the gate fully passed and the patch can be applied.
    pub approved: bool,
}

impl GateResult {
    /// A blocking failure with diagnostics.
    pub fn failed(diagnostics: Vec<String>, patch: String) -> Self {
        GateResult {
            check_pass: false,
            test_pass: false,
            diagnostics,
            patch,
            approved: false,
        }
    }
}

/// The VerificationGate owns a ShadowWorktree and enforces the
/// Verification-First contract:
///
///   1. Accept file writes from the CoderAgent into the shadow.
///   2. Run `cargo check` + `cargo test` in the shadow.
///   3. Produce a `GateResult` (passed/failed + full patch).
///   4. Only if approved does the caller apply the patch to the real workspace.
///
/// This is the architectural answer to the industry pain point of agents that
/// produce code with subtle bugs and force expensive code-review overhead.
pub struct VerificationGate {
    pub shadow: ShadowWorktree,
    pub verifier: Verifier,
}

impl VerificationGate {
    /// Create a new gate backed by a fresh shadow of `workspace_root`.
    pub async fn new(workspace_root: &str, barq: Arc<BarqIndex>) -> anyhow::Result<Self> {
        let shadow = ShadowWorktree::create(workspace_root).await?;
        let verifier = Verifier::new(barq, workspace_root);
        Ok(Self { shadow, verifier })
    }

    /// Write a file into the shadow (forwarded from CoderAgent tool calls).
    pub async fn stage_file(&self, relative_path: &str, content: &str) -> anyhow::Result<()> {
        self.shadow.write_file(relative_path, content).await
    }

    /// Run the full verification pipeline on the shadow workspace.
    ///
    /// Returns a `GateResult` describing whether the gate passed, the
    /// compiler diagnostics, and the unified patch ready for TUI review.
    pub async fn verify(&self) -> GateResult {
        let mut diagnostics = Vec::new();

        // 1. cargo check
        let (check_pass, check_err) = self.shadow.cargo_check().await;
        if !check_pass {
            diagnostics.push(format!("cargo check:\n{}", check_err));
        }

        // 2. cargo test (only if check passed — no point running tests on broken code)
        let test_pass = if check_pass {
            let (tp, test_err) = self.shadow.cargo_test().await;
            if !tp {
                diagnostics.push(format!("cargo test:\n{}", test_err));
            }
            tp
        } else {
            false
        };

        // 3. Build a full patch for TUI review regardless of outcome.
        let patch = self.shadow.full_diff().await;

        let approved = check_pass && test_pass;

        GateResult {
            check_pass,
            test_pass,
            diagnostics,
            patch,
            approved,
        }
    }

    /// Apply the verified shadow onto the real workspace. Must only be called
    /// after `verify()` returns `approved == true` AND the user accepts the
    /// patch in the TUI Action Sandbox.
    pub async fn apply(&self) -> anyhow::Result<()> {
        self.shadow.apply_to_origin().await
    }
}
