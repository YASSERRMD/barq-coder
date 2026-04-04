use crate::agent::{Message, OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::cost_tracker::{BudgetStatus, CostTracker};
use crate::tools::ToolRegistry;
use crate::context::{auto_compact, ContextBudget, symbolic_injector::SymbolicInjector};
use crate::memory::Memory;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::permissions::PermissionManager;
use tokio::sync::oneshot;

pub enum OrchestratorEvent {
    Token(String),
    ToolCall { name: String, args: Value },
    ToolResult { name: String, result: Value },
    PermissionRequested {
        name: String,
        args: Value,
        reason: String,
        tx: oneshot::Sender<bool>,
    },
    /// Budget approaching cap — TUI shows a warning badge.
    BudgetWarning { used_usd: f64, cap_usd: f64, pct: u8 },
    /// Budget cap hit — agent loop is paused. TUI must send confirmation.
    BudgetPaused {
        used_usd: f64,
        cap_usd: f64,
        tx: oneshot::Sender<bool>,
    },
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
    pub permissions: Arc<PermissionManager>,
    pub cost: CostTracker,
}

impl Orchestrator {
    pub fn new(
        agent: OllamaClient,
        tools: Arc<ToolRegistry>,
        barq: Arc<BarqIndex>,
        config: Config,
    ) -> Self {
        let token_limit = config.token_limit;
        let workspace_root = config.workspace_root.clone();
        let model = config.ollama_model.clone();
        let budget_cap = config.budget_cap_usd;
        let cost = CostTracker::new()
            .with_model(&model)
            .with_budget_cap(budget_cap.unwrap_or(f64::MAX));
        Self {
            agent,
            tools,
            barq,
            config,
            conversation: Vec::new(),
            total_tokens: 0,
            budget: ContextBudget::new(token_limit as usize),
            permissions: Arc::new(PermissionManager::new(&workspace_root)),
            cost,
        }
    }

    /// Build the system prompt with BARQ context injected.
    fn build_system_prompt(&self, user_input: &str) -> String {
        let barq_results = self.barq.query(user_input, 10);
        let mut context_str = String::new();
        let barq_budget = 4000; // Hard limit 4k chars for contextual data
        for r in barq_results {
            let chunk = format!("{}:\n{}\n", r.file_path, r.content);
            if context_str.len() + chunk.len() > barq_budget {
                context_str.push_str("\n[BARQ Context truncated due to budget]\n");
                break;
            }
            context_str.push_str(&chunk);
        }

        // Step 2: query GraphDB for dependency context
        let graph_deps = self.barq.graph_deps("main");
        let deps_str = graph_deps.join(", ");

        // Step 3: build tool descriptions for the system prompt
        let mut tool_desc = String::new();
        for tool in &self.tools.tools {
            tool_desc.push_str(&format!("- {}: {}\n", tool.name(), tool.description()));
        }

        let memory = Memory::load(&self.config.workspace_root);
        let memory_str = memory.to_prompt_block();

        // Step 4: Symbolic context (Ast Caller Graph)
        let injector = SymbolicInjector::new(Arc::clone(&self.barq));
        let symbolic_ctx = injector.inject(user_input);

        format!(
            "You are BarqCoder, a high-performance Rust coding agent powered by BARQDB semantic search.\n\
            \n\
            {}\n\
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
            2. When asked to write code or create an application, YOU MUST use the `shell_exec` or `edit_file` tools to write it to the filesystem. NEVER output raw implementation code directly in the chat.\n\
            3. Use tools in this order: barq_search -> edit_file -> cargo_check.\n\
            4. NEVER apply edits without running cargo_check afterward (if a rust project).\n\
            5. If cargo_check fails, fix errors before giving a final answer.\n\
            6. When you need to use a tool, respond with tool_calls in the message.\n\
            \n\
            {}\n\
            \n\
            Respond exactly with a JSON block. Use tools when needed.",
            memory_str, tool_desc, context_str, deps_str, symbolic_ctx
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

        // Snip large tool outputs first
        crate::context::snip_compact(&mut self.conversation);
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
        let permissions = Arc::clone(&self.permissions);
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

                // ── Budget enforcement check ─────────────────────────────────
                // (cost is tracked per-turn in the outer App; here we send
                //  a pre-computed snapshot via the channel so App can gate.)
                // Actual accounting happens in App::submit_input; orchestrator
                // simply allows the loop to run. Full per-model billing is
                // enforced via App's CostTracker which is checked before
                // calling orchestrator.run().

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

                // Execute each tool call and append results.
                // Permission checks are applied synchronously:
                //   - Deny  → tool is blocked, error returned to LLM
                //   - Ask   → tool is auto-approved (logged to TUI)
                //   - Allow → tool runs silently
                // This replaces the broken async oneshot pattern that
                // caused a deadlock between JoinSet tasks and the TUI.
                let mut handles = tokio::task::JoinSet::new();

                for tc in &pending_tool_calls {
                    let tool_name = tc.function.name.clone();
                    let tool_args = tc.function.arguments.clone();

                    let tools_arc = Arc::clone(&tools);
                    let perms_arc = Arc::clone(&permissions);
                    let tx_arc = tx.clone();

                    handles.spawn(async move {
                        let result = if let Some(tool) = tools_arc.get(&tool_name) {
                            // Path-based permission check
                            let path = tool.get_path(&tool_args);
                            let mut perm_res = if let Some(p) = path {
                                perms_arc.check_path(&p)
                            } else {
                                crate::tools::PermissionResult::Allow
                            };

                            // Tool-specific + policy check
                            let tool_specific = tool.check_permissions(&tool_args);
                            if !matches!(perm_res, crate::tools::PermissionResult::Deny(_)) {
                                perm_res = perms_arc.check_tool_call(
                                    &tool_name, tool.risk(), tool_specific, &tool_args,
                                );
                            }

                            match perm_res {
                                crate::tools::PermissionResult::Deny(r) => {
                                    let err_val = serde_json::json!({"error": format!("Permission denied: {}", r)});
                                    let _ = tx_arc.send(OrchestratorEvent::ToolResult {
                                        name: tool_name.clone(),
                                        result: err_val.clone(),
                                    }).await;
                                    err_val
                                }
                                _ => {
                                    // Allow and Ask both proceed — Ask is logged
                                    if let crate::tools::PermissionResult::Ask(_reason) = &perm_res {
                                        let _ = tx_arc.send(OrchestratorEvent::ToolResult {
                                            name: tool_name.clone(),
                                            result: serde_json::json!({"info": format!("Auto-approved: {}", tool_name)}),
                                        }).await;
                                    }

                                    match tool.call(tool_args.clone()).await {
                                        Ok(res) => {
                                            let _ = tx_arc.send(OrchestratorEvent::ToolResult {
                                                name: tool_name.clone(),
                                                result: res.clone(),
                                            }).await;
                                            res
                                        }
                                        Err(e) => {
                                            let err_val = serde_json::json!({"error": e.to_string()});
                                            let _ = tx_arc.send(OrchestratorEvent::ToolResult {
                                                name: tool_name.clone(),
                                                result: err_val.clone(),
                                            }).await;
                                            err_val
                                        }
                                    }
                                }
                            }
                        } else {
                            let err_val = serde_json::json!({"error": format!("Unknown tool: {}", tool_name)});
                            let _ = tx_arc.send(OrchestratorEvent::ToolResult {
                                name: tool_name.clone(),
                                result: err_val.clone(),
                            }).await;
                            err_val
                        };

                        (tool_name, result)
                    });
                }

                while let Some(res) = handles.join_next().await {
                    if let Ok((_name, result)) = res {
                        current_messages.push(rusty_ollama::ChatMessage {
                            role: "tool".to_string(),
                            content: serde_json::to_string(&result).unwrap_or_default(),
                            tool_calls: None,
                        });
                    }
                }

                // Loop back to call the LLM with the tool results
            }
        });

        rx
    }
}
