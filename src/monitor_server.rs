use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

use crate::protocol::LauncherToMonitor;
use crate::session_manager::{SessionManager, SessionStats};

/// 接続情報
#[derive(Debug)]
struct Connection {
    id: String,
    stream: UnixStream,
    connected_at: chrono::DateTime<chrono::Utc>,
}

/// Monitor サーバー
pub struct MonitorServer {
    listener: Option<UnixListener>,
    socket_path: PathBuf,
    session_manager: Arc<RwLock<SessionManager>>,
    connections: Arc<RwLock<HashMap<String, Connection>>>,
    ui_update_sender: broadcast::Sender<()>,
    task_handles: Vec<JoinHandle<()>>,
    verbose: bool,
}

impl MonitorServer {
    pub fn new(verbose: bool) -> Result<Self> {
        let socket_path = Self::get_socket_path()?;
        let session_manager = Arc::new(RwLock::new(SessionManager::new()));
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let (ui_update_sender, _) = broadcast::channel(100);

        Ok(Self {
            listener: None,
            socket_path,
            session_manager,
            connections,
            ui_update_sender,
            task_handles: Vec::new(),
            verbose,
        })
    }

    /// サーバー開始
    pub async fn start(&mut self) -> Result<()> {
        // 既存のソケットファイルを削除
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path).await?;
        }

        // Unix Domain Socket リスナーを作成
        let listener = UnixListener::bind(&self.socket_path)?;
        self.listener = Some(listener);

        if self.verbose {
            println!("📡 Monitor server started at: {:?}", self.socket_path);
        }

        // 定期クリーンアップタスク開始
        self.start_cleanup_task().await;

        Ok(())
    }

    /// メインループ実行
    pub async fn run(&mut self) -> Result<()> {
        if self.listener.is_none() {
            return Err(anyhow::anyhow!("Server not started"));
        }

        if self.verbose {
            println!("⚡ Server running, waiting for launcher connections...");
        }

        loop {
            tokio::select! {
                // 新しい接続を受け入れ
                accept_result = async {
                    match &self.listener {
                        Some(listener) => listener.accept().await,
                        None => return Err(std::io::Error::new(std::io::ErrorKind::Other, "No listener")),
                    }
                } => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let connection_id = crate::protocol::generate_connection_id();
                            if self.verbose {
                                println!("🔗 New connection: {}", connection_id);
                            }
                            self.handle_new_connection(connection_id, stream).await?;
                        }
                        Err(e) => {
                            eprintln!("❌ Accept error: {}", e);
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

        self.shutdown().await?;
        Ok(())
    }

    /// 新しい接続を処理
    async fn handle_new_connection(&mut self, connection_id: String, stream: UnixStream) -> Result<()> {
        let connection = Connection {
            id: connection_id.clone(),
            stream,
            connected_at: chrono::Utc::now(),
        };

        // 接続を登録
        self.connections.write().await.insert(connection_id.clone(), connection);

        // 接続ハンドラータスクを開始
        let task_handle = self.spawn_connection_handler(connection_id).await;
        self.task_handles.push(task_handle);

        Ok(())
    }

    /// 接続ハンドラータスクを生成
    async fn spawn_connection_handler(&self, connection_id: String) -> JoinHandle<()> {
        let connections = Arc::clone(&self.connections);
        let session_manager = Arc::clone(&self.session_manager);
        let ui_update_sender = self.ui_update_sender.clone();
        let verbose = self.verbose;

        tokio::spawn(async move {
            if let Err(e) = Self::handle_connection(
                connection_id.clone(),
                connections,
                session_manager,
                ui_update_sender,
                verbose,
            ).await {
                if verbose {
                    eprintln!("⚠️  Connection {} error: {}", connection_id, e);
                }
            }
        })
    }

    /// 個別接続の処理
    async fn handle_connection(
        connection_id: String,
        connections: Arc<RwLock<HashMap<String, Connection>>>,
        session_manager: Arc<RwLock<SessionManager>>,
        ui_update_sender: broadcast::Sender<()>,
        verbose: bool,
    ) -> Result<()> {
        let stream = {
            let mut connections_guard = connections.write().await;
            connections_guard.remove(&connection_id)
                .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?
                .stream
        };

        let mut reader = BufReader::new(stream);
        let mut buffer = String::new();

        loop {
            buffer.clear();
            
            match reader.read_line(&mut buffer).await {
                Ok(0) => {
                    // 接続が閉じられた
                    if verbose {
                        println!("📴 Connection closed: {}", connection_id);
                    }
                    break;
                }
                Ok(_) => {
                    // メッセージを受信
                    if let Ok(message) = serde_json::from_str::<LauncherToMonitor>(buffer.trim()) {
                        if verbose {
                            println!("📨 Message from {}: {:?}", connection_id, message);
                        }

                        // セッションマネージャーで処理
                        if let Err(e) = session_manager.write().await.handle_message(message) {
                            eprintln!("⚠️  Message handling error: {}", e);
                        }

                        // UI更新通知
                        let _ = ui_update_sender.send(());
                    } else {
                        if verbose {
                            eprintln!("⚠️  Invalid message format from {}: {}", connection_id, buffer.trim());
                        }
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("📡 Read error from {}: {}", connection_id, e);
                    }
                    break;
                }
            }
        }

        // 接続終了処理
        session_manager.write().await.remove_launcher(&connection_id);
        let _ = ui_update_sender.send(());

        Ok(())
    }

    /// 定期クリーンアップタスク開始
    async fn start_cleanup_task(&mut self) {
        let session_manager = Arc::clone(&self.session_manager);
        let verbose = self.verbose;

        let cleanup_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5分間隔
            
            loop {
                interval.tick().await;
                
                session_manager.write().await.cleanup_old_sessions();
                
                if verbose {
                    println!("🧹 Cleanup completed");
                }
            }
        });

        self.task_handles.push(cleanup_handle);
    }

    /// UI更新通知受信用
    pub fn subscribe_ui_updates(&self) -> broadcast::Receiver<()> {
        self.ui_update_sender.subscribe()
    }

    /// 現在の統計情報取得
    pub async fn get_stats(&self) -> SessionStats {
        self.session_manager.read().await.get_stats()
    }

    /// セッションマネージャー取得
    pub fn get_session_manager(&self) -> Arc<RwLock<SessionManager>> {
        Arc::clone(&self.session_manager)
    }

    /// サーバー終了
    async fn shutdown(&mut self) -> Result<()> {
        // 全タスクを終了
        for handle in self.task_handles.drain(..) {
            handle.abort();
        }

        // ソケットファイル削除
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path).await?;
        }

        if self.verbose {
            println!("✅ Monitor server shutdown complete");
        }

        Ok(())
    }

    /// ソケットパス取得
    fn get_socket_path() -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        Ok(temp_dir.join("ccmonitor.sock"))
    }

    /// 外部クライアント用のソケットパス取得
    pub fn get_client_socket_path() -> Result<PathBuf> {
        Self::get_socket_path()
    }
}

impl Drop for MonitorServer {
    fn drop(&mut self) {
        // ソケットファイルをクリーンアップ
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let server = MonitorServer::new(false);
        assert!(server.is_ok());
    }

    #[test]
    fn test_socket_path() {
        let path = MonitorServer::get_client_socket_path().unwrap();
        assert!(path.to_string_lossy().contains("ccmonitor.sock"));
    }
}