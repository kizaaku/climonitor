use anyhow::Result;
use chrono::Utc;
use climonitor_shared::grpc::{
    monitor_service_client::MonitorServiceClient, LauncherMessage, MonitorMessage,
};
use climonitor_shared::message_conversion::grpc_conversion;
use climonitor_shared::{CliToolType, ConnectionConfig, LauncherToMonitor, SessionStatus};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::transport::Channel;

#[derive(Clone, Debug)]
pub struct GrpcTransportClient {
    #[allow(dead_code)]
    client: MonitorServiceClient<Channel>,
    tx: mpsc::Sender<LauncherMessage>,
    _handle: std::sync::Arc<tokio::task::JoinHandle<()>>,
}

impl GrpcTransportClient {
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let client = MonitorServiceClient::connect(endpoint.to_string()).await?;

        let (tx, rx) = mpsc::channel(100);
        let stream = ReceiverStream::new(rx);

        let mut client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let response = client_clone.stream_session(stream).await;

            match response {
                Ok(response_stream) => {
                    let mut stream = response_stream.into_inner();
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(monitor_msg) => {
                                Self::handle_monitor_message(monitor_msg).await;
                            }
                            Err(e) => {
                                eprintln!("⚠️  Error receiving monitor message: {e}");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  gRPC stream error: {e}");
                }
            }
        });

        Ok(Self {
            client,
            tx,
            _handle: std::sync::Arc::new(handle),
        })
    }

    pub async fn send_message(&self, message: LauncherToMonitor) -> Result<()> {
        let grpc_message = grpc_conversion::to_grpc_launcher_message(message)?;
        self.tx.send(grpc_message).await?;
        Ok(())
    }

    async fn handle_monitor_message(monitor_msg: MonitorMessage) {
        if let Some(message) = monitor_msg.message {
            match message {
                climonitor_shared::grpc::monitor_message::Message::ConnectResponse(resp) => {
                    if resp.success {
                        println!("✅ Connected to monitor: {}", resp.launcher_id);
                        if let Some(msg) = resp.message {
                            println!("📝 Message: {msg}");
                        }
                    } else {
                        println!("❌ Connection failed: {}", resp.launcher_id);
                    }
                }
                climonitor_shared::grpc::monitor_message::Message::RequestReconnect(req) => {
                    println!("🔄 Monitor requests reconnection: {}", req.reason);
                    // TODO: 再接続ロジックを実装
                }
                climonitor_shared::grpc::monitor_message::Message::Ping(ping) => {
                    println!("🏓 Ping received: sequence={}", ping.sequence);
                    // TODO: Pong応答を実装
                }
            }
        }
    }
}

// 既存のtransport_clientとの互換性のためのWrapper
#[derive(Clone, Debug)]
pub struct GrpcLauncherClient {
    grpc_client: Option<GrpcTransportClient>,
    launcher_id: String,
    session_id: String,
}

impl GrpcLauncherClient {
    pub async fn new(connection_config: &ConnectionConfig) -> Result<Self> {
        let launcher_id = climonitor_shared::generate_connection_id();
        let session_id = climonitor_shared::generate_connection_id();

        // gRPCエンドポイントを構築
        let endpoint = match connection_config {
            #[cfg(unix)]
            ConnectionConfig::Unix { .. } => {
                // Unix Socket は gRPC では直接サポートされていないため、デフォルトgRPCアドレスを使用
                "http://127.0.0.1:50051".to_string()
            }
            ConnectionConfig::Grpc { bind_addr, .. } => {
                // gRPC設定から接続アドレスを取得
                if bind_addr.starts_with("http://") || bind_addr.starts_with("https://") {
                    bind_addr.clone()
                } else {
                    format!("http://{}", bind_addr)
                }
            }
        };

        Self::new_with_endpoint(launcher_id, session_id, endpoint).await
    }

    pub async fn new_with_endpoint(
        launcher_id: String,
        session_id: String,
        endpoint: String,
    ) -> Result<Self> {
        let grpc_client = match GrpcTransportClient::connect(&endpoint).await {
            Ok(client) => Some(client),
            Err(e) => {
                eprintln!("⚠️  Failed to connect to gRPC monitor: {e}");
                None
            }
        };

        Ok(Self {
            grpc_client,
            launcher_id,
            session_id,
        })
    }

    pub async fn send_connect(
        &self,
        project: Option<String>,
        tool_type: CliToolType,
        claude_args: Vec<String>,
        working_dir: std::path::PathBuf,
    ) -> Result<()> {
        if let Some(client) = &self.grpc_client {
            let message = LauncherToMonitor::Connect {
                launcher_id: self.launcher_id.clone(),
                project,
                tool_type,
                claude_args,
                working_dir,
                timestamp: Utc::now(),
            };
            client.send_message(message).await?;
        }
        Ok(())
    }

    pub async fn send_state_update(
        &self,
        status: SessionStatus,
        ui_above_text: Option<String>,
    ) -> Result<()> {
        if let Some(client) = &self.grpc_client {
            let message = LauncherToMonitor::StateUpdate {
                launcher_id: self.launcher_id.clone(),
                session_id: self.session_id.clone(),
                status,
                ui_above_text,
                timestamp: Utc::now(),
            };
            client.send_message(message).await?;
        }
        Ok(())
    }

    pub async fn send_context_update(&self, ui_above_text: Option<String>) -> Result<()> {
        if let Some(client) = &self.grpc_client {
            let message = LauncherToMonitor::ContextUpdate {
                launcher_id: self.launcher_id.clone(),
                session_id: self.session_id.clone(),
                ui_above_text,
                timestamp: Utc::now(),
            };
            client.send_message(message).await?;
        }
        Ok(())
    }

    pub async fn send_disconnect(&self) -> Result<()> {
        if let Some(client) = &self.grpc_client {
            let message = LauncherToMonitor::Disconnect {
                launcher_id: self.launcher_id.clone(),
                timestamp: Utc::now(),
            };
            client.send_message(message).await?;
        }
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.grpc_client.is_some()
    }

    pub fn get_launcher_id(&self) -> &str {
        &self.launcher_id
    }

    pub fn get_session_id(&self) -> &str {
        &self.session_id
    }
}
