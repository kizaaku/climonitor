use anyhow::Result;
use climonitor_shared::{transport::MessageSender, ConnectionConfig};

pub mod grpc;
#[cfg(unix)]
pub mod unix;

/// クライアント用ファクトリー
pub async fn create_message_sender(config: &ConnectionConfig) -> Result<Box<dyn MessageSender>> {
    match config {
        #[cfg(unix)]
        ConnectionConfig::Unix { .. } => {
            let sender = unix::UnixMessageSender::new(config).await?;
            Ok(Box::new(sender))
        }
        ConnectionConfig::Grpc { .. } => {
            let sender = grpc::GrpcMessageSender::new(config).await?;
            Ok(Box::new(sender))
        }
    }
}

/// 指定されたlauncher_idでクライアント用MessageSenderを作成
pub async fn create_message_sender_with_id(
    config: &ConnectionConfig,
    launcher_id: String,
) -> Result<Box<dyn MessageSender>> {
    match config {
        #[cfg(unix)]
        ConnectionConfig::Unix { .. } => {
            let sender = unix::UnixMessageSender::new_with_launcher_id(config, launcher_id).await?;
            Ok(Box::new(sender))
        }
        ConnectionConfig::Grpc { .. } => {
            // gRPCの場合は既存のIDを使用する仕組みが必要だが、今回はUnix socket問題の修正のみ
            let sender = grpc::GrpcMessageSender::new(config).await?;
            Ok(Box::new(sender))
        }
    }
}
