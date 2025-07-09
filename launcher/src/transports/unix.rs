use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use climonitor_shared::{
    transport::MessageSender, CliToolType, ConnectionConfig, LauncherToMonitor, SessionStatus,
};

/// Unix Socket クライアント実装
pub struct UnixMessageSender {
    socket_path: PathBuf,
    launcher_id: String,
    connection: Mutex<Option<UnixStream>>,
}

impl UnixMessageSender {
    pub async fn new(config: &ConnectionConfig) -> Result<Self> {
        match config {
            ConnectionConfig::Unix { socket_path } => Ok(Self {
                socket_path: socket_path.clone(),
                launcher_id: climonitor_shared::generate_connection_id(),
                connection: Mutex::new(None),
            }),
            _ => anyhow::bail!("Unix transport requires Unix socket configuration"),
        }
    }

    pub async fn new_with_launcher_id(
        config: &ConnectionConfig,
        launcher_id: String,
    ) -> Result<Self> {
        match config {
            ConnectionConfig::Unix { socket_path } => Ok(Self {
                socket_path: socket_path.clone(),
                launcher_id,
                connection: Mutex::new(None),
            }),
            _ => anyhow::bail!("Unix transport requires Unix socket configuration"),
        }
    }

    async fn send_message(&self, message: LauncherToMonitor) -> Result<()> {
        let mut connection_guard = self.connection.lock().await;

        // 接続がない場合、新しい接続を作成
        if connection_guard.is_none() {
            let stream = UnixStream::connect(&self.socket_path).await?;
            *connection_guard = Some(stream);
        }

        // 接続を取得
        let stream = connection_guard.as_mut().unwrap();

        // メッセージをJSONにシリアライズして送信
        let json = serde_json::to_string(&message)?;

        // 送信を試行
        match stream.write_all(json.as_bytes()).await {
            Ok(()) => {
                stream.write_all(b"\n").await?;
                stream.flush().await?;
                Ok(())
            }
            Err(_) => {
                // 接続が切れている場合、再接続して再試行
                let new_stream = UnixStream::connect(&self.socket_path).await?;
                *connection_guard = Some(new_stream);

                let stream = connection_guard.as_mut().unwrap();
                stream.write_all(json.as_bytes()).await?;
                stream.write_all(b"\n").await?;
                stream.flush().await?;
                Ok(())
            }
        }
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
            ui_above_text: None,
            timestamp,
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
