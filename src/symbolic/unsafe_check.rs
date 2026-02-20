use super::AstWalker;
use syn::visit::Visit;

pub fn check_unsafe(file_content: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if let Ok(file) = syn::parse_str::<syn::File>(file_content) {
        let mut walker = AstWalker::new();
        walker.visit_file(&file);

        if !walker.unsafe_blocks.is_empty() {
            diagnostics.push(format!("Found {} unsafe blocks. Please ensure they are necessary.", walker.unsafe_blocks.len()));
        }
    }
    diagnostics
}
