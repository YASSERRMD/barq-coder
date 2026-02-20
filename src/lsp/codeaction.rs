use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use super::Backend;

pub async fn handle_code_action(_backend: &Backend, _params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let action = CodeAction {
        title: "Ask BarqCoder".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        ..Default::default()
    };
    Ok(Some(vec![CodeActionOrCommand::CodeAction(action)]))
}
