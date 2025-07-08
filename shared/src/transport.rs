use anyhow::Result;
#[cfg(unix)]
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// 接続設定
#[derive(Debug, Clone)]
pub enum ConnectionConfig {
    #[cfg(unix)]
    Unix { socket_path: PathBuf },
}

impl ConnectionConfig {
    /// デフォルトのUnix socket設定
    #[cfg(unix)]
    pub fn default_unix() -> Self {
        Self::Unix {
            socket_path: std::env::temp_dir().join("climonitor.sock"),
        }
    }

    /// 環境変数から設定を読み込み
    #[cfg(unix)]
    pub fn from_env() -> Self {
        if let Ok(socket_path) = std::env::var("CLIMONITOR_SOCKET_PATH") {
            return Self::Unix {
                socket_path: socket_path.into(),
            };
        }
        Self::default_unix()
    }
}

/// 抽象化された接続ストリーム
pub enum Connection {
    #[cfg(unix)]
    Unix {
        reader: BufReader<tokio::net::unix::OwnedReadHalf>,
        writer: tokio::net::unix::OwnedWriteHalf,
        peer_addr: String,
    },
}

impl Connection {
    pub async fn read_line(&mut self, buf: &mut String) -> Result<usize> {
        match self {
            #[cfg(unix)]
            Connection::Unix { reader, .. } => Ok(reader.read_line(buf).await?),
        }
    }

    pub async fn write_all(&mut self, data: &[u8]) -> Result<()> {
        match self {
            #[cfg(unix)]
            Connection::Unix { writer, .. } => {
                writer.write_all(data).await?;
                Ok(())
            }
        }
    }

    pub async fn flush(&mut self) -> Result<()> {
        match self {
            #[cfg(unix)]
            Connection::Unix { writer, .. } => {
                writer.flush().await?;
                Ok(())
            }
        }
    }

    pub fn peer_addr(&self) -> &str {
        match self {
            #[cfg(unix)]
            Connection::Unix { peer_addr, .. } => peer_addr,
        }
    }
}

/// サーバー側のトランスポート
pub enum ServerTransport {
    #[cfg(unix)]
    Unix {
        listener: tokio::net::UnixListener,
        socket_path: PathBuf,
    },
}

impl ServerTransport {
    pub async fn bind(config: &ConnectionConfig) -> Result<Self> {
        match config {
            #[cfg(unix)]
            ConnectionConfig::Unix { socket_path } => {
                // 既存のソケットファイルを削除
                if socket_path.exists() {
                    tokio::fs::remove_file(socket_path).await?;
                }

                let listener = tokio::net::UnixListener::bind(socket_path)?;
                Ok(Self::Unix {
                    listener,
                    socket_path: socket_path.clone(),
                })
            }
        }
    }

    pub async fn accept(&mut self, _config: &ConnectionConfig) -> Result<Connection> {
        match self {
            #[cfg(unix)]
            ServerTransport::Unix {
                listener,
                socket_path,
            } => {
                let (stream, _) = listener.accept().await?;
                let (reader, writer) = stream.into_split();
                let peer_addr = format!("unix:{}", socket_path.display());

                Ok(Connection::Unix {
                    reader: BufReader::new(reader),
                    writer,
                    peer_addr,
                })
            }
        }
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            #[cfg(unix)]
            ServerTransport::Unix { socket_path, .. } => {
                if socket_path.exists() {
                    tokio::fs::remove_file(socket_path).await?;
                }
                Ok(())
            }
        }
    }
}

/// クライアント側のトランスポート
pub struct ClientTransport;

impl ClientTransport {
    pub async fn connect(config: &ConnectionConfig) -> Result<Connection> {
        match config {
            #[cfg(unix)]
            ConnectionConfig::Unix { socket_path } => {
                let stream = tokio::net::UnixStream::connect(socket_path).await?;
                let (reader, writer) = stream.into_split();
                let peer_addr = format!("unix:{}", socket_path.display());

                Ok(Connection::Unix {
                    reader: BufReader::new(reader),
                    writer,
                    peer_addr,
                })
            }
        }
    }
}

/// 設定に応じて適切なサーバートランスポートを作成
pub async fn create_server_transport(config: &ConnectionConfig) -> Result<ServerTransport> {
    ServerTransport::bind(config).await
}

/// 設定に応じて適切なクライアントで接続
pub async fn connect_client(config: &ConnectionConfig) -> Result<Connection> {
    ClientTransport::connect(config).await
}
