use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::mpsc;

mod agent;
mod agents;
mod barq;
mod collab;
mod config;
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
use tools::ToolRegistry;
use agents::coordinator::CoordinatorAgent;

struct App {
    input: String,
    messages: Vec<String>,
    tool_log: Vec<String>,
    barq_context: Vec<String>,
    should_quit: bool,
    
    // New fields
    orchestrator: Orchestrator,
    config: Config,
    is_indexing: bool,
    is_thinking: bool,
    current_tool: Option<String>,
    token_count: u32,
    session_id: String,
    
    // Agent orchestration
    coordinator: Arc<CoordinatorAgent>,

    // Channels for async operations
    event_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
}

impl App {
    fn new() -> Self {
        let config = Config::load();
        let agent = OllamaClient::new(&config.ollama_base_url, &config.ollama_model);
        
        // Setup barq index
        let barq = Arc::new(BarqIndex::new(&config).expect("Failed to create BarqIndex"));
        
        let tools = Arc::new(ToolRegistry::with_barq(Arc::clone(&barq)));
        
        let orchestrator = Orchestrator::new(agent.clone(), Arc::clone(&tools), Arc::clone(&barq), config.clone());
        let coordinator = Arc::new(CoordinatorAgent::new(agent, Arc::clone(&barq), tools));

        Self {
            input: String::new(),
            messages: Vec::new(),
            tool_log: Vec::new(),
            barq_context: Vec::new(),
            should_quit: false,
            orchestrator,
            config,
            is_indexing: false,
            is_thinking: false,
            current_tool: None,
            token_count: 0,
            session_id: format!("session_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),
            coordinator,
            event_rx: None,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        handle_input(app);
                    }
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    _ => {}
                }
            }
        }

        // Process orchestrator events
        if let Some(rx) = &mut app.event_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    OrchestratorEvent::Token(t) => {
                        app.is_thinking = true;
                        if let Some(last) = app.messages.last_mut() {
                            if last.starts_with("Agent:") {
                                last.push_str(&t);
                            } else {
                                app.messages.push(format!("Agent: {}", t));
                            }
                        } else {
                            app.messages.push(format!("Agent: {}", t));
                        }
                    }
                    OrchestratorEvent::ToolCall { name, args } => {
                        app.current_tool = Some(name.clone());
                        app.tool_log.push(format!("Calling {} with {}", name, args.to_string()));
                    }
                    OrchestratorEvent::ToolResult { name, result } => {
                        app.current_tool = None;
                        app.tool_log.push(format!("Result for {}: {}", name, result.to_string()));
                    }
                    OrchestratorEvent::Done(answer) => {
                        app.is_thinking = false;
                        app.current_tool = None;
                        app.messages.push(format!("Agent: {}", answer));
                        app.event_rx = None;
                        break;
                    }
                    OrchestratorEvent::Error(err) => {
                        app.is_thinking = false;
                        app.current_tool = None;
                        app.messages.push(format!("Error: {}", err));
                        app.event_rx = None;
                        break;
                    }
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_input(app: &mut App) {
    if app.input.is_empty() {
        return;
    }

    let input = app.input.clone();
    app.messages.push(format!("You: {}", input));
    app.input.clear();

    if input.starts_with("/index") {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let path = if parts.len() > 1 { parts[1] } else { "." };
        app.barq_context.push(format!("Indexing path: {}", path));
        if let Err(e) = app.orchestrator.barq.index_repo(path) {
            app.barq_context.push(format!("Error indexing: {}", e));
        } else {
            app.barq_context.push(format!("Indexed successfully."));
        }
    } else if input == "/config" {
        if let Ok(config_str) = toml::to_string_pretty(&app.config) {
            app.messages.push(format!("Config:\n{}", config_str));
        }
    } else if input == "/clear" {
        app.messages.clear();
        app.orchestrator.conversation.clear();
    } else if input == "/replay" {
        app.messages.push("Replay not implemented yet.".to_string());
    } else if input == "/help" {
        app.messages.push("Commands: /index [path], /config, /clear, /replay, /help".to_string());
    } else if input.starts_with("/goal ") {
        app.is_thinking = true;
        let goal_text = input["/goal ".len()..].to_string();
        app.messages.push(format!("Starting multi-agent goal: {}", goal_text));
        
        let coordinator = Arc::clone(&app.coordinator);
        let (tx, rx) = mpsc::channel(100);
        
        tokio::spawn(async move {
            let _ = tx.send(OrchestratorEvent::Token("Coordinator analyzing...".to_string())).await;
            match coordinator.execute_goal(&goal_text).await {
                Ok(_) => {
                    let _ = tx.send(OrchestratorEvent::Done("Goal completed successfully.".to_string())).await;
                }
                Err(e) => {
                    let _ = tx.send(OrchestratorEvent::Error(format!("Goal failed: {}", e))).await;
                }
            }
        });
        
        app.event_rx = Some(rx);
    } else {
        // Start orchestrator loop
        app.is_thinking = true;
        let rx = app.orchestrator.run(&input);
        app.event_rx = Some(rx);
    }
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Chat history
            Constraint::Percentage(20), // Tool log
            Constraint::Percentage(20), // BARQ Context
            Constraint::Percentage(10), // Input
        ])
        .split(f.area());

    // Pane 1: Chat history
    let messages_text = app.messages.join("\n");
    let mut messages_height = app.messages.len() as u16;
    let available_height = chunks[0].height.saturating_sub(2);
    let mut scroll_offset = 0;
    if messages_height > available_height {
        scroll_offset = messages_height - available_height;
    }

    let spinner = if app.is_thinking { "⠋" } else { "" };
    
    let messages_p = Paragraph::new(messages_text)
        .block(Block::default().title(format!("BarqCoder {}", spinner)).borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));
    f.render_widget(messages_p, chunks[0]);

    // Pane 2: Tool log
    let tool_title = if let Some(t) = &app.current_tool {
        format!("Tool Log [Active: {}]", t)
    } else {
        "Tool Log".to_string()
    };
    
    let tool_text = app.tool_log.join("\n");
    let tool_p = Paragraph::new(tool_text)
        .block(Block::default().title(tool_title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(tool_p, chunks[1]);

    // Pane 3: BARQ Context
    let context_title = format!("BARQ Context [Tokens: {}]", app.token_count);
    let context_text = app.barq_context.join("\n");
    let context_p = Paragraph::new(context_text)
        .block(Block::default().title(context_title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(context_p, chunks[2]);

    // Pane 4: Input
    let input_p = Paragraph::new(app.input.as_str())
        .block(Block::default().title("Input (ESC quit)").borders(Borders::ALL));
    f.render_widget(input_p, chunks[3]);
}
