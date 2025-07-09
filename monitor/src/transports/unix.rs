use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use climonitor_shared::{
    transport::{MessageHandler, MessageReceiver},
    ConnectionConfig, LauncherToMonitor,
};

/// Unix Socket サーバー実装
pub struct UnixMessageReceiver {
    socket_path: PathBuf,
    handler: std::sync::Arc<dyn MessageHandler>,
}

impl UnixMessageReceiver {
    pub async fn new(config: &ConnectionConfig, handler: Box<dyn MessageHandler>) -> Result<Self> {
        match config {
            ConnectionConfig::Unix { socket_path } => Ok(Self {
                socket_path: socket_path.clone(),
                handler: std::sync::Arc::from(handler),
            }),
            _ => anyhow::bail!("Unix transport requires Unix socket configuration"),
        }
    }

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
                                climonitor_shared::log_warn!(
                                    climonitor_shared::LogCategory::UnixSocket,
                                    "⚠️  Failed to handle message: {e}"
                                );
                            }
                        }
                        Err(e) => {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::UnixSocket,
                                "⚠️  Failed to parse message: {e}"
                            );
                        }
                    }
                }
                Err(e) => {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::UnixSocket,
                        "⚠️  Failed to read from Unix socket: {e}"
                    );
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
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        println!(
            "🚀 Unix socket server listening on: {}",
            self.socket_path.display()
        );

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let handler = std::sync::Arc::clone(&self.handler);
                    // 各接続を並行処理
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection_static(&*handler, stream).await {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::UnixSocket,
                                "⚠️  Connection handling failed: {e}"
                            );
                        }
                    });
                }
                Err(e) => {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::UnixSocket,
                        "⚠️  Failed to accept Unix socket connection: {e}"
                    );
                }
            }
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        // ソケットファイルを削除
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }
        Ok(())
    }
}
