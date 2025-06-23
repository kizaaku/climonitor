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
                    if let Some(msg_with_file) = msg {
                        if self.verbose {
                            println!("Found session: {}", msg_with_file.message.session_id);
                        }
                        self.session_store.update_session_with_file(msg_with_file.message, msg_with_file.file_path);
                    }
                }
                _ = sleep(Duration::from_millis(200)) => {
                    // タイムアウト
                }
            }
        }
        
        // 最新の状態に更新
        self.session_store.update_status_by_time();
        
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
                    if let Some(msg_with_file) = msg {
                        if self.verbose {
                            eprintln!("Initial session loaded: {}", msg_with_file.message.session_id);
                        }
                        self.session_store.update_session_with_file(msg_with_file.message, msg_with_file.file_path);
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
            // 既存セッションの状態を時間経過に基づいて更新
            self.session_store.update_status_by_time();
            
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
                    if let Some(msg_with_file) = msg {
                        if self.verbose {
                            eprintln!("New message: {:?}", msg_with_file.message.session_id);
                        }
                        self.session_store.update_session_with_file(msg_with_file.message, msg_with_file.file_path);
                    }
                }
                
                // Periodic refresh (every second)
                _ = refresh_interval.tick() => {
                    // 時間経過による状態更新とUI再描画
                    if self.verbose {
                        eprintln!("Periodic refresh - updating session states");
                    }
                    self.session_store.update_status_by_time();
                }
            }
        }

        Ok(())
    }

    pub async fn watch_mode(&mut self) -> anyhow::Result<()> {
        use tokio::time::{interval, Duration};
        
        println!("🔍 Claude Session Monitor - Watch Mode");
        println!("Press Ctrl+C to exit");
        println!("Updates every second...\n");
        
        let mut update_interval = interval(Duration::from_secs(1));
        let mut last_status_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        
        loop {
            tokio::select! {
                // Handle file watcher events
                msg = self.file_watcher.next_message() => {
                    if let Some(msg_with_file) = msg {
                        if self.verbose {
                            println!("📨 New message: {}", msg_with_file.message.session_id);
                        }
                        self.session_store.update_session_with_file(msg_with_file.message, msg_with_file.file_path);
                    }
                }
                
                // Periodic update and display
                _ = update_interval.tick() => {
                    self.session_store.update_status_by_time();
                    
                    let sessions_by_project = self.session_store.get_sessions_by_project();
                    let mut status_changed = false;
                    
                    // Check for status changes
                    for (project_name, sessions) in &sessions_by_project {
                        for session in sessions {
                            let current_status = format!("{} {}", session.status.icon(), session.status.label());
                            let session_key = format!("{}:{}", project_name, &session.id[..8]);
                            
                            if let Some(last_status) = last_status_map.get(&session_key) {
                                if last_status != &current_status {
                                    println!("🔄 {} - {} -> {}", 
                                        session_key, 
                                        last_status, 
                                        current_status
                                    );
                                    status_changed = true;
                                }
                            } else {
                                println!("✨ {} - {}", session_key, current_status);
                                status_changed = true;
                            }
                            
                            last_status_map.insert(session_key, current_status);
                        }
                    }
                    
                    // Remove sessions that no longer exist
                    let current_keys: std::collections::HashSet<String> = sessions_by_project.iter()
                        .flat_map(|(project_name, sessions)| {
                            sessions.iter().map(move |s| format!("{}:{}", project_name, &s.id[..8]))
                        })
                        .collect();
                    
                    last_status_map.retain(|key, _| current_keys.contains(key));
                    
                    if status_changed && self.verbose {
                        println!("📊 Active sessions: {}", 
                            sessions_by_project.values().flatten().count()
                        );
                    }
                }
            }
        }
    }

    pub async fn demo_mode(&mut self) -> anyhow::Result<()> {
        use tokio::time::{Duration, interval};
        use crate::session::{SessionMessage, MessageContent, ContentItem};
        use chrono::Utc;
        use serde_json::json;
        use tempfile::NamedTempFile;
        
        println!("🎭 Claude Session Monitor - Demo Mode");
        println!("Testing 1-second timer: tool_use → Active → (1s) → Approve");
        println!("Press Ctrl+C to exit\n");
        
        // Create a demo session with tool_use message
        let demo_session_id = "demo-session-12345678".to_string();
        let temp_file = NamedTempFile::new()?;
        
        let tool_use_msg = SessionMessage {
            parent_uuid: None,
            user_type: "demo".to_string(),
            cwd: "/demo/project".to_string(),
            session_id: demo_session_id.clone(),
            version: "1.0".to_string(),
            message_type: "assistant".to_string(),
            message: MessageContent::Assistant {
                role: "assistant".to_string(),
                content: vec![ContentItem::ToolUse {
                    id: "demo_tool_id".to_string(),
                    name: "Read".to_string(),
                    input: json!({"file_path": "/demo/file.txt"}),
                }],
            },
            uuid: "demo-uuid".to_string(),
            timestamp: Utc::now(),
            tool_use_result: None,
        };
        
        // Add the demo session
        self.session_store.update_session_with_file(tool_use_msg, temp_file.path().to_path_buf());
        
        println!("✨ Created demo session with tool_use message");
        println!("Session ID: {}", &demo_session_id[..16]);
        
        let mut update_interval = interval(Duration::from_millis(500)); // Update every 500ms for more responsive demo
        let mut elapsed_seconds = 0.0;
        
        loop {
            tokio::select! {
                _ = update_interval.tick() => {
                    elapsed_seconds += 0.5;
                    
                    // Update session status
                    self.session_store.update_status_by_time();
                    
                    // Get current status
                    let sessions_by_project = self.session_store.get_sessions_by_project();
                    if let Some(demo_session) = sessions_by_project.values()
                        .flatten()
                        .find(|s| s.id == demo_session_id) {
                        
                        println!("⏱️  {:.1}s - {} {} ({})", 
                            elapsed_seconds,
                            demo_session.status.icon(),
                            demo_session.status.label(),
                            &demo_session.id[..8]
                        );
                        
                        // Show the transition at 1 second
                        if elapsed_seconds >= 1.0 && elapsed_seconds < 1.5 {
                            println!("🎯 1-second timer triggered! Status should now be 'Approve'");
                        }
                        
                        // Exit after demonstrating for 3 seconds
                        if elapsed_seconds >= 3.0 {
                            println!("\n✅ Demo completed! The 1-second timer is working correctly.");
                            break;
                        }
                    } else {
                        println!("❌ Demo session not found");
                        break;
                    }
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