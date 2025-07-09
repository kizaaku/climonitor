use anyhow::Result;
use async_trait::async_trait;
use climonitor_shared::{
    transport::{MessageHandler, MessageReceiver},
    ConnectionConfig, LauncherToMonitor,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

use crate::notification::NotificationManager;
use crate::session_manager::SessionManager;

/// 接続情報
#[derive(Debug)]
struct ConnectionInfo {
    id: String,
    peer_addr: String,
    connected_at: chrono::DateTime<chrono::Utc>,
}

/// 抽象化されたTransport Monitor サーバー
pub struct TransportMonitorServer {
    config: ConnectionConfig,
    session_manager: Arc<RwLock<SessionManager>>,
    connections: Arc<RwLock<HashMap<String, ConnectionInfo>>>,
    ui_update_sender: broadcast::Sender<()>,
    task_handles: Vec<JoinHandle<()>>,
    verbose: bool,
    _log_file: Option<PathBuf>,
    _message_receiver: Option<Box<dyn MessageReceiver>>,
}

impl TransportMonitorServer {
    pub fn new(config: ConnectionConfig, verbose: bool, log_file: Option<PathBuf>) -> Result<Self> {
        let session_manager = Arc::new(RwLock::new(SessionManager::new()));
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let (ui_update_sender, _) = broadcast::channel(100);

        Ok(Self {
            config,
            session_manager,
            connections,
            ui_update_sender,
            task_handles: Vec::new(),
            verbose,
            _log_file: log_file,
            _message_receiver: None,
        })
    }

    /// サーバー開始とメインループ実行
    pub async fn run(&mut self) -> Result<()> {
        if self.verbose {
            println!("📡 Starting monitor server with config: {:?}", self.config);
        }

        // Create message handler
        let handler = MonitorMessageHandler {
            session_manager: Arc::clone(&self.session_manager),
            ui_update_sender: self.ui_update_sender.clone(),
            _connections: Arc::clone(&self.connections),
            verbose: self.verbose,
        };

        // Create message receiver
        let mut message_receiver =
            crate::transports::create_message_receiver(&self.config, Box::new(handler)).await?;

        if self.verbose {
            println!("⚡ Server running, waiting for launcher connections...");
        }

        tokio::select! {
            // Start the message receiver
            result = message_receiver.start_server() => {
                if let Err(e) = result {
                    eprintln!("❌ Server error: {e}");
                }
            }

            // Ctrl+C などでの終了
            _ = tokio::signal::ctrl_c() => {
                if self.verbose {
                    println!("\n🛑 Shutting down monitor server...");
                }
            }
        }

        self.shutdown(&mut message_receiver).await?;
        Ok(())
    }

    // This method is no longer needed with the new trait-based approach

    // This method is no longer needed with the new trait-based approach

    // This method is no longer needed with the new trait-based approach

    // This method is now handled by the MessageHandler implementation

    /// 必要に応じて通知を送信（既存のロジックを再利用）
    async fn send_notification_if_needed(
        tool_name: String,
        duration_seconds: u64,
        status: climonitor_shared::SessionStatus,
        ui_above_text: Option<String>,
        previous_status: Option<climonitor_shared::SessionStatus>,
    ) {
        use climonitor_shared::SessionStatus;

        let notification_manager = NotificationManager::new();
        let message = ui_above_text.unwrap_or_else(|| "状態変化".to_string());
        let duration_str = format!("{duration_seconds}s");

        // WaitingInput -> Idle の場合はキャンセルとみなして通知しない
        if let (Some(SessionStatus::WaitingInput), SessionStatus::Idle) =
            (&previous_status, &status)
        {
            return;
        }

        // 作業待ちと完了時のみ通知
        match status {
            SessionStatus::WaitingInput => {
                notification_manager
                    .notify_waiting(&tool_name, &message, &duration_str)
                    .await;
            }
            SessionStatus::Idle => {
                notification_manager
                    .notify_completion(&tool_name, &message, &duration_str)
                    .await;
            }
            _ => {
                // 他の状態では通知しない
            }
        }
    }

    /// UI更新通知受信用
    pub fn subscribe_ui_updates(&self) -> broadcast::Receiver<()> {
        self.ui_update_sender.subscribe()
    }

    /// セッションマネージャー取得
    pub fn get_session_manager(&self) -> Arc<RwLock<SessionManager>> {
        Arc::clone(&self.session_manager)
    }

    /// サーバー終了
    async fn shutdown(&mut self, message_receiver: &mut Box<dyn MessageReceiver>) -> Result<()> {
        // 全タスクを終了
        for handle in self.task_handles.drain(..) {
            handle.abort();
        }

        // サーバー終了
        message_receiver.shutdown().await?;

        if self.verbose {
            println!("✅ Monitor server shutdown complete");
        }

        Ok(())
    }
}

/// MessageHandler implementation for the monitor server
struct MonitorMessageHandler {
    session_manager: Arc<RwLock<SessionManager>>,
    ui_update_sender: broadcast::Sender<()>,
    _connections: Arc<RwLock<HashMap<String, ConnectionInfo>>>,
    verbose: bool,
}

#[async_trait]
impl MessageHandler for MonitorMessageHandler {
    async fn handle_message(&self, message: LauncherToMonitor) -> Result<()> {
        if self.verbose {
            println!("📨 Handling message: {message:?}");
        }

        // 通知用の情報を事前に抽出
        let notification_info = match &message {
            LauncherToMonitor::StateUpdate {
                launcher_id,
                session_id,
                status,
                ui_above_text,
                ..
            } => {
                let (tool_name, duration_seconds, previous_status) = {
                    let manager = self.session_manager.read().await;
                    let tool_name = manager
                        .get_launcher(launcher_id)
                        .map(|l| l.tool_type.to_command().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    let (duration_seconds, previous_status) =
                        if let Some(session) = manager.get_session(session_id) {
                            let elapsed = chrono::Utc::now()
                                .signed_duration_since(session.last_status_change);
                            let duration = elapsed.num_seconds().max(0) as u64;
                            (duration, Some(session.status.clone()))
                        } else {
                            (0, None)
                        };

                    (tool_name, duration_seconds, previous_status)
                };
                Some((
                    tool_name,
                    duration_seconds,
                    status.clone(),
                    ui_above_text.clone(),
                    previous_status,
                ))
            }
            _ => None,
        };

        // セッションマネージャーで処理
        if let Err(e) = self.session_manager.write().await.handle_message(message) {
            eprintln!("⚠️  Message handling error: {e}");
        } else {
            if self.verbose {
                println!("✅ Message processed successfully");
            }

            // 通知送信（StateUpdateの場合のみ）
            if let Some((tool_name, duration_seconds, status, ui_above_text, previous_status)) =
                notification_info
            {
                TransportMonitorServer::send_notification_if_needed(
                    tool_name,
                    duration_seconds,
                    status,
                    ui_above_text,
                    previous_status,
                )
                .await;
            }
        }

        // UI更新通知
        let _ = self.ui_update_sender.send(());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = ConnectionConfig::default_grpc();
        let server = TransportMonitorServer::new(config, false, None);
        assert!(server.is_ok());
    }
}
