use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use climonitor_shared::{
    transport::MessageSender, CliToolType, ConnectionConfig, LauncherToMonitor, SessionStatus,
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
                launcher_id: climonitor_shared::generate_connection_id(),
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
