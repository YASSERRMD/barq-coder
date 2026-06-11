use serde::{Deserialize, Serialize};

/// Trust tier for a provider, controlling which tool categories it may invoke.
///
/// A provider at a lower tier cannot invoke high-risk tools even if those tools
/// appear in the registry. Local providers (Ollama) default to Full; hosted
/// providers that accept arbitrary untrusted input should be capped lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustTier {
    /// Read-only: search, read_file, barq_search, list_files.
    ReadOnly = 0,
    /// Code-modify: edit_file, create_file, cargo_check, plus ReadOnly tools.
    CodeModify = 1,
    /// Shell: shell_exec, run_tests, cargo_test, plus CodeModify tools.
    Shell = 2,
    /// Full: all tools including delegate_task and network operations.
    Full = 3,
}

impl TrustTier {
    /// Return true if a tool with the given name is permitted at this tier.
    pub fn permits_tool(&self, tool_name: &str) -> bool {
        let required = Self::required_tier_for(tool_name);
        (*self as u8) >= (required as u8)
    }

    fn required_tier_for(tool_name: &str) -> TrustTier {
        match tool_name {
            // Shell execution tools — require Shell tier
            "shell_exec" | "cargo_check" | "cargo_test" | "run_tests" => TrustTier::Shell,
            // Swarm delegation and network — require Full tier
            "delegate_task" | "web_fetch" | "web_search" => TrustTier::Full,
            // Filesystem writes — require CodeModify tier
            "edit_file" | "create_file" | "write_file" | "delete_file" | "apply_patch" => {
                TrustTier::CodeModify
            }
            // Everything else (reads, searches) — ReadOnly
            _ => TrustTier::ReadOnly,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TrustTier::ReadOnly => "read_only",
            TrustTier::CodeModify => "code_modify",
            TrustTier::Shell => "shell",
            TrustTier::Full => "full",
        }
    }
}
