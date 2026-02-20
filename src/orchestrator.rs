use crate::agent::{Message, OllamaClient, parse_response};
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::tools::ToolRegistry;
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
    pub tools: ToolRegistry,
    pub barq: Arc<BarqIndex>,
    pub config: Config,
    pub conversation: Vec<Message>,
}

impl Orchestrator {
    pub fn new(
        agent: OllamaClient,
        tools: ToolRegistry,
        barq: Arc<BarqIndex>,
        config: Config,
    ) -> Self {
        Self {
            agent,
            tools,
            barq,
            config,
            conversation: Vec::new(),
        }
    }

    pub fn run(&mut self, user_input: &str) -> mpsc::Receiver<OrchestratorEvent> {
        let (tx, rx) = mpsc::channel(100);

        // Step 1: query BARQDB for top 10 context results
        let barq_results = self.barq.query(user_input, 10);
        let mut context_str = String::new();
        for r in barq_results {
            context_str.push_str(&format!("{}:\n{}\n", r.file_path, r.content));
        }

        // Step 2: query GraphDB for deps (using dummy symbol for now)
        let graph_deps = self.barq.graph_deps("main");
        let deps_str = graph_deps.join(", ");

        // Step 3: build system prompt
        let sys_prompt = format!(
            "You are BarqCoder, elite Rust/Go coding agent powered by BARQDB semantic search.\n\
             \n\
             CONTEXT FROM BARQDB:\n\
             {}\n\
             \n\
             GRAPH DEPENDENCIES:\n\
             {}\n\
             \n\
             RULES:\n\
             1. ALWAYS reference BARQ context before suggesting code\n\
             2. Use tools in this order: barq_search -> edit_file -> cargo_check\n\
             3. NEVER apply edits without running cargo_check after\n\
             4. If cargo_check fails, fix errors before final_answer\n\
             5. Respond ONLY as valid JSON matching this schema:\n\
             {{\n\
               \"reasoning\": \"string (max 5 bullets)\",\n\
               \"tool_calls\": [{{\"name\": \"string\", \"arguments\": {{}}}}],\n\
               \"final_answer\": \"string | null\"\n\
             }}\n\
             6. final_answer ONLY when task is complete and verified",
             context_str, deps_str
        );

        if self.conversation.is_empty() {
            self.conversation.push(Message {
                role: "system".to_string(),
                content: sys_prompt,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Step 4: add user message to conversation
        self.conversation.push(Message {
            role: "user".to_string(),
            content: user_input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        // Mock event stream behavior for compilation
        tokio::spawn(async move {
            let _ = tx.send(OrchestratorEvent::Token("Thinking...".to_string())).await;
            let _ = tx.send(OrchestratorEvent::Done("I am ready to help.".to_string())).await;
        });

        rx
    }
}
