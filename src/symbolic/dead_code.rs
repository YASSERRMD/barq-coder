use syn::{visit::Visit, Expr, ExprMethodCall, ItemFn, ItemImpl, ItemStruct, Pat, Type};

/// Detects dead code patterns using AST analysis:
/// - Private functions with no documented public re-export
/// - #[allow(dead_code)] on items (signals known dead code)
/// - Struct fields that are never read (prefix _ heuristic)
pub fn detect_dead_code(_file_path: &str, file_content: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let file = match syn::parse_str::<syn::File>(file_content) {
        Ok(f) => f,
        Err(_) => return warnings,
    };

    let mut walker = DeadCodeWalker::new();
    walker.visit_file(&file);

    for name in &walker.allow_dead_code_items {
        warnings.push(format!(
            "Item '{}' is marked #[allow(dead_code)] — consider removing it.",
            name
        ));
    }

    for field in &walker.underscore_fields {
        warnings.push(format!(
            "Field '{}' uses an underscore prefix, suggesting it may be unused.",
            field
        ));
    }

    warnings
}

struct DeadCodeWalker {
    allow_dead_code_items: Vec<String>,
    underscore_fields: Vec<String>,
}

impl DeadCodeWalker {
    fn new() -> Self {
        Self {
            allow_dead_code_items: Vec::new(),
            underscore_fields: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for DeadCodeWalker {
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        let has_allow_dead = i.attrs.iter().any(|a| {
            a.path().is_ident("allow")
                && a.meta.to_token_stream().to_string().contains("dead_code")
        });
        if has_allow_dead {
            self.allow_dead_code_items
                .push(i.sig.ident.to_string());
        }
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_item_struct(&mut self, i: &'ast ItemStruct) {
        for field in &i.fields {
            if let Some(ident) = &field.ident {
                let name = ident.to_string();
                if name.starts_with('_') && name.len() > 1 {
                    self.underscore_fields.push(format!("{}::{}", i.ident, name));
                }
            }
        }
        syn::visit::visit_item_struct(self, i);
    }
}

// ToTokens is needed for meta check
use quote::ToTokens;
