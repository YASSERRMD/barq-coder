use barq_ir::{AgentMessage, MessageRole, StreamEvent, ToolCallPayload};
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::context::{auto_compact, ContextBudget, symbolic_injector::SymbolicInjector};
use crate::cost_tracker::CostTracker;
use crate::memory::Memory;
use crate::permissions::PermissionManager;
use crate::providers::ProviderAdapter;
use crate::tools::ToolRegistry;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

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
    BudgetWarning { used_usd: f64, cap_usd: f64, pct: u8 },
    BudgetPaused { used_usd: f64, cap_usd: f64, tx: oneshot::Sender<bool> },
    Done(String),
    Error(String),
}

pub struct Orchestrator {
    pub agent: Arc<dyn ProviderAdapter>,
    pub tools: Arc<ToolRegistry>,
    pub barq: Arc<BarqIndex>,
    pub config: Config,
    pub conversation: Vec<AgentMessage>,
    pub total_tokens: usize,
    pub budget: ContextBudget,
    pub permissions: Arc<PermissionManager>,
    pub cost: CostTracker,
}

impl Orchestrator {
    pub fn new(
        agent: Arc<dyn ProviderAdapter>,
        tools: Arc<ToolRegistry>,
        barq: Arc<BarqIndex>,
        config: Config,
    ) -> Self {
        let token_limit = config.token_limit;
        let workspace_root = config.workspace_root.clone();
        let model = agent.model_id().to_string();
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

    fn build_system_prompt(&self, user_input: &str) -> String {
        let barq_results = self.barq.query(user_input, 10);
        let mut context_str = String::new();
        let barq_budget = 4000;
        for r in barq_results {
            let chunk = format!("{}:\n{}\n", r.file_path, r.content);
            if context_str.len() + chunk.len() > barq_budget {
                context_str.push_str("\n[BARQ Context truncated due to budget]\n");
                break;
            }
            context_str.push_str(&chunk);
        }

        let graph_deps = self.barq.graph_deps("main");
        let deps_str = graph_deps.join(", ");

        let tier = self.agent.trust_tier();
        let mut tool_desc = String::new();
        for tool in &self.tools.tools {
            if tier.permits_tool(tool.name()) {
                tool_desc.push_str(&format!("- {}: {}\n", tool.name(), tool.description()));
            }
        }

        let memory = Memory::load(&self.config.workspace_root);
        let memory_str = memory.to_prompt_block();

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
            2. When asked to write code or create an application, YOU MUST use the `shell_exec` or `edit_file` tools to write it to the filesystem.\n\
            3. Use tools in this order: barq_search -> edit_file -> cargo_check.\n\
            4. NEVER apply edits without running cargo_check afterward (if a Rust project).\n\
            5. If cargo_check fails, fix errors before giving a final answer.\n\
            6. Call tools through the model's native tool-calling interface — do NOT print JSON wrappers in assistant text.\n\
            7. Respond in plain language. NEVER answer with a JSON envelope unless explicitly asked.\n\
            \n\
            {}\n\
            \n\
            Respond in plain language. Use tools when needed.",
            memory_str, tool_desc, context_str, deps_str, symbolic_ctx
        )
    }

    fn build_tool_schemas(&self) -> Vec<Value> {
        let caps = self.agent.capabilities();
        if !caps.can_use_tools() {
            return Vec::new();
        }
        let tier = self.agent.trust_tier();
        self.tools
            .tools
            .iter()
            .filter(|t| tier.permits_tool(t.name()))
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

    pub fn run(&mut self, user_input: &str) -> mpsc::Receiver<OrchestratorEvent> {
        let (tx, rx) = mpsc::channel(256);

        let sys_prompt = self.build_system_prompt(user_input);

        if self.conversation.is_empty() {
            self.conversation.push(AgentMessage::system(sys_prompt));
        }

        self.conversation.push(AgentMessage::user(user_input));
        self.total_tokens = crate::context::total_tokens(&self.conversation);

        crate::context::snip_compact(&mut self.conversation);
        self.total_tokens = crate::context::total_tokens(&self.conversation);

        if self.budget.needs_compact(&self.conversation) {
            auto_compact(&mut self.conversation, 10);
            self.total_tokens = crate::context::total_tokens(&self.conversation);
        }

        let messages = self.conversation.clone();
        let tool_schemas = self.build_tool_schemas();
        let tools = Arc::clone(&self.tools);
        let permissions = Arc::clone(&self.permissions);
        let max_iterations = self.config.max_iterations;
        let agent = Arc::clone(&self.agent);

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

                let schemas = if tool_schemas.is_empty() { None } else { Some(tool_schemas.clone()) };
                let mut stream_rx = agent.chat_stream(current_messages.clone(), schemas);

                let mut assistant_text = String::new();
                let mut pending_tool_calls: Vec<ToolCallPayload> = Vec::new();

                while let Some(event) = stream_rx.recv().await {
                    match event {
                        StreamEvent::TextDelta { content } => {
                            let _ = tx.send(OrchestratorEvent::Token(content.clone())).await;
                            assistant_text.push_str(&content);
                        }
                        StreamEvent::ToolCallDone { payload } => {
                            let _ = tx
                                .send(OrchestratorEvent::ToolCall {
                                    name: payload.name.clone(),
                                    args: payload.arguments.clone(),
                                })
                                .await;
                            pending_tool_calls.push(payload);
                        }
                        StreamEvent::Finish { .. } | StreamEvent::StepFinish { .. } => break,
                        StreamEvent::Error { message, .. } => {
                            let _ = tx.send(OrchestratorEvent::Error(message)).await;
                            return;
                        }
                        _ => {}
                    }
                }

                if pending_tool_calls.is_empty() {
                    let _ = tx.send(OrchestratorEvent::Done(assistant_text)).await;
                    return;
                }

                current_messages.push(AgentMessage::assistant_with_tools(
                    assistant_text,
                    pending_tool_calls.clone(),
                ));

                let mut handles = tokio::task::JoinSet::new();

                for tc in pending_tool_calls {
                    let tool_name = tc.name.clone();
                    let tool_args = tc.arguments.clone();
                    let call_id = tc.id.clone();
                    let tools_arc = Arc::clone(&tools);
                    let perms_arc = Arc::clone(&permissions);
                    let tx_arc = tx.clone();

                    handles.spawn(async move {
                        let result = if let Some(tool) = tools_arc.get(&tool_name) {
                            let path = tool.get_path(&tool_args);
                            let mut perm_res = if let Some(p) = path {
                                perms_arc.check_path(&p)
                            } else {
                                crate::tools::PermissionResult::Allow
                            };

                            let tool_specific = tool.check_permissions(&tool_args);
                            if !matches!(perm_res, crate::tools::PermissionResult::Deny(_)) {
                                perm_res = perms_arc.check_tool_call(
                                    &tool_name, tool.risk(), tool_specific, &tool_args,
                                );
                            }

                            if let crate::tools::PermissionResult::Deny(r) = perm_res {
                                serde_json::json!({"error": format!("Permission denied: {}", r)})
                            } else {
                                let mut allowed = true;
                                if let crate::tools::PermissionResult::Ask(reason) = perm_res {
                                    let (reply_tx, reply_rx) = oneshot::channel();
                                    let _ = tx_arc
                                        .send(OrchestratorEvent::PermissionRequested {
                                            name: tool_name.clone(),
                                            args: tool_args.clone(),
                                            reason,
                                            tx: reply_tx,
                                        })
                                        .await;
                                    allowed = reply_rx.await.unwrap_or(false);
                                }

                                if allowed {
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
                                } else {
                                    serde_json::json!({"error": "User denied permission."})
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

                        (call_id, result)
                    });
                }

                while let Some(res) = handles.join_next().await {
                    if let Ok((call_id, result)) = res {
                        current_messages.push(AgentMessage::tool_result(
                            call_id,
                            serde_json::to_string(&result).unwrap_or_default(),
                        ));
                    }
                }
            }
        });

        rx
    }
}
