use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use super::Backend;

pub async fn handle_hover(_backend: &Backend, _params: HoverParams) -> Result<Option<Hover>> {
    Ok(Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String("BarqCoder: Hover info from BARQ".to_string())),
        range: None,
    }))
}
