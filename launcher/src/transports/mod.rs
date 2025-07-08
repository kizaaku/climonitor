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
