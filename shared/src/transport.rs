use anyhow::{Result, anyhow};
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// 接続設定
#[derive(Debug, Clone)]
pub enum ConnectionConfig {
    Unix {
        socket_path: PathBuf,
    },
    Tcp {
        bind_addr: String, // "0.0.0.0:3001" or "localhost:3001"
        allowed_ips: Vec<String>, // IP許可リスト
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
            allowed_ips: Vec::new(),
        }
    }

    /// 環境変数から設定を読み込み
    pub fn from_env() -> Self {
        if let Ok(tcp_addr) = std::env::var("CLIMONITOR_TCP_ADDR") {
            Self::Tcp {
                bind_addr: tcp_addr,
                allowed_ips: Vec::new(),
            }
        } else if let Ok(socket_path) = std::env::var("CLIMONITOR_SOCKET_PATH") {
            Self::Unix {
                socket_path: socket_path.into(),
            }
        } else {
            Self::default_unix()
        }
    }

    /// TCP接続のIP許可チェック
    pub fn is_ip_allowed(&self, peer_addr: &std::net::SocketAddr) -> bool {
        match self {
            ConnectionConfig::Unix { .. } => true, // Unix socketは常に許可
            ConnectionConfig::Tcp { allowed_ips, .. } => {
                // 許可リストが空の場合は全て許可
                if allowed_ips.is_empty() {
                    return true;
                }

                let peer_ip = peer_addr.ip();
                
                for allowed_pattern in allowed_ips {
                    if is_ip_match(&peer_ip, allowed_pattern) {
                        return true;
                    }
                }
                
                false
            }
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
            ConnectionConfig::Tcp { bind_addr, .. } => {
                let listener = tokio::net::TcpListener::bind(bind_addr).await?;
                Ok(Self::Tcp { listener })
            }
        }
    }

    pub async fn accept(&mut self, config: &ConnectionConfig) -> Result<Connection> {
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
                
                // IP許可チェック
                if !config.is_ip_allowed(&addr) {
                    return Err(anyhow!("Connection from {} is not allowed", addr.ip()));
                }
                
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
            ConnectionConfig::Tcp { bind_addr, .. } => {
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

/// IPアドレスと許可パターンのマッチング
fn is_ip_match(ip: &IpAddr, pattern: &str) -> bool {
    // 完全一致
    if let Ok(allowed_ip) = IpAddr::from_str(pattern) {
        return *ip == allowed_ip;
    }
    
    // CIDR記法のサポート
    if let Some((network, prefix_len)) = pattern.split_once('/') {
        if let (Ok(network_ip), Ok(prefix)) = (IpAddr::from_str(network), prefix_len.parse::<u8>()) {
            return is_ip_in_network(ip, &network_ip, prefix);
        }
    }
    
    // 特別なパターン
    match pattern {
        "localhost" => {
            *ip == IpAddr::from([127, 0, 0, 1]) || *ip == IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1])
        }
        "any" | "*" => true,
        _ => false,
    }
}

/// IPアドレスがネットワークに含まれるかチェック
fn is_ip_in_network(ip: &IpAddr, network: &IpAddr, prefix_len: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => {
            if prefix_len > 32 { return false; }
            let mask = if prefix_len == 0 { 0 } else { !((1u32 << (32 - prefix_len)) - 1) };
            (u32::from(*ip) & mask) == (u32::from(*net) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(net)) => {
            if prefix_len > 128 { return false; }
            let ip_bytes = ip.octets();
            let net_bytes = net.octets();
            
            let full_bytes = prefix_len / 8;
            let remaining_bits = prefix_len % 8;
            
            // 完全な8ビット単位でのマッチング
            if ip_bytes[..full_bytes as usize] != net_bytes[..full_bytes as usize] {
                return false;
            }
            
            // 残りのビットでのマッチング
            if remaining_bits > 0 {
                let mask = 0xFF << (8 - remaining_bits);
                let ip_byte = ip_bytes[full_bytes as usize];
                let net_byte = net_bytes[full_bytes as usize];
                if (ip_byte & mask) != (net_byte & mask) {
                    return false;
                }
            }
            
            true
        }
        _ => false, // IPv4とIPv6の混在は許可しない
    }
}