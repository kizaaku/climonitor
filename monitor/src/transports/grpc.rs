use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

use climonitor_shared::{
    grpc::monitor_service_server::{MonitorService, MonitorServiceServer},
    grpc::{LauncherMessage as GrpcLauncherMessage, MonitorMessage},
    message_conversion as grpc_conversion,
    transport::{MessageHandler, MessageReceiver},
    ConnectionConfig,
};

/// gRPC メッセージレシーバー実装
pub struct GrpcMessageReceiver {
    bind_addr: String,
    allowed_ips: Vec<String>,
    handler: std::sync::Arc<dyn MessageHandler>,
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
                handler: std::sync::Arc::from(handler),
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
            handler: std::sync::Arc::clone(&self.handler),
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
        // gRPCサーバーのシャットダウン処理
        Ok(())
    }
}

/// gRPC サービス実装
struct GrpcMonitorService {
    handler: std::sync::Arc<dyn MessageHandler>,
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
            if !climonitor_shared::ip_utils::is_ip_allowed_by_list(
                &remote_addr.ip(),
                &self.allowed_ips,
            ) {
                return Err(Status::permission_denied(format!(
                    "IP address {} is not allowed",
                    remote_addr.ip()
                )));
            }
        }

        let mut stream = request.into_inner();
        let handler = std::sync::Arc::clone(&self.handler);

        // 入力ストリームを処理
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(grpc_message) => {
                        // gRPCメッセージを内部プロトコルに変換
                        match grpc_conversion::grpc_conversion::from_grpc_launcher_message(
                            grpc_message,
                        ) {
                            Ok(message) => {
                                // ハンドラーに渡す
                                if let Err(e) = handler.handle_message(message).await {
                                    eprintln!("⚠️  Failed to handle gRPC message: {e}");
                                }
                            }
                            Err(e) => {
                                eprintln!("⚠️  Failed to convert gRPC message: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️  gRPC stream error: {e}");
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
