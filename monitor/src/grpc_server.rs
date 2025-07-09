use crate::session_manager::SessionManager;
use anyhow::Result;
use climonitor_shared::grpc::{
    monitor_service_server::{MonitorService, MonitorServiceServer},
    ConnectResponse, LauncherMessage, MonitorMessage,
};
use climonitor_shared::message_conversion::grpc_conversion;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};

pub struct CliMonitorService {
    session_manager: Arc<tokio::sync::RwLock<SessionManager>>,
    ui_tx: tokio::sync::broadcast::Sender<()>,
}

impl CliMonitorService {
    pub fn new(
        session_manager: Arc<tokio::sync::RwLock<SessionManager>>,
        ui_tx: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            session_manager,
            ui_tx,
        }
    }
}

#[tonic::async_trait]
impl MonitorService for CliMonitorService {
    type StreamSessionStream = ReceiverStream<Result<MonitorMessage, Status>>;

    async fn stream_session(
        &self,
        request: Request<Streaming<LauncherMessage>>,
    ) -> Result<Response<Self::StreamSessionStream>, Status> {
        let mut stream = request.into_inner();
        let session_manager = self.session_manager.clone();

        let (tx, rx) = mpsc::channel(100);

        // ストリーム処理を別タスクで実行
        let ui_tx = self.ui_tx.clone();
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(launcher_msg) => {
                        if let Err(e) = Self::handle_launcher_message(
                            &session_manager,
                            &ui_tx,
                            launcher_msg,
                            &tx,
                        )
                        .await
                        {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::Grpc,
                                "⚠️  Error handling launcher message: {e}"
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::Grpc,
                            "⚠️  Error receiving launcher message: {e}"
                        );
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

impl CliMonitorService {
    async fn handle_launcher_message(
        session_manager: &Arc<tokio::sync::RwLock<SessionManager>>,
        ui_tx: &tokio::sync::broadcast::Sender<()>,
        launcher_msg: LauncherMessage,
        tx: &mpsc::Sender<Result<MonitorMessage, Status>>,
    ) -> Result<()> {
        if let Some(ref message) = launcher_msg.message {
            // 接続メッセージの場合は応答を送信（messageをcloneする前に処理）
            let response_opt = if let climonitor_shared::grpc::launcher_message::Message::Connect(
                ref connect_req,
            ) = message
            {
                Some(MonitorMessage {
                    message: Some(
                        climonitor_shared::grpc::monitor_message::Message::ConnectResponse(
                            ConnectResponse {
                                launcher_id: connect_req.launcher_id.clone(),
                                success: true,
                                message: Some("Connected successfully".to_string()),
                            },
                        ),
                    ),
                })
            } else {
                None
            };

            // gRPCメッセージを既存のprotocolに変換
            let protocol_msg = grpc_conversion::from_grpc_launcher_message(launcher_msg)?;

            // 既存のSessionManagerで処理
            {
                let mut manager = session_manager.write().await;
                if let Err(e) = manager.handle_message(protocol_msg) {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::Grpc,
                        "⚠️  Session manager error: {e}"
                    );
                }
            }

            // UI更新チャネルにメッセージを送信
            if let Err(e) = ui_tx.send(()) {
                climonitor_shared::log_warn!(
                    climonitor_shared::LogCategory::Grpc,
                    "⚠️  Failed to send UI update: {e}"
                );
            }

            // 応答を送信
            if let Some(response) = response_opt {
                if let Err(e) = tx.send(Ok(response)).await {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::Grpc,
                        "⚠️  Failed to send connect response: {e}"
                    );
                }
            }
        }

        Ok(())
    }
}

pub async fn start_grpc_server(
    session_manager: Arc<tokio::sync::RwLock<SessionManager>>,
    ui_tx: tokio::sync::broadcast::Sender<()>,
    bind_addr: &str,
) -> Result<()> {
    let addr = bind_addr.parse()?;
    let service = CliMonitorService::new(session_manager, ui_tx);

    println!("🚀 Starting gRPC server on {addr}");

    Server::builder()
        .add_service(MonitorServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
