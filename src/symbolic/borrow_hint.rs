use super::AstWalker;
use syn::visit::Visit;

pub fn analyze_borrows(file_content: &str) -> Vec<String> {
    let mut hints = Vec::new();
    if let Ok(file) = syn::parse_str::<syn::File>(file_content) {
        let mut walker = AstWalker::new();
        walker.visit_file(&file);

        if walker.clones > 2 {
            hints.push(format!("Detected {} .clone() calls. Consider passing by reference instead.", walker.clones));
        }
    }
    hints
}
