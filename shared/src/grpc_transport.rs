use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status, Streaming};

use crate::{
    grpc::{
        monitor_service_client::MonitorServiceClient, monitor_service_server::MonitorService,
        monitor_service_server::MonitorServiceServer, LauncherMessage as GrpcLauncherMessage,
        MonitorMessage,
    },
    message_conversion::grpc_conversion,
    transport::{MessageHandler, MessageReceiver, MessageSender},
    CliToolType, ConnectionConfig, LauncherToMonitor, SessionStatus,
};

/// gRPC クライアント実装
pub struct GrpcMessageSender {
    client: MonitorServiceClient<Channel>,
    launcher_id: String,
    tx: mpsc::Sender<GrpcLauncherMessage>,
}

impl GrpcMessageSender {
    pub async fn new(config: &ConnectionConfig) -> Result<Self> {
        match config {
            ConnectionConfig::Grpc { bind_addr, .. } => {
                let endpoint = if bind_addr.starts_with("http://") || bind_addr.starts_with("https://") {
                    bind_addr.clone()
                } else {
                    format!("http://{}", bind_addr)
                };

                let client = MonitorServiceClient::connect(endpoint).await?;
                let launcher_id = crate::generate_connection_id();

                let (tx, rx) = mpsc::channel(100);
                let stream = ReceiverStream::new(rx);

                // gRPCストリーミング接続を開始
                let mut client_clone = client.clone();
                tokio::spawn(async move {
                    let response = client_clone.stream_session(stream).await;
                    match response {
                        Ok(response_stream) => {
                            let mut stream = response_stream.into_inner();
                            while let Some(result) = stream.next().await {
                                match result {
                                    Ok(_monitor_msg) => {
                                        // Monitor → Launcherメッセージの処理（現在は未実装）
                                    }
                                    Err(e) => {
                                        eprintln!("⚠️  Error receiving monitor message: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("⚠️  gRPC stream error: {}", e);
                        }
                    }
                });

                Ok(Self {
                    client,
                    launcher_id,
                    tx,
                })
            }
            _ => anyhow::bail!("gRPC transport requires gRPC configuration"),
        }
    }

    async fn send_grpc_message(&self, message: LauncherToMonitor) -> Result<()> {
        let grpc_message = grpc_conversion::to_grpc_launcher_message(message)?;
        self.tx.send(grpc_message).await.map_err(|e| {
            anyhow::anyhow!("Failed to send gRPC message: {}", e)
        })?;
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
        project_name: Option<String>,
    ) -> Result<()> {
        let message = LauncherToMonitor::StateUpdate {
            launcher_id: self.launcher_id.clone(),
            session_id,
            status,
            timestamp,
            ui_above_text: None,
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

/// gRPC サーバー実装
pub struct GrpcMessageReceiver {
    bind_addr: String,
    allowed_ips: Vec<String>,
    handler: Arc<dyn MessageHandler>,
}

impl GrpcMessageReceiver {
    pub async fn new(config: &ConnectionConfig, handler: Box<dyn MessageHandler>) -> Result<Self> {
        match config {
            ConnectionConfig::Grpc {
                bind_addr,
                allowed_ips,
            } => Ok(Self {
                bind_addr: bind_addr.clone(),
                allowed_ips: allowed_ips.clone(),
                handler: Arc::from(handler),
            }),
            _ => anyhow::bail!("gRPC transport requires gRPC configuration"),
        }
    }
}

#[async_trait]
impl MessageReceiver for GrpcMessageReceiver {
    async fn start_server(&mut self) -> Result<()> {
        let addr = SocketAddr::from_str(&self.bind_addr)?;
        let service = GrpcMonitorService {
            handler: self.handler.clone(),
            allowed_ips: self.allowed_ips.clone(),
        };

        println!("🚀 gRPC server listening on: {}", addr);

        Server::builder()
            .add_service(MonitorServiceServer::new(service))
            .serve(addr)
            .await?;

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        println!("🛑 gRPC server shutdown");
        Ok(())
    }
}

/// gRPC サービス実装
struct GrpcMonitorService {
    handler: Arc<dyn MessageHandler>,
    allowed_ips: Vec<String>,
}

#[async_trait]
impl MonitorService for GrpcMonitorService {
    type StreamSessionStream = ReceiverStream<Result<MonitorMessage, Status>>;

    async fn stream_session(
        &self,
        request: Request<Streaming<GrpcLauncherMessage>>,
    ) -> Result<Response<Self::StreamSessionStream>, Status> {
        // IP許可チェック
        if let Some(remote_addr) = request.remote_addr() {
            if !crate::ip_utils::is_ip_allowed_by_list(&remote_addr.ip(), &self.allowed_ips) {
                return Err(Status::permission_denied(format!(
                    "IP address {} is not allowed",
                    remote_addr.ip()
                )));
            }
        }

        let mut stream = request.into_inner();
        let handler = self.handler.clone();

        // 入力ストリームを処理
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(grpc_message) => {
                        // gRPCメッセージを内部プロトコルに変換
                        match grpc_conversion::from_grpc_launcher_message(grpc_message) {
                            Ok(message) => {
                                // ハンドラーに渡す
                                if let Err(e) = handler.handle_message(message).await {
                                    eprintln!("⚠️  Failed to handle gRPC message: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("⚠️  Failed to convert gRPC message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️  gRPC stream error: {}", e);
                        break;
                    }
                }
            }
        });

        // 空のレスポンスストリームを返す（現在Monitor→Launcherメッセージは未実装）
        let (tx, rx) = mpsc::channel(1);
        drop(tx); // すぐに閉じる
        let output_stream = ReceiverStream::new(rx);

        Ok(Response::new(output_stream))
    }
}