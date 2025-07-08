use anyhow::Result;
use climonitor_shared::{
    transport::{MessageHandler, MessageReceiver},
    ConnectionConfig,
};

pub mod grpc;
#[cfg(unix)]
pub mod unix;

/// サーバー用ファクトリー
pub async fn create_message_receiver(
    config: &ConnectionConfig,
    handler: Box<dyn MessageHandler>,
) -> Result<Box<dyn MessageReceiver>> {
    match config {
        #[cfg(unix)]
        ConnectionConfig::Unix { .. } => {
            let receiver = unix::UnixMessageReceiver::new(config, handler).await?;
            Ok(Box::new(receiver))
        }
        ConnectionConfig::Grpc { .. } => {
            let receiver = grpc::GrpcMessageReceiver::new(config, handler).await?;
            Ok(Box::new(receiver))
        }
    }
}
