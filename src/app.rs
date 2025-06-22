use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::io;
use tokio::time::{Duration, interval};
use crate::{
    session::SessionStore,
    watcher::FileWatcher,
    ui,
};
use chrono::{DateTime, Utc};
use crate::unicode_utils::{truncate_id, truncate_message};

pub struct App {
    session_store: SessionStore,
    file_watcher: FileWatcher,
    project_filter: Option<String>,
    verbose: bool,
}

impl App {
    pub async fn new(project_filter: Option<String>, verbose: bool) -> anyhow::Result<Self> {
        if verbose {
            eprintln!("Initializing Claude Session Monitor...");
        }
        
        let session_store = SessionStore::new();
        
        if verbose {
            eprintln!("Creating file watcher...");
        }
        let file_watcher = FileWatcher::new()?;
        
        if verbose {
            eprintln!("File watcher created successfully");
        }

        Ok(Self {
            session_store,
            file_watcher,
            project_filter,
            verbose,
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        if self.verbose {
            eprintln!("Setting up terminal...");
        }
        
        // Terminal setup
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        
        if self.verbose {
            eprintln!("Terminal setup complete, starting main loop...");
        }

        let result = self.run_app(&mut terminal).await;
        
        if self.verbose {
            eprintln!("Main loop exited with result: {:?}", result);
        }

        // Terminal cleanup
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    pub async fn print_status(&mut self) -> anyhow::Result<()> {
        use tokio::time::{sleep, Duration};
        
        // 少し待ってセッションファイルを読み込む
        for _ in 0..5 {
            tokio::select! {
                msg = self.file_watcher.next_message() => {
                    if let Some(session_msg) = msg {
                        if self.verbose {
                            println!("Found session: {}", session_msg.session_id);
                        }
                        self.session_store.update_session(session_msg);
                    }
                }
                _ = sleep(Duration::from_millis(200)) => {
                    // タイムアウト
                }
            }
        }
        
        let sessions_by_project = self.session_store.get_sessions_by_project();
        
        if sessions_by_project.is_empty() {
            println!("🔍 No Claude sessions found");
            println!("Make sure Claude is running and has active sessions in ~/.claude/projects/");
            return Ok(());
        }
        
        println!("📊 Claude Session Status");
        println!("========================");
        
        for (project_name, sessions) in sessions_by_project {
            println!("\n📁 Project: {}", project_name);
            println!("   Sessions: {}", sessions.len());
            
            for session in sessions {
                let status_icon = session.status.icon();
                let status_label = session.status.label();
                let time_ago = format_time_ago(session.last_activity);
                
                println!("   {} {} {} - {}", 
                    status_icon, 
                    status_label,
                    truncate_id(&session.id), 
                    time_ago
                );
                
                if let Some(task) = &session.current_task {
                    println!("     📝 {}", task);
                }
                
                if !session.last_message.is_empty() {
                    let preview = truncate_message(&session.last_message, 57);
                    println!("     💬 {}", preview);
                }
            }
        }
        
        Ok(())
    }

    async fn run_app<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()> {
        let mut refresh_interval = interval(Duration::from_millis(1000));
        
        // 初期セッション読み込み待機
        let mut loaded_count = 0;
        for i in 0..10 {
            tokio::select! {
                msg = self.file_watcher.next_message() => {
                    if let Some(session_msg) = msg {
                        if self.verbose {
                            eprintln!("Initial session loaded: {}", session_msg.session_id);
                        }
                        self.session_store.update_session(session_msg);
                        loaded_count += 1;
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if self.verbose && i == 9 {
                        eprintln!("Initial loading timeout, loaded {} sessions", loaded_count);
                    }
                }
            }
        }

        loop {
            // Draw UI
            let sessions_by_project = self.session_store.get_sessions_by_project();
            
            if self.verbose {
                eprintln!("Drawing UI with {} projects", sessions_by_project.len());
            }
            
            terminal.draw(|f| {
                ui::draw(f, &sessions_by_project, &self.project_filter);
            })?;

            // Handle events
            tokio::select! {
                // Handle keyboard input
                should_exit = Self::handle_input() => {
                    match should_exit {
                        Ok(true) => break,  // Exit requested
                        Ok(false) => {},    // Continue
                        Err(_) => {},       // Error, but continue
                    }
                }
                
                // Handle file watcher events
                msg = self.file_watcher.next_message() => {
                    if let Some(session_msg) = msg {
                        if self.verbose {
                            eprintln!("New message: {:?}", session_msg.session_id);
                        }
                        self.session_store.update_session(session_msg);
                    }
                }
                
                // Periodic refresh
                _ = refresh_interval.tick() => {
                    // Force redraw every second
                }
            }
        }

        Ok(())
    }

    async fn handle_input() -> anyhow::Result<bool> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(true); // Should exit
                        }
                        KeyCode::Char('r') => {
                            // Refresh - already handled by periodic refresh
                        }
                        _ => {}
                    }
                }
            }
        }
        
        Ok(false) // Continue running
    }
}

fn format_time_ago(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);

    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else {
        format!("{}d ago", duration.num_days())
    }
}