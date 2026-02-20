use tower_lsp::lsp_types::*;

pub fn get_diagnostics(_text: &str) -> Vec<Diagnostic> {
    // Mock diagnostics from symbolic verifier
    vec![]
}
