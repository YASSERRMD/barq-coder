use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use std::sync::Arc;

pub struct CoderAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
}

impl CoderAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, barq, tools }
    }

    pub async fn implement_step(&self, step_id: &str, description: &str) -> anyhow::Result<String> {
        let context = self.barq.query(description, 5);
        let mut context_str = String::new();
        for res in context {
            context_str.push_str(&format!("File: {}\nContent:\n{}\n\n", res.file_path, res.content));
        }

        let prompt = format!(
            "Step ID: {}\nDescription: {}\n\nContext:\n{}\n\nImplement this step. You have access to tools. Write files, run cargo checks, and execute shell commands to fulfill the requirement. State what you have done.",
            step_id, description, context_str
        );

        let mut messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Coder.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            }
        ];

        let tool_schemas = self.tools.schemas();
        let mut final_response = String::new();
        let max_iterations = 7;

        for _ in 0..max_iterations {
            let mut rx = self.llm.chat_stream(messages.clone(), Some(tool_schemas.clone()));
            
            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();
            
            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Token(text) => iter_response.push_str(&text),
                    StreamEvent::ToolCall(t) => tool_calls.push(t),
                    StreamEvent::Done => break,
                    StreamEvent::Error(e) => return Err(anyhow::anyhow!("Coder LLM error: {}", e)),
                    _ => {}
                }
            }
            
            if tool_calls.is_empty() {
                final_response = iter_response;
                break;
            } else {
                messages.push(rusty_ollama::ChatMessage {
                    role: "assistant".to_string(),
                    content: iter_response,
                    tool_calls: Some(tool_calls.clone()),
                });
                
                for tc in tool_calls {
                    let mut tool_result = String::new();
                    if let Some(tool) = self.tools.get(&tc.function.name) {
                        let parsed_args = tc.function.arguments.clone();
                        match tool.call(parsed_args).await {
                            Ok(res) => tool_result = res.to_string(),
                            Err(e) => tool_result = format!("Error executing tool: {}", e),
                        }
                    } else {
                        tool_result = format!("Tool not found: {}", tc.function.name);
                    }
                    
                    messages.push(rusty_ollama::ChatMessage {
                        role: "tool".to_string(),
                        content: tool_result,
                        tool_calls: None, // ollama tool responses dont have tool calls
                    });
                }
            }
        }

        Ok(final_response)
    }
}
