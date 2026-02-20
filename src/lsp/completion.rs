use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use super::Backend;

pub async fn handle_completion(_backend: &Backend, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
    Ok(Some(CompletionResponse::Array(vec![
        CompletionItem::new_simple("barq_suggest".to_string(), "BarqCoder AI Suggestion".to_string())
    ])))
}
