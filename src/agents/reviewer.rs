use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use serde_json::Value;
use std::sync::Arc;

/// Extract the boolean `approved` field from a JSON LLM response.
/// Searches for a JSON object in the response text (LLMs sometimes wrap JSON
/// in prose), parses it with serde_json, and reads the `approved` field.
/// Returns `false` if the JSON is malformed or the field is absent/non-bool.
fn extract_approval_from_response(response: &str) -> bool {
    // Find the outermost JSON object in the response
    let start = response.find('{');
    let end = response.rfind('}');
    let (Some(s), Some(e)) = (start, end) else {
        return false;
    };
    if s > e {
        return false;
    }
    let json_slice = &response[s..=e];
    serde_json::from_str::<Value>(json_slice)
        .ok()
        .and_then(|v| v.get("approved").and_then(Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::extract_approval_from_response;

    #[test]
    fn extracts_approved_true_from_clean_json() {
        assert!(extract_approval_from_response(r#"{"approved": true, "feedback": "LGTM"}"#));
    }

    #[test]
    fn extracts_approved_false_from_clean_json() {
        assert!(!extract_approval_from_response(r#"{"approved": false, "feedback": "needs work"}"#));
    }

    #[test]
    fn extracts_approved_from_json_wrapped_in_prose() {
        let response = "Sure! Here is my review:\n{\"approved\": true, \"feedback\": \"Good\"}\nHope that helps.";
        assert!(extract_approval_from_response(response));
    }

    #[test]
    fn returns_false_for_malformed_json() {
        assert!(!extract_approval_from_response("approved: true"));
        assert!(!extract_approval_from_response(""));
        assert!(!extract_approval_from_response("{broken json"));
    }

    #[test]
    fn returns_false_when_approved_field_missing() {
        assert!(!extract_approval_from_response(r#"{"feedback": "looks good"}"#));
    }
}

pub struct ReviewerAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
}

impl ReviewerAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>) -> Self {
        Self { llm, barq }
    }

    pub async fn review_diff(&self, step_id: &str, diff: &str) -> anyhow::Result<bool> {
        let prompt = format!(
            "Review the following diff for step {}:\n\n{}\n\nDoes this code meet quality, security, and performance standards? Reply with strictly JSON containing a 'approved' boolean field and a 'feedback' string.",
            step_id, diff
        );

        let messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Reviewer.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            },
        ];

        let mut rx = self.llm.chat_stream(messages, None);

        let mut full_response = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(text) => {
                    full_response.push_str(&text);
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    return Err(anyhow::anyhow!("LLM error: {}", e));
                }
                _ => {}
            }
        }

        // Try to parse the LLM response as JSON and extract the "approved" field.
        // Fall back to conservative approval (false) if the JSON is malformed so
        // a bad LLM response never silently approves a bad diff.
        let approved = extract_approval_from_response(&full_response);

        Ok(approved)
    }
}
