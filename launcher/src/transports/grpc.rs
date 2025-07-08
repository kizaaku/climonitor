use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

use climonitor_shared::{
    grpc::monitor_service_client::MonitorServiceClient,
    grpc::LauncherMessage as GrpcLauncherMessage, message_conversion as grpc_conversion,
    transport::MessageSender, CliToolType, ConnectionConfig, LauncherToMonitor, SessionStatus,
};

/// gRPC クライアント実装
pub struct GrpcMessageSender {
    _client: MonitorServiceClient<Channel>,
    launcher_id: String,
    tx: mpsc::Sender<GrpcLauncherMessage>,
}

impl GrpcMessageSender {
    pub async fn new(config: &ConnectionConfig) -> Result<Self> {
        match config {
            ConnectionConfig::Grpc { bind_addr, .. } => {
                let endpoint =
                    if bind_addr.starts_with("http://") || bind_addr.starts_with("https://") {
                        bind_addr.clone()
                    } else {
                        format!("http://{bind_addr}")
                    };

                let client = MonitorServiceClient::connect(endpoint).await?;
                let launcher_id = climonitor_shared::generate_connection_id();

                let (tx, rx) = mpsc::channel(100);
                let input_stream = ReceiverStream::new(rx);

                let mut client_clone = client.clone();
                tokio::spawn(async move {
                    match client_clone.stream_session(input_stream).await {
                        Ok(response) => {
                            let mut stream = response.into_inner();
                            while let Ok(Some(_message)) = stream.message().await {
                                // Monitor → Launcherメッセージの処理（現在は未実装）
                            }
                        }
                        Err(e) => {
                            eprintln!("⚠️  gRPC stream error: {e}");
                        }
                    }
                });

                Ok(Self {
                    _client: client,
                    launcher_id,
                    tx,
                })
            }
            _ => anyhow::bail!("gRPC transport requires gRPC configuration"),
        }
    }

    async fn send_grpc_message(&self, message: LauncherToMonitor) -> Result<()> {
        let grpc_message = grpc_conversion::grpc_conversion::to_grpc_launcher_message(message)?;
        self.tx
            .send(grpc_message)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send gRPC message: {}", e))?;
        Ok(())
    }
}

#[async_trait]
impl MessageSender for GrpcMessageSender {
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
        self.send_grpc_message(message).await
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
        self.send_grpc_message(message).await
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
        self.send_grpc_message(message).await
    }

    async fn send_disconnect(&self, _session_id: String) -> Result<()> {
        let message = LauncherToMonitor::Disconnect {
            launcher_id: self.launcher_id.clone(),
            timestamp: Utc::now(),
        };
        self.send_grpc_message(message).await
    }
}
