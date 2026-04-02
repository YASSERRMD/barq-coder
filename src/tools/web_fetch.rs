use super::{Tool, ToolMetadata};
use async_trait::async_trait;
use serde_json::{json, Value};

/// WebFetchTool — fetch content from a URL (like Claude Code's WebFetchTool).
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &'static str { "web_fetch" }

    fn description(&self) -> &'static str {
        "Fetch content from a URL and return it as text"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "max_bytes": { "type": "number", "description": "Maximum bytes to read (default 100000)" }
            },
            "required": ["url"]
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            timeout_secs: 15,
            search_hint: Some("http url download documentation api".to_string()),
            ..Default::default()
        }
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let max_bytes = args.get("max_bytes").and_then(|v| v.as_u64()).unwrap_or(100_000) as usize;

        if url.is_empty() {
            return Err(anyhow::anyhow!("URL is required"));
        }

        // Use reqwest (already a dependency via rusty_ollama)
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let response = client.get(url).send().await?;
        let status = response.status().as_u16();

        if !response.status().is_success() {
            return Ok(json!({
                "success": false,
                "status": status,
                "error": format!("HTTP {}", status)
            }));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let bytes = response.bytes().await?;
        let body = if bytes.len() > max_bytes {
            String::from_utf8_lossy(&bytes[..max_bytes]).to_string()
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        Ok(json!({
            "success": true,
            "status": status,
            "content_type": content_type,
            "content": body,
            "size_bytes": bytes.len(),
            "truncated": bytes.len() > max_bytes
        }))
    }
}
