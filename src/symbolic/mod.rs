use syn::visit::Visit;
use syn::ItemFn;

pub mod rules;
pub mod unsafe_check;
pub mod borrow_hint;
pub mod dead_code;
pub mod type_check;
pub mod cycle_detect;
pub mod security;
pub mod perf;

pub struct AstWalker {
    pub unsafe_blocks: Vec<syn::ExprUnsafe>,
    pub loops: Vec<syn::ExprLoop>,
    pub clones: usize,
}

impl AstWalker {
    pub fn new() -> Self {
        Self {
            unsafe_blocks: Vec::new(),
            loops: Vec::new(),
            clones: 0,
        }
    }
}

impl<'ast> Visit<'ast> for AstWalker {
    fn visit_expr_unsafe(&mut self, i: &'ast syn::ExprUnsafe) {
        self.unsafe_blocks.push(i.clone());
        syn::visit::visit_expr_unsafe(self, i);
    }

    fn visit_expr_loop(&mut self, i: &'ast syn::ExprLoop) {
        self.loops.push(i.clone());
        syn::visit::visit_expr_loop(self, i);
    }
    
    fn visit_expr_method_call(&mut self, i: &'ast syn::ExprMethodCall) {
        if i.method == "clone" {
            self.clones += 1;
        }
        syn::visit::visit_expr_method_call(self, i);
    }
}
