use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::session_manager::SessionManager;
use crate::unicode_utils::truncate_str;

/// ターミナル幅を取得（デフォルト80）
fn get_terminal_width() -> usize {
    if let Some((width, _)) = term_size::dimensions() {
        width.max(40) // 最低40文字は確保
    } else {
        80 // デフォルト幅
    }
}

/// ライブUI管理
pub struct LiveUI {
    session_manager: Arc<RwLock<SessionManager>>,
    update_receiver: broadcast::Receiver<()>,
    verbose: bool,
    last_update: Option<DateTime<Utc>>,
    rendering: bool,
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
            rendering: false,
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
                    if !self.rendering {
                        self.render_ui().await;
                    }
                }

                // 定期更新（5秒間隔）
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                    if !self.rendering {
                        self.render_ui().await;
                    }
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
        if self.rendering {
            return; // 既に描画中の場合はスキップ
        }

        self.rendering = true;

        // 画面クリア（初回以外）
        if self.last_update.is_some() {
            print!("\x1b[2J\x1b[H"); // ANSI: 画面クリア + カーソルを左上に移動
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
        }

        let now = Utc::now();
        self.last_update = Some(now);

        // ヘッダー
        self.render_header().await;

        // セッション詳細（unknown project は除外）
        self.render_sessions().await;

        // フッター
        self.render_footer();

        self.rendering = false;
    }

    /// ヘッダー描画
    async fn render_header(&self) {
        let stats = self.session_manager.read().await.get_stats();
        let terminal_width = get_terminal_width();

        println!("🔥 Claude Session Monitor - Live Mode");
        println!("📊 Session: {stats}", stats = stats.total_sessions);
        println!("{}", "═".repeat(terminal_width));
    }

    /// セッション詳細描画
    async fn render_sessions(&self) {
        let session_manager = self.session_manager.read().await;
        let sessions_by_project = session_manager.get_sessions_by_project();

        if sessions_by_project.is_empty() {
            println!("⏳ No launcher connections");
            println!("💡 Start with: climonitor-launcher claude");
            println!();
            return;
        }

        // セッション表示開始（ヘッダーなし）

        for (project_name, sessions) in sessions_by_project {
            println!("  📁 {project_name}:");

            for session in sessions {
                let status_icon = session.status.icon();
                let status_label = session.status.label();
                let elapsed = format_duration_since(session.last_activity);
                // confidence表示を削除

                // 不要な表示項目を削除（ui_above_textで置き換え）

                // Show tool type
                let tool_type_display = if let Some(ref tool_type) = session.tool_type {
                    match tool_type {
                        climonitor_shared::CliToolType::Claude => " 🤖",
                        climonitor_shared::CliToolType::Gemini => " ✨",
                    }
                } else {
                    ""
                };

                let execution_indicator = if session.is_waiting_for_execution {
                    " ⏳"
                } else {
                    ""
                };

                // UI box上のテキスト表示（⏺以降）
                let ui_above_display = if let Some(ref ui_text) = session.ui_above_text {
                    let terminal_width = get_terminal_width();
                    let available_width = terminal_width.saturating_sub(20); // 余白を考慮
                    format!(
                        " {ui_text}",
                        ui_text = truncate_str(ui_text, available_width)
                    )
                } else {
                    String::new()
                };

                println!(
                    "    {status_icon}{tool_type_display} {status_label}{execution_indicator} | {elapsed}{ui_above_display}"
                );

                // 最新メッセージ表示
                if let Some(ref message) = session.last_message {
                    let preview = truncate_str(message, 60);
                    println!("      💬 {preview}");
                }

                // Usage reset time display
                if let Some(ref reset_time) = session.usage_reset_time {
                    println!("      ⏰ Usage resets at: {reset_time}");
                }

                // 詳細情報（verbose モード）
                if self.verbose && !session.evidence.is_empty() {
                    let evidence = session.evidence.join(", ");
                    println!("      🔍 Evidence: {evidence}");
                }

                if self.verbose {
                    if let Some(ref context) = session.launcher_context {
                        let context_display = truncate_str(context, 50);
                        println!("      📝 Context: {context_display}");
                    }
                }
            }
            println!();
        }
    }

    /// フッター描画
    fn render_footer(&self) {
        if let Some(last_update) = self.last_update {
            println!(
                "🔄 Last update: {} | Press Ctrl+C to exit",
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
        let seconds = duration.num_seconds();
        format!("{seconds}s ago")
    } else if duration.num_minutes() < 60 {
        let minutes = duration.num_minutes();
        format!("{minutes}m ago")
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        format!("{hours}h ago")
    } else {
        let days = duration.num_days();
        format!("{days}d ago")
    }
}

/// 非インタラクティブ表示（--no-tui相当）
pub async fn print_snapshot(session_manager: Arc<RwLock<SessionManager>>, verbose: bool) {
    let session_manager = session_manager.read().await;
    let stats = session_manager.get_stats();
    let sessions_by_project = session_manager.get_sessions_by_project();

    println!("📊 Claude Session Monitor - Snapshot");
    println!("Session: {stats}", stats = stats.total_sessions);
    println!("{}", "═".repeat(50));

    if sessions_by_project.is_empty() {
        println!("🔍 No active sessions found");
        println!("💡 Start with: climonitor-launcher claude");
        return;
    }

    for (project_name, sessions) in sessions_by_project {
        println!("\n📁 Project: {project_name}");
        let session_count = sessions.len();
        println!("   Sessions: {session_count}");

        for session in sessions {
            let status_icon = session.status.icon();
            let status_label = session.status.label();
            let elapsed = format_duration_since(session.last_activity);

            println!(
                "   {} {} {} - {}",
                status_icon,
                status_label,
                truncate_str(&session.id, 12),
                elapsed
            );

            if let Some(ref message) = session.last_message {
                let preview = truncate_str(message, 57);
                println!("     💬 {preview}");
            }

            if verbose && !session.evidence.is_empty() {
                let evidence = session.evidence.join(", ");
                println!("     🔍 {evidence}");
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
