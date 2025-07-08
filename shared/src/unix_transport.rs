use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::{
    transport::{MessageHandler, MessageReceiver, MessageSender},
    CliToolType, ConnectionConfig, LauncherToMonitor, SessionStatus,
};

/// Unix Socket クライアント実装
pub struct UnixMessageSender {
    socket_path: PathBuf,
    launcher_id: String,
}

impl UnixMessageSender {
    pub async fn new(config: &ConnectionConfig) -> Result<Self> {
        match config {
            ConnectionConfig::Unix { socket_path } => Ok(Self {
                socket_path: socket_path.clone(),
                launcher_id: crate::generate_connection_id(),
            }),
            _ => anyhow::bail!("Unix transport requires Unix socket configuration"),
        }
    }

    async fn send_message(&self, message: LauncherToMonitor) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (_reader, mut writer) = stream.into_split();

        // メッセージをJSONにシリアライズして送信
        let json = serde_json::to_string(&message)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        Ok(())
    }
}

#[async_trait]
impl MessageSender for UnixMessageSender {
    async fn send_connect(
        &self,
        project: Option<String>,
        tool_type: CliToolType,
        args: Vec<String>,
        working_dir: PathBuf,
    ) -> Result<()> {
        let message = LauncherToMonitor::Connect {
            launcher_id: self.launcher_id.clone(),
            project,
            tool_type,
            claude_args: args,
            working_dir,
            timestamp: Utc::now(),
        };
        self.send_message(message).await
    }

    async fn send_status_update(
        &self,
        session_id: String,
        status: SessionStatus,
        timestamp: DateTime<Utc>,
        _project_name: Option<String>,
    ) -> Result<()> {
        let message = LauncherToMonitor::StateUpdate {
            launcher_id: self.launcher_id.clone(),
            session_id,
            status,
            timestamp,
            ui_above_text: None,
        };
        self.send_message(message).await
    }

    async fn send_context_update(
        &self,
        session_id: String,
        ui_text: String,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        let message = LauncherToMonitor::ContextUpdate {
            launcher_id: self.launcher_id.clone(),
            session_id,
            ui_above_text: Some(ui_text),
            timestamp,
        };
        self.send_message(message).await
    }

    async fn send_disconnect(&self, _session_id: String) -> Result<()> {
        let message = LauncherToMonitor::Disconnect {
            launcher_id: self.launcher_id.clone(),
            timestamp: Utc::now(),
        };
        self.send_message(message).await
    }
}

/// Unix Socket サーバー実装
pub struct UnixMessageReceiver {
    socket_path: PathBuf,
    handler: std::sync::Arc<dyn MessageHandler>,
    _listener: Option<UnixListener>,
}

impl UnixMessageReceiver {
    pub async fn new(config: &ConnectionConfig, handler: Box<dyn MessageHandler>) -> Result<Self> {
        match config {
            ConnectionConfig::Unix { socket_path } => Ok(Self {
                socket_path: socket_path.clone(),
                handler: std::sync::Arc::from(handler),
                _listener: None,
            }),
            _ => anyhow::bail!("Unix transport requires Unix socket configuration"),
        }
    }

    #[allow(dead_code)]
    async fn handle_connection(&self, stream: UnixStream) -> Result<()> {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // 接続終了
                Ok(_) => {
                    // JSONメッセージをデシリアライズ
                    match serde_json::from_str::<LauncherToMonitor>(line.trim()) {
                        Ok(message) => {
                            if let Err(e) = self.handler.handle_message(message).await {
                                eprintln!("⚠️  Failed to handle message: {e}");
                            }
                        }
                        Err(e) => {
                            eprintln!("⚠️  Failed to parse message: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to read from Unix socket: {e}");
                    break;
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl MessageReceiver for UnixMessageReceiver {
    async fn start_server(&mut self) -> Result<()> {
        // 既存のソケットファイルを削除
        if self.socket_path.exists() {
            tokio::fs::remove_file(&self.socket_path).await?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        println!(
            "🚀 Unix socket server listening on: {}",
            self.socket_path.display()
        );

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handler = std::sync::Arc::clone(&self.handler);
                    // 各接続を並行処理
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection_static(&*handler, stream).await {
                            eprintln!("⚠️  Connection handling failed: {e}");
                        }
                    });
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to accept Unix socket connection: {e}");
                }
            }
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        if self.socket_path.exists() {
            tokio::fs::remove_file(&self.socket_path).await?;
        }
        println!("🛑 Unix socket server shutdown");
        Ok(())
    }
}

impl UnixMessageReceiver {
    // staticメソッドでhandler参照を回避
    async fn handle_connection_static(
        handler: &dyn MessageHandler,
        stream: UnixStream,
    ) -> Result<()> {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // 接続終了
                Ok(_) => {
                    // JSONメッセージをデシリアライズ
                    match serde_json::from_str::<LauncherToMonitor>(line.trim()) {
                        Ok(message) => {
                            if let Err(e) = handler.handle_message(message).await {
                                eprintln!("⚠️  Failed to handle message: {e}");
                            }
                        }
                        Err(e) => {
                            eprintln!("⚠️  Failed to parse message: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to read from Unix socket: {e}");
                    break;
                }
            }
        }

        Ok(())
    }
}
