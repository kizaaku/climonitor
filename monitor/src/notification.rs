// notification.rs - Simple notification system via user script

use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct NotificationManager {
    script_path: Option<PathBuf>,
}

impl NotificationManager {
    pub fn new() -> Self {
        let script_path = Self::find_notification_script();
        Self { script_path }
    }

    /// ~/.climonitor/notify.sh を探す
    fn find_notification_script() -> Option<PathBuf> {
        if let Some(home) = home::home_dir() {
            let script = home.join(".climonitor").join("notify.sh");
            if script.exists() && script.is_file() {
                return Some(script);
            }
        }
        None
    }

    /// 通知スクリプトを実行
    pub async fn notify(
        &self,
        event_type: &str,
        tool: &str,
        message: &str,
        duration: &str,
    ) {
        if let Some(ref script_path) = self.script_path {
            self.execute_script(script_path, event_type, tool, message, duration).await;
        }
        // スクリプトがない場合は何もしない（エラーも出さない）
    }

    /// スクリプト実行（非同期、エラーは無視）
    async fn execute_script(
        &self,
        script_path: &Path,
        event_type: &str,
        tool: &str,
        message: &str,
        duration: &str,
    ) {
        let script_path = script_path.to_path_buf();
        let event_type = event_type.to_string();
        let tool = tool.to_string();
        let message = message.to_string();
        let duration = duration.to_string();

        tokio::spawn(async move {
            let result = Command::new("sh")
                .arg(&script_path)
                .arg(&event_type)
                .arg(&tool)
                .arg(&message)
                .arg(&duration)
                .output()
                .await;

            // エラーは無視（デバッグ時のみログ出力）
            if let Err(_e) = result {
                #[cfg(debug_assertions)]
                eprintln!("⚠️  Notification script failed: {_e}");
            }
        });
    }

    /// 実行完了通知
    pub async fn notify_completion(&self, tool: &str, message: &str, duration: &str) {
        self.notify("completed", tool, message, duration).await;
    }

    /// エラー通知
    pub async fn notify_error(&self, tool: &str, message: &str) {
        self.notify("error", tool, message, "").await;
    }

    /// 長時間待機通知
    pub async fn notify_waiting(&self, tool: &str, message: &str, duration: &str) {
        self.notify("waiting", tool, message, duration).await;
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}