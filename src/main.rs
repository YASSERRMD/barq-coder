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
use std::{io, time::Duration};

mod agent;
mod barq;
mod config;
mod tools;
mod verifier;

struct App {
    input: String,
    messages: Vec<String>,
    tool_log: Vec<String>,
    barq_context: Vec<String>,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            tool_log: Vec::new(),
            barq_context: Vec::new(),
            should_quit: false,
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
    terminal.show_cursor()?;

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

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_input(app: &mut App) {
    if !app.input.is_empty() {
        app.messages.push(format!("You: {}", app.input));
        app.input.clear();
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
    let messages_height = app.messages.len() as u16;
    let available_height = chunks[0].height.saturating_sub(2);
    let mut scroll_offset = 0;
    if messages_height > available_height {
        scroll_offset = messages_height - available_height;
    }
    
    let messages_p = Paragraph::new(messages_text)
        .block(Block::default().title("BarqCoder").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));
    f.render_widget(messages_p, chunks[0]);

    // Pane 2: Tool log
    let tool_text = app.tool_log.join("\n");
    let tool_p = Paragraph::new(tool_text)
        .block(Block::default().title("Tool Log").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(tool_p, chunks[1]);

    // Pane 3: BARQ Context
    let context_text = app.barq_context.join("\n");
    let context_p = Paragraph::new(context_text)
        .block(Block::default().title("BARQ Context").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(context_p, chunks[2]);

    // Pane 4: Input
    let input_p = Paragraph::new(app.input.as_str())
        .block(Block::default().title("Input (ESC quit)").borders(Borders::ALL));
    f.render_widget(input_p, chunks[3]);
}
