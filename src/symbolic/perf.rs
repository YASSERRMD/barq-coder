use super::AstWalker;
use syn::visit::Visit;

// Advanced perf linting (O(n^2), clones in loop)
pub fn lint_perf(file_content: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Ok(file) = syn::parse_str::<syn::File>(file_content) {
        let mut walker = AstWalker::new();
        walker.visit_file(&file);

        if !walker.loops.is_empty() && walker.clones > 0 {
            warnings.push("Detected clones being performed, and loops exist. Ensure clone is not inside an inner loop.".to_string());
        }
    }
    warnings
}
