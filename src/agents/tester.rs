use barq_ir::{AgentMessage, StreamEvent};
use crate::barq::BarqIndex;
use crate::providers::ProviderAdapter;
use crate::tools::ToolRegistry;
use crate::verifier::Verifier;
use std::sync::Arc;

pub struct TesterAgent {
    pub llm: Arc<dyn ProviderAdapter>,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
}

impl TesterAgent {
    pub fn new(llm: Arc<dyn ProviderAdapter>, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, barq, tools }
    }

    pub async fn test_step(&self, step_id: &str, impl_result: &str) -> anyhow::Result<String> {
        let verifier = Verifier::new(self.barq.clone(), ".");
        let res = verifier.verify_edit("", "", "").await;

        let all_pass = res.cargo_check_pass && res.cargo_test_pass;
        if all_pass {
            return Ok(format!("Step ID: {}\nAll tests and build checks passed.", step_id));
        }

        let prompt = format!(
            "Step ID: {}\nImplementation Result: {}\n\nVerification failed. \
             Use the tools to debug and fix the tests. Errors:\n{:?}",
            step_id, impl_result, res.errors
        );

        let messages = vec![
            AgentMessage::system(crate::agents::AgentRole::Tester.system_prompt()),
            AgentMessage::user(prompt),
        ];

        let tool_schemas = self.tools.schemas();
        let mut current_messages = messages;
        let mut final_response = String::new();
        let max_iterations = 5;

        for _ in 0..max_iterations {
            let mut rx = self.llm.chat_stream(current_messages.clone(), Some(tool_schemas.clone()));
            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::TextDelta { content } => iter_response.push_str(&content),
                    StreamEvent::ToolCallDone { payload } => tool_calls.push(payload),
                    StreamEvent::Finish { .. } => break,
                    StreamEvent::Error { message, .. } => {
                        return Err(anyhow::anyhow!("Tester LLM error: {}", message))
                    }
                    _ => {}
                }
            }

            if tool_calls.is_empty() {
                final_response = iter_response;
                break;
            }

            current_messages.push(AgentMessage::assistant_with_tools(
                iter_response,
                tool_calls.clone(),
            ));

            for tc in tool_calls {
                let tool_result = if let Some(tool) = self.tools.get(&tc.name) {
                    match tool.call(tc.arguments.clone()).await {
                        Ok(res) => res.to_string(),
                        Err(e) => format!("Error executing tool: {}", e),
                    }
                } else {
                    format!("Tool not found: {}", tc.name)
                };

                current_messages.push(AgentMessage::tool_result(tc.id, tool_result));
            }
        }

        Ok(final_response)
    }
}
