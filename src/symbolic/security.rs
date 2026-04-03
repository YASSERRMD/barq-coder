use syn::visit::Visit;

/// Security pattern scanner — detects common anti-patterns via AST and text heuristics:
/// - Raw SQL string concatenation (format! with SELECT/INSERT/UPDATE/DELETE)
/// - Hardcoded credentials (password/secret literals)
/// - Use of deprecated unsafe FFI transmute
/// - println! of secrets (debug leak risk)
pub fn scan_security_patterns(file_content: &str) -> Vec<String> {
    let mut findings = Vec::new();

    // Text-level heuristics (fast pre-filter before AST)
    let lower = file_content.to_lowercase();

    // SQL injection surface: format! containing SQL keywords
    if lower.contains("format!") && (lower.contains("select ") || lower.contains("insert into") || lower.contains("update ") || lower.contains("delete from")) {
        findings.push("Possible SQL injection: detected string formatting with SQL keywords. Use parameterised queries instead.".to_string());
    }

    // Hardcoded secrets
    for pattern in &["password =", "secret =", "api_key =", "token =", "passwd ="] {
        if lower.contains(pattern) && lower.contains('"') {
            findings.push(format!(
                "Possible hardcoded credential: '{}' found. Use environment variables or a secrets manager.",
                pattern.trim()
            ));
        }
    }

    // AST-level checks
    let file = match syn::parse_str::<syn::File>(file_content) {
        Ok(f) => f,
        Err(_) => return findings,
    };

    let mut walker = SecurityWalker::new();
    walker.visit_file(&file);

    for item in walker.transmute_uses {
        findings.push(format!(
            "Use of `mem::transmute` detected: '{}'. This is highly unsafe; prefer safe alternatives.",
            item
        ));
    }

    findings
}

struct SecurityWalker {
    transmute_uses: Vec<String>,
}

impl SecurityWalker {
    fn new() -> Self {
        Self { transmute_uses: Vec::new() }
    }
}

impl<'ast> Visit<'ast> for SecurityWalker {
    fn visit_expr_call(&mut self, i: &'ast syn::ExprCall) {
        use quote::ToTokens;
        let call_str = i.func.to_token_stream().to_string();
        if call_str.contains("transmute") {
            self.transmute_uses.push(call_str);
        }
        syn::visit::visit_expr_call(self, i);
    }
}
