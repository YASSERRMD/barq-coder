use crate::barq::BarqIndex;
use std::sync::Arc;
use tokio::process::Command;

pub struct Verifier {
    pub barq: Arc<BarqIndex>,
    pub workspace: String,
}

pub struct VerifyResult {
    pub cargo_check_pass: bool,
    pub cargo_test_pass: bool,
    pub semantic_score: f32,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub should_revert: bool,
}

impl Verifier {
    pub fn new(barq: Arc<BarqIndex>, workspace: &str) -> Self {
        Self {
            barq,
            workspace: workspace.to_string(),
        }
    }

    pub async fn verify_edit(
        &self,
        _file_path: &str,
        original: &str,
        patched: &str,
    ) -> VerifyResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Step 1: cargo check
        let mut check_cmd = Command::new("cargo");
        check_cmd
            .arg("check")
            .arg("--message-format")
            .arg("json")
            .current_dir(&self.workspace);

        let check_out = check_cmd.output().await;
        let mut cargo_check_pass = false;

        if let Ok(out) = check_out {
            cargo_check_pass = out.status.success();
            if !cargo_check_pass {
                errors.push(String::from_utf8_lossy(&out.stderr).to_string());
            }
        } else {
            errors.push("Failed to run cargo check".to_string());
        }

        // Step 2: cargo test
        let mut test_cmd = Command::new("cargo");
        test_cmd
            .arg("test")
            .arg("--no-fail-fast")
            .current_dir(&self.workspace);

        let test_out = test_cmd.output().await;
        let mut cargo_test_pass = false;

        if let Ok(out) = test_out {
            cargo_test_pass = out.status.success();
            if !cargo_test_pass {
                errors.push(String::from_utf8_lossy(&out.stderr).to_string());
            }
        } else {
            errors.push("Failed to run cargo test".to_string());
        }

        // Step 3: semantic diff
        let semantic_score = if original != patched {
            0.85 // Placeholder logic since BarqIndex true vector comparing requires both
        } else {
            1.0
        };

        // Step 4: Validate unsafe
        if !verify_no_unsafe(patched) {
            errors.push("Unsafe block detected".to_string());
            cargo_check_pass = false;
        }

        VerifyResult {
            cargo_check_pass,
            cargo_test_pass,
            semantic_score,
            errors,
            warnings,
            should_revert: !cargo_check_pass || !cargo_test_pass,
        }
    }

    pub fn cycle_check(&self, symbol: &str) -> bool {
        let deps = self.barq.graph_deps(symbol);
        deps.contains(&symbol.to_string()) 
    }
}

pub fn verify_no_unsafe(content: &str) -> bool {
    // Basic string check as a fallback
    if content.contains("unsafe {") || content.contains("unsafe fn") || content.contains("unsafe trait") {
         return false;
    }
    
    // Attempt parse
    if let Ok(file) = syn::parse_file(content) {
        for item in file.items {
            match item {
                syn::Item::Fn(f) => {
                    if f.sig.unsafety.is_some() {
                        return false;
                    }
                }
                syn::Item::Trait(t) => {
                    if t.unsafety.is_some() {
                        return false;
                    }
                }
                syn::Item::Impl(i) => {
                    if i.unsafety.is_some() {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }
    true
}
