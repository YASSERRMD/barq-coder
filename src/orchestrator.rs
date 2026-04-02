use crate::agent::{Message, OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::tools::ToolRegistry;
use crate::context::{auto_compact, ContextBudget};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum OrchestratorEvent {
    Token(String),
    ToolCall { name: String, args: Value },
    ToolResult { name: String, result: Value },
    Done(String),
    Error(String),
}

pub struct Orchestrator {
    pub agent: OllamaClient,
    pub tools: Arc<ToolRegistry>,
    pub barq: Arc<BarqIndex>,
    pub config: Config,
    pub conversation: Vec<Message>,
    pub total_tokens: usize,
    pub budget: ContextBudget,
}

impl Orchestrator {
    pub fn new(
        agent: OllamaClient,
        tools: Arc<ToolRegistry>,
        barq: Arc<BarqIndex>,
        config: Config,
    ) -> Self {
        let token_limit = config.token_limit;
        Self {
            agent,
            tools,
            barq,
            config,
            conversation: Vec::new(),
            total_tokens: 0,
            budget: ContextBudget::new(token_limit as usize),
        }
    }

    /// Build the system prompt with BARQ context injected.
    fn build_system_prompt(&self, user_input: &str) -> String {
        // Step 1: query BARQDB for relevant context
        let barq_results = self.barq.query(user_input, 10);
        let mut context_str = String::new();
        for r in barq_results {
            context_str.push_str(&format!("{}:\n{}\n", r.file_path, r.content));
        }

        // Step 2: query GraphDB for dependency context
        let graph_deps = self.barq.graph_deps("main");
        let deps_str = graph_deps.join(", ");

        // Step 3: build tool descriptions for the system prompt
        let mut tool_desc = String::new();
        for tool in &self.tools.tools {
            tool_desc.push_str(&format!("- {}: {}\n", tool.name(), tool.description()));
        }

        format!(
            "You are BarqCoder, a high-performance Rust coding agent powered by BARQDB semantic search.\n\
            \n\
            AVAILABLE TOOLS:\n\
            {}\n\
            \n\
            CONTEXT FROM BARQDB:\n\
            {}\n\
            \n\
            GRAPH DEPENDENCIES:\n\
            {}\n\
            \n\
            RULES:\n\
            1. ALWAYS reference the BARQ context before suggesting code changes.\n\
            2. Use tools in this order: barq_search -> edit_file -> cargo_check.\n\
            3. NEVER apply edits without running cargo_check afterward.\n\
            4. If cargo_check fails, fix errors before giving a final answer.\n\
            5. When you need to use a tool, respond with tool_calls in the message.\n\
            6. When the task is complete and verified, provide your final answer as plain text.",
            tool_desc, context_str, deps_str
        )
    }

    /// Build tool schemas in the Ollama tools format.
    fn build_tool_schemas(&self) -> Vec<Value> {
        self.tools
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.schema()
                    }
                })
            })
            .collect()
    }

    /// Estimate token count for a string (rough: 1 token per 4 chars).
    fn estimate_tokens(text: &str) -> usize {
        text.len() / 4
    }

    /// Run the agent loop. Returns a channel that emits OrchestratorEvents.
    pub fn run(&mut self, user_input: &str) -> mpsc::Receiver<OrchestratorEvent> {
        let (tx, rx) = mpsc::channel(256);

        // Build system prompt with fresh context
        let sys_prompt = self.build_system_prompt(user_input);

        // Initialize conversation if empty
        if self.conversation.is_empty() {
            self.conversation.push(Message {
                role: "system".to_string(),
                content: sys_prompt.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Add user message
        self.conversation.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        // Update token estimate
        self.total_tokens = crate::context::total_tokens(&self.conversation);

        // Auto-compact if exceeding budget threshold
        if self.budget.needs_compact(&self.conversation) {
            auto_compact(&mut self.conversation, 10); // Keep last 10 messages
            self.total_tokens = crate::context::total_tokens(&self.conversation);
        }

        // Convert to ChatMessage format for the API
        let messages: Vec<rusty_ollama::ChatMessage> = self
            .conversation
            .iter()
            .map(|m| m.to_chat_message())
            .collect();

        let tool_schemas = self.build_tool_schemas();
        let tools = Arc::clone(&self.tools);
        let max_iterations = self.config.max_iterations;
        let agent = self.agent.clone();

        // Spawn the agent loop
        tokio::spawn(async move {
            let mut current_messages = messages;
            let mut iteration: u8 = 0;

            loop {
                if iteration >= max_iterations {
                    let _ = tx
                        .send(OrchestratorEvent::Error(format!(
                            "Max iterations ({}) reached. Stopping.",
                            max_iterations
                        )))
                        .await;
                    break;
                }

                iteration += 1;

                // Call the LLM
                let schemas = if tool_schemas.is_empty() {
                    None
                } else {
                    Some(tool_schemas.clone())
                };
                let mut stream_rx = agent.chat_stream(current_messages.clone(), schemas);

                let mut assistant_text = String::new();
                let mut pending_tool_calls: Vec<rusty_ollama::ToolCallResponse> = Vec::new();

                // Consume the stream
                while let Some(event) = stream_rx.recv().await {
                    match event {
                        StreamEvent::Token(text) => {
                            let _ = tx.send(OrchestratorEvent::Token(text.clone())).await;
                            assistant_text.push_str(&text);
                        }
                        StreamEvent::ToolCall(tc) => {
                            let _ = tx
                                .send(OrchestratorEvent::ToolCall {
                                    name: tc.function.name.clone(),
                                    args: tc.function.arguments.clone(),
                                })
                                .await;
                            pending_tool_calls.push(tc);
                        }
                        StreamEvent::Done => break,
                        StreamEvent::Error(e) => {
                            let _ = tx.send(OrchestratorEvent::Error(e)).await;
                            return;
                        }
                    }
                }

                // No tool calls — the model gave a final answer
                if pending_tool_calls.is_empty() {
                    let _ = tx.send(OrchestratorEvent::Done(assistant_text)).await;
                    return;
                }

                // Append assistant message with tool calls
                current_messages.push(rusty_ollama::ChatMessage {
                    role: "assistant".to_string(),
                    content: assistant_text,
                    tool_calls: Some(pending_tool_calls.clone()),
                });

                // Execute each tool call and append results
                for tc in &pending_tool_calls {
                    let tool_name = &tc.function.name;
                    let tool_args = &tc.function.arguments;

                    let result = if let Some(tool) = tools.get(tool_name) {
                        match tool.call(tool_args.clone()).await {
                            Ok(result) => {
                                let _ = tx
                                    .send(OrchestratorEvent::ToolResult {
                                        name: tool_name.clone(),
                                        result: result.clone(),
                                    })
                                    .await;
                                result
                            }
                            Err(e) => {
                                let err_val = serde_json::json!({"error": e.to_string()});
                                let _ = tx
                                    .send(OrchestratorEvent::ToolResult {
                                        name: tool_name.clone(),
                                        result: err_val.clone(),
                                    })
                                    .await;
                                err_val
                            }
                        }
                    } else {
                        let err_val =
                            serde_json::json!({"error": format!("Unknown tool: {}", tool_name)});
                        let _ = tx
                            .send(OrchestratorEvent::ToolResult {
                                name: tool_name.clone(),
                                result: err_val.clone(),
                            })
                            .await;
                        err_val
                    };

                    // Append tool result as a "tool" role message
                    current_messages.push(rusty_ollama::ChatMessage {
                        role: "tool".to_string(),
                        content: serde_json::to_string(&result).unwrap_or_default(),
                        tool_calls: None,
                    });
                }

                // Loop back to call the LLM with the tool results
            }
        });

        rx
    }
}
