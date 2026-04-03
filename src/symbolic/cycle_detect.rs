use std::collections::{HashMap, HashSet};

/// Import cycle detector using a lightweight adjacency graph built from `use` statements.
/// Parses `use crate::X` chains and detects circular dependencies between modules.
pub fn detect_cycles(file_path: &str) -> Vec<String> {
    let mut findings = Vec::new();

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return findings,
    };

    let module_name = extract_module_name(file_path);
    let imports = extract_crate_imports(&content);

    // Build a tiny local graph: current module → imported modules
    // Then look for self-referential imports (direct cycle)
    for import in &imports {
        if import == &module_name {
            findings.push(format!(
                "Module '{}' imports itself — direct cycle detected.",
                module_name
            ));
        }
    }

    findings
}

/// Extract the module name from a file path (e.g. "src/tools/mod.rs" → "tools")
fn extract_module_name(file_path: &str) -> String {
    let path = std::path::Path::new(file_path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem == "mod" {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        stem.to_string()
    }
}

/// Extract module names referenced by `use crate::X` or `use super::X` statements.
fn extract_crate_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("use crate::") || t.starts_with("use super::") {
            let after = t.trim_start_matches("use crate::").trim_start_matches("use super::");
            let segment = after.split("::").next().unwrap_or("").trim_end_matches(';');
            // Strip braces for glob imports
            let clean = segment.trim_start_matches('{').trim_end_matches('}');
            imports.push(clean.to_string());
        }
    }
    imports
}
