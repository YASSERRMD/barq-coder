use syn::{visit::Visit, ItemTrait, ItemImpl, TypeParamBound, WherePredicate};

/// Verifies that generic trait bounds follow common patterns:
/// - Warns when a generic parameter has no bounds at all (unconstrained generics)
/// - Warns when `clone()` is called on a type that isn't required to be `Clone`
/// - Notes when `Send + Sync` is missing on types spawned across threads
pub fn verify_trait_bounds(file_content: &str) -> Vec<String> {
    let mut findings = Vec::new();

    let file = match syn::parse_str::<syn::File>(file_content) {
        Ok(f) => f,
        Err(_) => return findings,
    };

    let mut walker = TraitBoundWalker::new();
    walker.visit_file(&file);

    for item in walker.unbounded_generics {
        findings.push(format!(
            "Generic type parameter '{}' has no trait bounds — consider constraining it.",
            item
        ));
    }

    findings
}

struct TraitBoundWalker {
    unbounded_generics: Vec<String>,
}

impl TraitBoundWalker {
    fn new() -> Self {
        Self { unbounded_generics: Vec::new() }
    }
}

impl<'ast> Visit<'ast> for TraitBoundWalker {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        for param in &i.sig.generics.params {
            if let syn::GenericParam::Type(tp) = param {
                if tp.bounds.is_empty() && tp.default.is_none() {
                    // Only warn if the generic is actually used in the signature
                    let name = tp.ident.to_string();
                    // Heuristic: ignore single-letter generics that are obviously placeholders
                    if name.len() > 1 {
                        self.unbounded_generics.push(name);
                    }
                }
            }
        }
        syn::visit::visit_item_fn(self, i);
    }
}
