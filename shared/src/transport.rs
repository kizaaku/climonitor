use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;

use crate::{LauncherToMonitor, SessionStatus, CliToolType};

/// 接続設定
#[derive(Debug, Clone)]
pub enum ConnectionConfig {
    #[cfg(unix)]
    Unix { socket_path: std::path::PathBuf },
    Grpc {
        bind_addr: String,        // "0.0.0.0:50051" or "localhost:50051"
        allowed_ips: Vec<String>, // IP許可リスト
    },
}

impl ConnectionConfig {
    /// デフォルトのUnix socket設定
    #[cfg(unix)]
    pub fn default_unix() -> Self {
        Self::Unix {
            socket_path: std::env::temp_dir().join("climonitor.sock"),
        }
    }

    /// デフォルトのgRPC設定
    pub fn default_grpc() -> Self {
        Self::Grpc {
            bind_addr: "127.0.0.1:50051".to_string(),
            allowed_ips: Vec::new(),
        }
    }

    /// 環境変数から設定を読み込み
    pub fn from_env() -> Self {
        if let Ok(grpc_addr) = std::env::var("CLIMONITOR_GRPC_ADDR") {
            return Self::Grpc {
                bind_addr: grpc_addr,
                allowed_ips: Vec::new(),
            };
        }
        #[cfg(unix)]
        {
            if let Ok(socket_path) = std::env::var("CLIMONITOR_SOCKET_PATH") {
                return Self::Unix {
                    socket_path: socket_path.into(),
                };
            }
            Self::default_unix()
        }
        #[cfg(not(unix))]
        {
            Self::default_grpc()
        }
    }

    /// IP許可チェック
    pub fn is_ip_allowed(&self, peer_addr: &SocketAddr) -> bool {
        match self {
            #[cfg(unix)]
            ConnectionConfig::Unix { .. } => true, // Unix socketは常に許可
            ConnectionConfig::Grpc { allowed_ips, .. } => {
                crate::ip_utils::is_ip_allowed_by_list(&peer_addr.ip(), allowed_ips)
            }
        }
    }
}

/// 抽象的なクライアント送信インターフェース
#[async_trait]
pub trait MessageSender: Send + Sync {
    async fn send_connect(
        &self,
        project: Option<String>,
        tool_type: CliToolType,
        args: Vec<String>,
        working_dir: std::path::PathBuf,
    ) -> Result<()>;

    async fn send_status_update(
        &self,
        session_id: String,
        status: SessionStatus,
        timestamp: chrono::DateTime<chrono::Utc>,
        project_name: Option<String>,
    ) -> Result<()>;

    async fn send_context_update(
        &self,
        session_id: String,
        ui_text: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<()>;

    async fn send_disconnect(&self, session_id: String) -> Result<()>;
}

/// 抽象的なサーバーインターフェース
#[async_trait]
pub trait MessageReceiver: Send + Sync {
    async fn start_server(&mut self) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}

/// クライアント用ファクトリー
pub async fn create_message_sender(config: &ConnectionConfig) -> Result<Box<dyn MessageSender>> {
    match config {
        #[cfg(unix)]
        ConnectionConfig::Unix { .. } => {
            let sender = crate::unix_transport::UnixMessageSender::new(config).await?;
            Ok(Box::new(sender))
        }
        ConnectionConfig::Grpc { .. } => {
            let sender = crate::grpc_transport::GrpcMessageSender::new(config).await?;
            Ok(Box::new(sender))
        }
    }
}

/// サーバー用ファクトリー
pub async fn create_message_receiver(
    config: &ConnectionConfig,
    handler: Box<dyn MessageHandler>,
) -> Result<Box<dyn MessageReceiver>> {
    match config {
        #[cfg(unix)]
        ConnectionConfig::Unix { .. } => {
            let receiver = crate::unix_transport::UnixMessageReceiver::new(config, handler).await?;
            Ok(Box::new(receiver))
        }
        ConnectionConfig::Grpc { .. } => {
            let receiver = crate::grpc_transport::GrpcMessageReceiver::new(config, handler).await?;
            Ok(Box::new(receiver))
        }
    }
}

/// メッセージハンドラートレイト
#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle_message(&self, message: LauncherToMonitor) -> Result<()>;
}