use anyhow::Result;
use climonitor_shared::{Connection, ConnectionConfig, LauncherToMonitor, ServerTransport, create_server_transport};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

use crate::notification::NotificationManager;
use crate::session_manager::SessionManager;

/// 接続情報
#[derive(Debug)]
#[allow(dead_code)]
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
    log_file: Option<PathBuf>,
}

impl TransportMonitorServer {
    pub fn new(
        config: ConnectionConfig,
        verbose: bool,
        log_file: Option<PathBuf>,
    ) -> Result<Self> {
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
            log_file,
        })
    }

    /// サーバー開始とメインループ実行
    pub async fn run(&mut self) -> Result<()> {
        if self.verbose {
            println!("📡 Starting monitor server with config: {:?}", self.config);
        }

        let mut server_transport = create_server_transport(&self.config).await?;

        if self.verbose {
            println!("⚡ Server running, waiting for launcher connections...");
        }

        loop {
            tokio::select! {
                // 新しい接続を受け入れ
                accept_result = server_transport.accept(&self.config) => {
                    match accept_result {
                        Ok(connection) => {
                            let connection_id = climonitor_shared::generate_connection_id();
                            let peer_addr = connection.peer_addr().to_string();
                            
                            if self.verbose {
                                println!("🔗 New connection: {} from {}", connection_id, peer_addr);
                            }
                            
                            self.handle_new_connection(connection_id, connection, peer_addr).await?;
                        }
                        Err(e) => {
                            eprintln!("❌ Accept error: {e}");
                        }
                    }
                }

                // Ctrl+C などでの終了
                _ = tokio::signal::ctrl_c() => {
                    if self.verbose {
                        println!("\n🛑 Shutting down monitor server...");
                    }
                    break;
                }
            }
        }

        self.shutdown(&mut server_transport).await?;
        Ok(())
    }

    /// 新しい接続を処理
    async fn handle_new_connection(
        &mut self,
        connection_id: String,
        connection: Connection,
        peer_addr: String,
    ) -> Result<()> {
        let connection_info = ConnectionInfo {
            id: connection_id.clone(),
            peer_addr,
            connected_at: chrono::Utc::now(),
        };

        // 接続を登録
        self.connections
            .write()
            .await
            .insert(connection_id.clone(), connection_info);

        // 接続ハンドラータスクを開始
        let task_handle = self.spawn_connection_handler(connection_id, connection).await;
        self.task_handles.push(task_handle);

        Ok(())
    }

    /// 接続ハンドラータスクを生成
    async fn spawn_connection_handler(
        &self,
        connection_id: String,
        connection: Connection,
    ) -> JoinHandle<()> {
        let connections = Arc::clone(&self.connections);
        let session_manager = Arc::clone(&self.session_manager);
        let ui_update_sender = self.ui_update_sender.clone();
        let verbose = self.verbose;
        let log_file = self.log_file.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::handle_connection(
                connection_id.clone(),
                connection,
                connections,
                session_manager,
                ui_update_sender,
                verbose,
                log_file,
            )
            .await
            {
                if verbose {
                    eprintln!("⚠️  Connection {connection_id} error: {e}");
                }
            }
        })
    }

    /// 個別接続の処理
    async fn handle_connection(
        connection_id: String,
        mut connection: Connection,
        connections: Arc<RwLock<HashMap<String, ConnectionInfo>>>,
        session_manager: Arc<RwLock<SessionManager>>,
        ui_update_sender: broadcast::Sender<()>,
        verbose: bool,
        _log_file: Option<PathBuf>,
    ) -> Result<()> {
        let mut buffer = String::new();

        loop {
            buffer.clear();

            match connection.read_line(&mut buffer).await {
                Ok(0) => {
                    // 接続が閉じられた
                    if verbose {
                        println!("📴 Connection closed: {connection_id}");
                    }

                    // 接続が切断された場合、関連するlauncherを削除
                    Self::cleanup_disconnected_launcher(&connection_id, &session_manager, verbose)
                        .await;

                    break;
                }
                Ok(bytes_read) => {
                    // メッセージを受信
                    if verbose {
                        println!(
                            "📥 Raw message from {} ({} bytes): {}",
                            connection_id,
                            bytes_read,
                            buffer.trim()
                        );
                    }

                    if let Ok(message) = serde_json::from_str::<LauncherToMonitor>(buffer.trim()) {
                        if verbose {
                            println!("📨 Parsed message from {connection_id}: {message:?}");
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
                                    let manager = session_manager.read().await;
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
                        if let Err(e) = session_manager.write().await.handle_message(message) {
                            eprintln!("⚠️  Message handling error: {e}");
                        } else {
                            if verbose {
                                println!("✅ Message processed successfully");
                            }

                            // 通知送信（StateUpdateの場合のみ）
                            if let Some((
                                tool_name,
                                duration_seconds,
                                status,
                                ui_above_text,
                                previous_status,
                            )) = notification_info
                            {
                                Self::send_notification_if_needed(
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
                        let _ = ui_update_sender.send(());
                    } else if verbose {
                        eprintln!(
                            "⚠️  Invalid message format from {}: {}",
                            connection_id,
                            buffer.trim()
                        );
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("📡 Read error from {connection_id}: {e}");
                    }

                    // エラーで接続が切断された場合も、関連するlauncherを削除
                    Self::cleanup_disconnected_launcher(&connection_id, &session_manager, verbose)
                        .await;

                    break;
                }
            }
        }

        // 接続情報をクリーンアップ
        connections.write().await.remove(&connection_id);
        
        // 接続終了処理
        Self::cleanup_disconnected_launcher(&connection_id, &session_manager, verbose).await;
        let _ = ui_update_sender.send(());

        Ok(())
    }

    /// 切断されたlauncherのクリーンアップ（既存のロジックを再利用）
    async fn cleanup_disconnected_launcher(
        connection_id: &str,
        session_manager: &Arc<RwLock<SessionManager>>,
        verbose: bool,
    ) {
        let mut manager = session_manager.write().await;

        // まず、connection_idをlauncher_idとして直接削除を試行
        if let Some(removed_launcher) = manager.remove_launcher(connection_id) {
            if verbose {
                println!(
                    "🗑️  Removed launcher by connection ID: {} ({})",
                    connection_id,
                    removed_launcher
                        .project
                        .unwrap_or_else(|| "unknown".to_string())
                );
            }
            return;
        }

        // 直接削除できない場合は、connection_idからlauncher_idを推測
        let launcher_ids = manager.get_launcher_ids();
        let mut launcher_ids_to_remove = Vec::new();

        for launcher_id in launcher_ids {
            if launcher_id.contains(connection_id) || connection_id.contains(&launcher_id) {
                launcher_ids_to_remove.push(launcher_id);
            }
        }

        for launcher_id in launcher_ids_to_remove {
            if let Some(removed_launcher) = manager.remove_launcher(&launcher_id) {
                if verbose {
                    println!(
                        "🗑️  Removed disconnected launcher: {} ({})",
                        launcher_id,
                        removed_launcher
                            .project
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                }
            }
        }
    }

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
    async fn shutdown(
        &mut self,
        server_transport: &mut ServerTransport,
    ) -> Result<()> {
        // 全タスクを終了
        for handle in self.task_handles.drain(..) {
            handle.abort();
        }

        // サーバー終了
        server_transport.shutdown().await?;

        if self.verbose {
            println!("✅ Monitor server shutdown complete");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = ConnectionConfig::default_unix();
        let server = TransportMonitorServer::new(config, false, None);
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_tcp_server_creation() {
        let config = ConnectionConfig::default_tcp();
        let server = TransportMonitorServer::new(config, false, None);
        assert!(server.is_ok());
    }
}