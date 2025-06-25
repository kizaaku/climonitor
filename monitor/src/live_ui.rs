use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use chrono::{DateTime, Utc};

use crate::session_manager::SessionManager;
use crate::unicode_utils::truncate_str;
use ccmonitor_shared::LauncherStatus;

/// ライブUI管理
pub struct LiveUI {
    session_manager: Arc<RwLock<SessionManager>>,
    update_receiver: broadcast::Receiver<()>,
    verbose: bool,
    last_update: Option<DateTime<Utc>>,
}

impl LiveUI {
    pub fn new(
        session_manager: Arc<RwLock<SessionManager>>,
        update_receiver: broadcast::Receiver<()>,
        verbose: bool,
    ) -> Self {
        Self {
            session_manager,
            update_receiver,
            verbose,
            last_update: None,
        }
    }

    /// UI表示ループ開始
    pub async fn run(&mut self) -> anyhow::Result<()> {
        println!("🔥 Claude Session Monitor - Live Mode");
        println!("📡 Server running, waiting for launcher connections...");
        println!("Press Ctrl+C to exit\n");

        // 初期表示
        self.render_ui().await;

        // 更新ループ
        loop {
            tokio::select! {
                // 更新通知受信
                _ = self.update_receiver.recv() => {
                    self.render_ui().await;
                }
                
                // 定期更新（5秒間隔）
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                    self.render_ui().await;
                }

                // Ctrl+C 終了
                _ = tokio::signal::ctrl_c() => {
                    println!("\n👋 Shutting down Live UI...");
                    break;
                }
            }
        }

        Ok(())
    }

    /// UI描画
    async fn render_ui(&mut self) {
        let now = Utc::now();
        self.last_update = Some(now);

        // 画面クリア（カーソルを先頭に移動）
        print!("\x1B[H\x1B[2J");

        // ヘッダー
        self.render_header().await;

        // 接続状況
        self.render_connections().await;

        // セッション詳細
        self.render_sessions().await;

        // フッター
        self.render_footer();
    }

    /// ヘッダー描画
    async fn render_header(&self) {
        let stats = self.session_manager.read().await.get_stats();
        
        println!("🔥 Claude Session Monitor - Live Mode");
        println!("📊 Launchers: {} | Sessions: {} (Active: {})", 
            stats.active_launchers, 
            stats.total_sessions, 
            stats.active_sessions
        );
        println!("{}", "═".repeat(80));
    }

    /// 接続状況描画
    async fn render_connections(&self) {
        let session_manager = self.session_manager.read().await;
        let launchers = session_manager.get_active_launchers();

        if launchers.is_empty() {
            println!("⏳ No launcher connections");
            println!("💡 Start with: ccmonitor-launcher claude");
            println!();
            return;
        }

        println!("🔗 Active Launchers:");
        for launcher in launchers {
            let project_str = launcher.project.as_deref().unwrap_or("(no project)");
            let elapsed = format_duration_since(launcher.last_activity);
            let status_icon = match launcher.status {
                LauncherStatus::Connected => "🟡",
                LauncherStatus::Active => "🟢",
                LauncherStatus::Idle => "⚪",
                LauncherStatus::Disconnected => "🔴",
            };

            println!("  {} {} | {} | {}", 
                status_icon,
                truncate_str(&launcher.id, 12),
                truncate_str(project_str, 20),
                elapsed
            );

            if self.verbose {
                let args_str = launcher.claude_args.join(" ");
                println!("     Args: {}", truncate_str(&args_str, 60));
            }
        }
        println!();
    }

    /// セッション詳細描画
    async fn render_sessions(&self) {
        let session_manager = self.session_manager.read().await;
        let sessions_by_project = session_manager.get_sessions_by_project();

        if sessions_by_project.is_empty() {
            println!("📭 No active sessions");
            return;
        }

        println!("📋 Active Sessions:");
        
        for (project_name, sessions) in sessions_by_project {
            println!("  📁 {}:", project_name);
            
            for session in sessions {
                let status_icon = session.status.icon();
                let status_label = session.status.label();
                let elapsed = format_duration_since(session.last_activity);
                let confidence_str = if session.confidence > 0.0 {
                    format!(" ({:.0}%)", session.confidence * 100.0)
                } else {
                    String::new()
                };

                // Show launcher context if available (first few chars)
                let context_display = if let Some(ref context) = session.launcher_context {
                    let short_context = truncate_str(context, 8);
                    format!(" [{}]", short_context)
                } else {
                    String::new()
                };
                
                let execution_indicator = if session.is_waiting_for_execution {
                    " ⏳"
                } else {
                    ""
                };

                println!("    {} {}{} {} | {}{}{}", 
                    status_icon,
                    status_label,
                    execution_indicator,
                    truncate_str(&session.id, 12),
                    elapsed,
                    confidence_str,
                    context_display
                );

                // 最新メッセージ表示
                if let Some(ref message) = session.last_message {
                    let preview = truncate_str(message, 60);
                    println!("      💬 {}", preview);
                }

                // Usage reset time display
                if let Some(ref reset_time) = session.usage_reset_time {
                    println!("      ⏰ Usage resets at: {}", reset_time);
                }
                
                // 詳細情報（verbose モード）
                if self.verbose && !session.evidence.is_empty() {
                    println!("      🔍 Evidence: {}", session.evidence.join(", "));
                }
                
                if self.verbose {
                    if let Some(ref context) = session.launcher_context {
                        println!("      📝 Context: {}", truncate_str(context, 50));
                    }
                }
            }
            println!();
        }
    }

    /// フッター描画
    fn render_footer(&self) {
        if let Some(last_update) = self.last_update {
            println!("🔄 Last update: {} | Press Ctrl+C to exit", 
                last_update.format("%H:%M:%S")
            );
        }
    }

}

/// 時間経過フォーマット
fn format_duration_since(time: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(time);

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


/// 非インタラクティブ表示（--no-tui相当）
pub async fn print_snapshot(session_manager: Arc<RwLock<SessionManager>>, verbose: bool) {
    let session_manager = session_manager.read().await;
    let stats = session_manager.get_stats();
    let sessions_by_project = session_manager.get_sessions_by_project();

    println!("📊 Claude Session Monitor - Snapshot");
    println!("Launchers: {} | Sessions: {} (Active: {})", 
        stats.active_launchers, 
        stats.total_sessions, 
        stats.active_sessions
    );
    println!("{}", "═".repeat(50));

    if sessions_by_project.is_empty() {
        println!("🔍 No active sessions found");
        println!("💡 Start with: ccmonitor-launcher claude");
        return;
    }

    for (project_name, sessions) in sessions_by_project {
        println!("\n📁 Project: {}", project_name);
        println!("   Sessions: {}", sessions.len());
        
        for session in sessions {
            let status_icon = session.status.icon();
            let status_label = session.status.label();
            let elapsed = format_duration_since(session.last_activity);
            
            println!("   {} {} {} - {}", 
                status_icon, 
                status_label,
                truncate_str(&session.id, 12), 
                elapsed
            );
            
            if let Some(ref message) = session.last_message {
                let preview = truncate_str(message, 57);
                println!("     💬 {}", preview);
            }

            if verbose && !session.evidence.is_empty() {
                println!("     🔍 {}", session.evidence.join(", "));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_formatting() {
        let now = Utc::now();
        
        // 30秒前
        let time = now - chrono::Duration::seconds(30);
        assert!(format_duration_since(time).contains("s ago"));
        
        // 5分前
        let time = now - chrono::Duration::minutes(5);
        assert!(format_duration_since(time).contains("m ago"));
        
        // 2時間前
        let time = now - chrono::Duration::hours(2);
        assert!(format_duration_since(time).contains("h ago"));
    }

}