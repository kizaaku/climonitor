use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// 接続設定
#[derive(Debug, Clone)]
pub enum ConnectionConfig {
    Unix {
        socket_path: PathBuf,
    },
    Tcp {
        bind_addr: String, // "0.0.0.0:3001" or "localhost:3001"
    },
}

impl ConnectionConfig {
    /// デフォルトのUnix socket設定
    pub fn default_unix() -> Self {
        Self::Unix {
            socket_path: std::env::temp_dir().join("climonitor.sock"),
        }
    }

    /// デフォルトのTCP設定
    pub fn default_tcp() -> Self {
        Self::Tcp {
            bind_addr: "127.0.0.1:3001".to_string(),
        }
    }

    /// 環境変数から設定を読み込み
    pub fn from_env() -> Self {
        if let Ok(tcp_addr) = std::env::var("CLIMONITOR_TCP_ADDR") {
            Self::Tcp {
                bind_addr: tcp_addr,
            }
        } else if let Ok(socket_path) = std::env::var("CLIMONITOR_SOCKET_PATH") {
            Self::Unix {
                socket_path: socket_path.into(),
            }
        } else {
            Self::default_unix()
        }
    }
}

/// 抽象化された接続ストリーム
pub enum Connection {
    Unix {
        reader: BufReader<tokio::net::unix::OwnedReadHalf>,
        writer: tokio::net::unix::OwnedWriteHalf,
        peer_addr: String,
    },
    Tcp {
        reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
        writer: tokio::net::tcp::OwnedWriteHalf,
        peer_addr: String,
    },
}

impl Connection {
    pub async fn read_line(&mut self, buf: &mut String) -> Result<usize> {
        match self {
            Connection::Unix { reader, .. } => Ok(reader.read_line(buf).await?),
            Connection::Tcp { reader, .. } => Ok(reader.read_line(buf).await?),
        }
    }

    pub async fn write_all(&mut self, data: &[u8]) -> Result<()> {
        match self {
            Connection::Unix { writer, .. } => {
                writer.write_all(data).await?;
                Ok(())
            }
            Connection::Tcp { writer, .. } => {
                writer.write_all(data).await?;
                Ok(())
            }
        }
    }

    pub async fn flush(&mut self) -> Result<()> {
        match self {
            Connection::Unix { writer, .. } => {
                writer.flush().await?;
                Ok(())
            }
            Connection::Tcp { writer, .. } => {
                writer.flush().await?;
                Ok(())
            }
        }
    }

    pub fn peer_addr(&self) -> &str {
        match self {
            Connection::Unix { peer_addr, .. } => peer_addr,
            Connection::Tcp { peer_addr, .. } => peer_addr,
        }
    }
}

/// サーバー側のトランスポート
pub enum ServerTransport {
    Unix {
        listener: tokio::net::UnixListener,
        socket_path: PathBuf,
    },
    Tcp {
        listener: tokio::net::TcpListener,
    },
}

impl ServerTransport {
    pub async fn bind(config: &ConnectionConfig) -> Result<Self> {
        match config {
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
            ConnectionConfig::Tcp { bind_addr } => {
                let listener = tokio::net::TcpListener::bind(bind_addr).await?;
                Ok(Self::Tcp { listener })
            }
        }
    }

    pub async fn accept(&mut self) -> Result<Connection> {
        match self {
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
            ServerTransport::Tcp { listener } => {
                let (stream, addr) = listener.accept().await?;
                let (reader, writer) = stream.into_split();
                let peer_addr = format!("tcp:{}", addr);

                Ok(Connection::Tcp {
                    reader: BufReader::new(reader),
                    writer,
                    peer_addr,
                })
            }
        }
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            ServerTransport::Unix { socket_path, .. } => {
                if socket_path.exists() {
                    tokio::fs::remove_file(socket_path).await?;
                }
                Ok(())
            }
            ServerTransport::Tcp { .. } => {
                // TCPリスナーは自動的にドロップで閉じられる
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
            ConnectionConfig::Tcp { bind_addr } => {
                let stream = tokio::net::TcpStream::connect(bind_addr).await?;
                let addr = stream.peer_addr()?;
                let (reader, writer) = stream.into_split();
                let peer_addr = format!("tcp:{}", addr);

                Ok(Connection::Tcp {
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