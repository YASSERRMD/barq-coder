use crate::barq::BarqIndex;
use std::sync::Arc;

/// Deep Symbolic RAG Injector
///
/// Unlike standard vector search that just returns textually similar chunks,
/// the Symbolic Injector parses the user's intent to find target symbols
/// (e.g., function names) and queries the graph database (BarqIndex)
/// to fetch AST-based upstream callers. This guarantees that the agent
/// understands the blast radius and usage patterns of any modified code.
pub struct SymbolicInjector {
    barq: Arc<BarqIndex>,
}

impl SymbolicInjector {
    pub fn new(barq: Arc<BarqIndex>) -> Self {
        Self { barq }
    }

    /// Injects symbolic caller graphs into standard text context.
    pub fn inject(&self, user_query: &str) -> String {
        let mut injection = String::new();

        // Very basic heuristic: if the query looks like it mentions a function
        // (words ending with () or pascal/camel case), we try to fetch callers.
        let potential_symbols: Vec<&str> = user_query
            .split_whitespace()
            .filter(|w| w.ends_with("()") || w.contains('_') || w.chars().any(|c| c.is_uppercase()))
            .map(|w| w.trim_matches(|c: char| c == '(' || c == ')' || c == '`' || c == '\''))
            .collect();

        if !potential_symbols.is_empty() {
            injection.push_str("\n--- Deep Symbolic Context (Caller Graph) ---\n");
            for symbol in potential_symbols {
                let callers = self.barq.get_callers(symbol);
                if !callers.is_empty() {
                    injection.push_str(&format!("Symbol `{}` is called by:\n", symbol));
                    for caller in callers {
                        injection.push_str(&format!("  - {}\n", caller));
                    }
                }
            }
            injection.push_str("--------------------------------------------\n");
        }

        injection
    }
}
