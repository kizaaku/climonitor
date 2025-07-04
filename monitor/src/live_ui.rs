use chrono::{DateTime, Local, Utc};
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
        let session_manager = self.session_manager.read().await;
        let launcher_count = session_manager.get_active_launchers().len();
        let terminal_width = get_terminal_width();

        println!("🔥 Claude Session Monitor - Live Mode");
        println!("📊 Launchers: {launcher_count}");
        println!("{}", "═".repeat(terminal_width));
    }

    /// ランチャー詳細描画（セッション情報も含む）
    async fn render_sessions(&self) {
        let session_manager = self.session_manager.read().await;
        let launchers_by_project = session_manager.get_launchers_by_project();

        // launcher接続があるかをチェック
        if launchers_by_project.is_empty() {
            println!("⏳ No launcher connections");
            println!("💡 Start with: climonitor-launcher claude");
            println!();
            return;
        }

        for (project_name, launchers) in launchers_by_project {
            println!("  📁 {project_name}:");

            for (launcher, session_opt) in launchers {
                // Tool type display
                let tool_type_display = match launcher.tool_type {
                    climonitor_shared::CliToolType::Claude => " 🤖",
                    climonitor_shared::CliToolType::Gemini => " ✨",
                };

                if let Some(session) = session_opt {
                    // セッションがある場合：通常表示
                    let status_icon = session.status.icon();
                    let status_label = session.status.label();
                    let elapsed = format_duration_since(session.last_activity);

                    let execution_indicator = if session.is_waiting_for_execution {
                        " ⏳"
                    } else {
                        ""
                    };

                    // UI box上のテキスト表示
                    let ui_above_display = if let Some(ref ui_text) = session.ui_above_text {
                        let terminal_width = get_terminal_width();
                        // 固定部分の文字数を計算: "    🔵 🤖 完了 | 51s ago "
                        let prefix_length = 4
                            + 1
                            + 2
                            + 1
                            + status_label.len()
                            + execution_indicator.len()
                            + 3
                            + elapsed.len()
                            + 1;
                        let available_width = terminal_width.saturating_sub(prefix_length);
                        format!(" {}", truncate_str(ui_text, available_width))
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
                } else {
                    // セッションがない場合：待機中表示
                    let elapsed = format_duration_since(launcher.last_activity);
                    println!("    🔗{tool_type_display} 接続済み | {elapsed}");
                }
            }
            println!();
        }
    }

    /// フッター描画
    fn render_footer(&self) {
        if let Some(last_update) = self.last_update {
            // UTCからローカル時刻に変換
            let local_time = last_update.with_timezone(&Local);
            println!(
                "🔄 Last update: {} | Press Ctrl+C to exit",
                local_time.format("%H:%M:%S")
            );
        }
    }
}

/// 時間経過フォーマット（ロケール対応）
fn format_duration_since(time: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(time);

    // システムロケールに基づいて適切な suffix を決定
    let (seconds_suffix, minutes_suffix, hours_suffix, days_suffix) = get_locale_suffixes();

    if duration.num_seconds() < 60 {
        let seconds = duration.num_seconds();
        format!("{seconds}{seconds_suffix}")
    } else if duration.num_minutes() < 60 {
        let minutes = duration.num_minutes();
        format!("{minutes}{minutes_suffix}")
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        format!("{hours}{hours_suffix}")
    } else {
        let days = duration.num_days();
        format!("{days}{days_suffix}")
    }
}

/// ロケールに基づいて時間単位のサフィックスを取得
fn get_locale_suffixes() -> (&'static str, &'static str, &'static str, &'static str) {
    // 環境変数でロケールを判定
    let lang = std::env::var("LANG").unwrap_or_else(|_| "en".to_string());

    if lang.starts_with("ja") {
        ("秒前", "分前", "時間前", "日前")
    } else {
        ("s ago", "m ago", "h ago", "d ago")
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
        let result = format_duration_since(time);
        assert!(result.contains("30") && (result.contains("s ago") || result.contains("秒前")));

        // 5分前
        let time = now - chrono::Duration::minutes(5);
        let result = format_duration_since(time);
        assert!(result.contains("5") && (result.contains("m ago") || result.contains("分前")));

        // 2時間前
        let time = now - chrono::Duration::hours(2);
        let result = format_duration_since(time);
        assert!(result.contains("2") && (result.contains("h ago") || result.contains("時間前")));
    }

    #[test]
    fn test_locale_suffixes() {
        let (s, m, h, d) = get_locale_suffixes();

        // English or Japanese suffixes should be returned
        assert!(
            (s == "s ago" && m == "m ago" && h == "h ago" && d == "d ago")
                || (s == "秒前" && m == "分前" && h == "時間前" && d == "日前")
        );
    }
}
