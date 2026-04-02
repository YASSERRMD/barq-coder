#![allow(unused)]

use crossterm::{
    event::{
        self, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod agent;
mod agents;
mod barq;
mod collab;
mod config;
mod context;
mod lsp;
mod macro_goals;
mod orchestrator;
mod session;
mod symbolic;
mod tools;
mod tui;
mod verifier;
mod voice;

use agent::OllamaClient;
use barq::BarqIndex;
use config::Config;
use orchestrator::{Orchestrator, OrchestratorEvent};
use session::{SessionEvent, SessionStore};
use tools::ToolRegistry;
use agents::coordinator::CoordinatorAgent;
use tui::{ActiveTab, ChatMessage, Focus, SessionEntry, TuiState};

// ─────────────────────────────────────────────────────────────────────────────
// App: top-level state container
// ─────────────────────────────────────────────────────────────────────────────
struct App {
    tui: TuiState,
    orchestrator: Orchestrator,
    config: Config,
    coordinator: Arc<CoordinatorAgent>,
    event_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
    session_store: SessionStore,
    session_id: String,
}

impl App {
    fn new(resume_id: Option<String>) -> Self {
        let config = Config::load();
        let agent = OllamaClient::new(&config.ollama_base_url, &config.ollama_model);
        let barq = Arc::new(BarqIndex::new(&config).expect("Failed to create BarqIndex"));
        let tools = Arc::new(ToolRegistry::with_barq(Arc::clone(&barq)));
        let orchestrator = Orchestrator::new(
            agent.clone(),
            Arc::clone(&tools),
            Arc::clone(&barq),
            config.clone(),
        );
        let coordinator = Arc::new(CoordinatorAgent::new(
            agent,
            Arc::clone(&barq),
            tools,
        ));

        let session_store = SessionStore::new(&config.workspace_root);
        
        let session_id = resume_id.unwrap_or_else(|| {
            format!(
                "session_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            )
        });

        // Session ID is used for JSONL transcript appending
        let _active_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Load workspace files for sidebar
        let workspace_files = walkdir::WalkDir::new(&config.workspace_root)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let p = e.path().to_string_lossy();
                !p.contains("target/")
                    && !p.contains(".git/")
                    && e.path().extension().map_or(false, |ext| {
                        matches!(
                            ext.to_string_lossy().as_ref(),
                            "rs" | "toml" | "md" | "yaml" | "yml" | "json"
                        )
                    })
            })
            .take(80)
            .map(|e| {
                e.path()
                    .strip_prefix(&config.workspace_root)
                    .unwrap_or(e.path())
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();

        // Load saved sessions for the Sessions tab
        let saved_sessions = session_store
            .list()
            .into_iter()
            .map(|m| SessionEntry {
                id: m.id,
                created_at: m.created_at,
                event_count: m.event_count,
                workspace: m.workspace,
            })
            .collect::<Vec<_>>();

        let token_limit = config.token_limit;
        let model = config.ollama_model.clone();

        let mut tui = TuiState::new(token_limit, model, session_id.clone());
        tui.workspace_files = workspace_files;
        tui.sessions = saved_sessions;

        Self {
            tui,
            orchestrator,
            config,
            coordinator,
            event_rx: None,
            session_store,
            session_id,
        }
    }

    /// Load past session events into the UI
    fn load_session(&mut self) {
        for ev in self.session_store.replay(&self.session_id) {
            match ev {
                SessionEvent::UserInput { content, .. } => {
                    self.tui.add_message(ChatMessage::user(&content));
                }
                SessionEvent::AssistantMessage { content, .. } => {
                    self.tui.add_message(ChatMessage::agent(&content));
                }
                SessionEvent::ToolCall { name, args, .. } => {
                    self.tui.add_message(ChatMessage::tool_call(format!(
                        "{} ← {}",
                        name, args
                    )));
                }
                SessionEvent::EditApplied { file, patch, .. } => {
                    // Just show as system message for historical load
                    self.tui.add_message(ChatMessage::system(format!(
                        "Edit applied to {}",
                        file
                    )));
                }
                SessionEvent::Error { message, .. } => {
                    self.tui.add_message(ChatMessage::error(&message));
                }
                _ => {}
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Deterministic / seed mode
    if let Some(idx) = args.iter().position(|a| a == "--seed") {
        let _seed = args.get(idx + 1).and_then(|v| v.parse::<u64>().ok());
    }

    tokio::spawn(start_health_server());

    if args.iter().any(|arg| arg == "--lsp") {
        lsp::start_lsp().await;
        return Ok(());
    }

    // Check for --continue or --resume
    let mut resume_id = None;
    if args.iter().any(|arg| arg == "--continue") {
        let store = SessionStore::new(".");
        resume_id = store.last_session_id();
    } else if let Some(idx) = args.iter().position(|a| a == "--resume") {
        resume_id = args.get(idx + 1).cloned();
    }

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(resume_id.clone());
    if resume_id.is_some() {
        app.load_session();
    }
    
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;

    if let Err(err) = res {
        eprintln!("{:?}", err);
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Main event loop
// ─────────────────────────────────────────────────────────────────────────────
async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> anyhow::Result<()> {
    // Spinner / tick interval
    let tick_rate = Duration::from_millis(120);

    loop {
        terminal.draw(|f| tui::draw(f, &mut app.tui))?;

        // ── Orchestrator event drain ──────────────────────────────────────────
        if let Some(rx) = &mut app.event_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    OrchestratorEvent::Token(t) => {
                        app.tui.is_thinking = true;
                        app.tui.append_agent_token(&t);
                        let _ = app.session_store.append(&app.session_id, &SessionEvent::assistant(&t));
                    }
                    OrchestratorEvent::ToolCall { name, args } => {
                        app.tui.current_tool = Some(name.clone());
                        let entry = format!("Calling {} with {}", name, args);
                        app.tui.tool_log.push(entry.clone());
                        app.tui.add_message(ChatMessage::tool_call(&entry));
                        let _ = app.session_store.append(&app.session_id, &SessionEvent::tool_call(&name, args.clone()));
                    }
                    OrchestratorEvent::ToolResult { name, result } => {
                        app.tui.current_tool = None;
                        let entry = format!("Result for {}: {}", name, result);
                        app.tui.tool_log.push(entry.clone());
                        app.tui.add_message(ChatMessage::tool_result(&entry));
                    }
                    OrchestratorEvent::Done(answer) => {
                        app.tui.is_thinking = false;
                        app.tui.current_tool = None;
                        app.tui.add_message(ChatMessage::agent(&answer));
                        app.tui.set_status("Done", false);
                        app.event_rx = None;
                        let _ = app.session_store.append(&app.session_id, &SessionEvent::assistant(&answer));
                        break;
                    }
                    OrchestratorEvent::Error(err) => {
                        app.tui.is_thinking = false;
                        app.tui.current_tool = None;
                        app.tui.add_message(ChatMessage::error(&err));
                        app.tui.set_status(format!("Error: {}", err), true);
                        app.event_rx = None;
                        break;
                    }
                }
            }
        }

        // ── Input events ──────────────────────────────────────────────────────
        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(app, key.code, key.modifiers);
                    if app.tui.status_message.is_some() && !matches!(key.code, KeyCode::Esc) {
                        // clear transient status on next keypress
                    }
                }
                Event::Mouse(m) => handle_mouse(app, m),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        // ── Tick ──────────────────────────────────────────────────────────────
        app.tui.tick = app.tui.tick.wrapping_add(1);

        if app.tui.status_message.is_some() && app.tui.tick % 25 == 0 {
            app.tui.clear_status();
        }

        // ── Quit ──────────────────────────────────────────────────────────────
        if app.tui.should_quit() {
            return Ok(());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Key handling
// ─────────────────────────────────────────────────────────────────────────────
fn handle_key(app: &mut App, key: KeyCode, mods: KeyModifiers) {
    // Global: Esc → quit
    if key == KeyCode::Esc {
        app.tui.mark_quit();
        return;
    }

    // Tab switching: Tab / Shift+Tab
    if key == KeyCode::Tab {
        app.tui.active_tab = app.tui.active_tab.next();
        return;
    }
    if key == KeyCode::BackTab {
        app.tui.active_tab = app.tui.active_tab.prev();
        return;
    }

    // Alt+S → toggle sidebar
    if mods == KeyModifiers::ALT && key == KeyCode::Char('s') {
        app.tui.sidebar_visible = !app.tui.sidebar_visible;
        return;
    }

    // Per-tab handling
    match app.tui.active_tab {
        ActiveTab::Chat => handle_chat_keys(app, key, mods),
        ActiveTab::Diff => handle_diff_keys(app, key),
        ActiveTab::Sessions => handle_sessions_keys(app, key),
    }
}

fn handle_chat_keys(app: &mut App, key: KeyCode, _mods: KeyModifiers) {
    match key {
        // Submit
        KeyCode::Enter => {
            if let Some(input) = app.tui.commit_input() {
                submit_input(app, &input);
            }
        }

        // Character input
        KeyCode::Char(c) => {
            app.tui.input_insert(c);
        }

        // Editing
        KeyCode::Backspace => app.tui.input_delete_back(),
        KeyCode::Left => app.tui.input_move_left(),
        KeyCode::Right => app.tui.input_move_right(),
        KeyCode::Home => app.tui.input_home(),
        KeyCode::End => app.tui.input_end(),

        // History
        KeyCode::Up => {
            if app.tui.focus == Focus::Input {
                app.tui.history_prev();
            } else {
                // scroll chat
                app.tui.chat_scroll = app.tui.chat_scroll.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if app.tui.focus == Focus::Input {
                app.tui.history_next();
            } else {
                app.tui.chat_scroll += 1;
            }
        }

        // Page scroll for chat
        KeyCode::PageUp => {
            app.tui.chat_scroll = app.tui.chat_scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.tui.chat_scroll += 10;
        }

        // Sidebar navigation
        KeyCode::F(1) => {
            app.tui.focus = if app.tui.focus == Focus::Sidebar {
                Focus::Input
            } else {
                Focus::Sidebar
            };
        }

        _ => {}
    }
}

fn handle_diff_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Up => app.tui.diff_scroll = app.tui.diff_scroll.saturating_sub(1),
        KeyCode::Down => app.tui.diff_scroll += 1,
        KeyCode::PageUp => app.tui.diff_scroll = app.tui.diff_scroll.saturating_sub(20),
        KeyCode::PageDown => app.tui.diff_scroll += 20,
        KeyCode::Home => app.tui.diff_scroll = 0,
        KeyCode::End => {
            app.tui.diff_scroll = app.tui.diff_content.len().saturating_sub(1);
        }
        _ => {}
    }
}

fn handle_sessions_keys(app: &mut App, key: KeyCode) {
    let len = app.tui.sessions.len();
    if len == 0 {
        return;
    }
    let cur = app.tui.session_list_state.selected().unwrap_or(0);
    match key {
        KeyCode::Up => {
            app.tui.session_list_state.select(Some(cur.saturating_sub(1)));
        }
        KeyCode::Down => {
            app.tui.session_list_state.select(Some((cur + 1).min(len - 1)));
        }
        KeyCode::Enter => {
            // Replay selected session — push events as chat messages
            if let Some(idx) = app.tui.session_list_state.selected() {
                if let Some(s) = app.tui.sessions.get(idx) {
                    let sid = s.id.clone();
                    app.tui.add_message(ChatMessage::system(format!(
                        "Replaying session: {}",
                        sid
                    )));
                    for ev in app.session_store.replay(&sid) {
                        match ev {
                            SessionEvent::UserInput { content, .. } => {
                                app.tui.add_message(ChatMessage::user(&content));
                            }
                            SessionEvent::AssistantMessage { content, .. } => {
                                app.tui.append_agent_token(&content);
                            }
                            SessionEvent::ToolCall { name, args, .. } => {
                                app.tui.add_message(ChatMessage::tool_call(format!(
                                    "{} ← {}",
                                    name, args
                                )));
                            }
                            SessionEvent::EditApplied { file, patch, .. } => {
                                app.tui.set_diff(&patch);
                                app.tui.add_message(ChatMessage::system(format!(
                                    "Edit applied to {}",
                                    file
                                )));
                            }
                            SessionEvent::Error { message, .. } => {
                                app.tui.add_message(ChatMessage::error(&message));
                            }
                            _ => {}
                        }
                    }
                    app.tui.active_tab = ActiveTab::Chat;
                }
            }
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, m: crossterm::event::MouseEvent) {
    match m.kind {
        MouseEventKind::ScrollUp => match app.tui.active_tab {
            ActiveTab::Chat => {
                app.tui.chat_scroll = app.tui.chat_scroll.saturating_sub(3);
            }
            ActiveTab::Diff => {
                app.tui.diff_scroll = app.tui.diff_scroll.saturating_sub(3);
            }
            _ => {}
        },
        MouseEventKind::ScrollDown => match app.tui.active_tab {
            ActiveTab::Chat => app.tui.chat_scroll += 3,
            ActiveTab::Diff => app.tui.diff_scroll += 3,
            _ => {}
        },
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Command dispatch
// ─────────────────────────────────────────────────────────────────────────────
fn submit_input(app: &mut App, input: &str) {
    app.tui.add_message(ChatMessage::user(input));
    let _ = app.session_store.append(&app.session_id, &SessionEvent::user(input));

    if input.starts_with("/index") {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let path = if parts.len() > 1 { parts[1] } else { "." };
        app.tui.is_indexing = true;
        app.tui.barq_context.push(format!("Indexing: {}", path));
        match app.orchestrator.barq.index_repo(path) {
            Ok(_) => {
                app.tui.barq_context.push("Index complete.".to_string());
                app.tui.set_status("Index complete", false);
            }
            Err(e) => {
                app.tui.barq_context.push(format!("Error: {}", e));
                app.tui.set_status(format!("Index error: {}", e), true);
            }
        }
        app.tui.is_indexing = false;
    } else if input == "/config" {
        if let Ok(cfg_str) = toml::to_string_pretty(&app.config) {
            app.tui.add_message(ChatMessage::system(format!("Config:\n{}", cfg_str)));
        }
    } else if input == "/clear" {
        app.tui.messages.clear();
        app.tui.tool_log.clear();
        app.orchestrator.conversation.clear();
        app.tui.add_message(ChatMessage::system("Session cleared."));
    } else if input == "/help" {
        app.tui.add_message(ChatMessage::system(HELP_TEXT));
    } else if input.starts_with("/goal ") {
        let goal_text = input["/goal ".len()..].to_string();
        app.tui.is_thinking = true;
        app.tui.add_message(ChatMessage::system(format!(
            "Starting multi-agent goal: {}",
            goal_text
        )));
        let coordinator = Arc::clone(&app.coordinator);
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let _ = tx
                .send(OrchestratorEvent::Token("Coordinator analyzing…".to_string()))
                .await;
            match coordinator.execute_goal(&goal_text).await {
                Ok(_) => {
                    let _ = tx
                        .send(OrchestratorEvent::Done("Goal completed.".to_string()))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(OrchestratorEvent::Error(format!("Goal failed: {}", e)))
                        .await;
                }
            }
        });
        app.event_rx = Some(rx);
    } else if input == "/diff" {
        // Show last diff in Diff tab (placeholder)
        app.tui.set_diff("--- a/example.rs\n+++ b/example.rs\n@@ -1,4 +1,4 @@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"Hello, BarqCoder!\");\n }\n");
    } else if input == "/sessions" {
        app.tui.active_tab = ActiveTab::Sessions;
    } else {
        // Regular AI prompt
        app.tui.is_thinking = true;
        let rx = app.orchestrator.run(input);
        app.event_rx = Some(rx);
    }
}

const HELP_TEXT: &str = "\
Commands:
  /index [path]   — Index codebase into BarqDB
  /goal <text>    — Run multi-agent goal (Planner → Coder → Tester → Reviewer)
  /diff           — Show last diff in the Diff tab
  /sessions       — Jump to Sessions tab
  /config         — Show current config
  /clear          — Clear chat and conversation
  /help           — Show this message

Keys:
  Enter           — Send message
  ↑/↓             — Navigate input history / scroll
  PageUp/Down     — Scroll chat
  Tab / Shift+Tab — Switch tabs
  Alt+S           — Toggle sidebar
  F1              — Toggle sidebar focus
  Esc             — Quit";

// ─────────────────────────────────────────────────────────────────────────────
// Quit flag on TuiState
// ─────────────────────────────────────────────────────────────────────────────
impl TuiState {
    pub fn should_quit(&self) -> bool {
        self._quit
    }
    pub fn mark_quit(&mut self) {
        self._quit = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Health server
// ─────────────────────────────────────────────────────────────────────────────
async fn start_health_server() {
    if let Ok(listener) = TcpListener::bind("0.0.0.0:8080").await {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                if stream.read(&mut buf).await.is_ok() {
                    let req = String::from_utf8_lossy(&buf);
                    let response = if req.starts_with("GET /health") {
                        "HTTP/1.1 200 OK\r\n\r\nOK"
                    } else if req.starts_with("GET /metrics") {
                        "HTTP/1.1 200 OK\r\n\r\nbarqcoder_requests_total 0\n"
                    } else {
                        "HTTP/1.1 404 Not Found\r\n\r\n"
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        }
    }
}
