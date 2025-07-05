use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::transport::ConnectionConfig;

/// メインの設定構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct Config {
    /// 接続設定
    #[serde(default)]
    pub connection: ConnectionSettings,

    /// ログ設定
    #[serde(default)]
    pub logging: LoggingSettings,

    /// 通知設定
    #[serde(default)]
    pub notification: NotificationSettings,

    /// UI設定
    #[serde(default)]
    pub ui: UiSettings,
}

/// 接続関連の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSettings {
    /// 接続タイプ ("unix" または "tcp")
    #[serde(default = "default_connection_type")]
    pub r#type: String,

    /// TCP接続時のバインドアドレス
    #[serde(default = "default_tcp_bind_addr")]
    pub tcp_bind_addr: String,

    /// Unix socket接続時のソケットパス
    pub unix_socket_path: Option<PathBuf>,

    /// TCP接続時のIP許可リスト（空の場合は全て許可）
    #[serde(default)]
    pub tcp_allowed_ips: Vec<String>,
}

/// ログ関連の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct LoggingSettings {
    /// 詳細ログを有効にするか
    #[serde(default)]
    pub verbose: bool,

    /// ログファイルのパス（実装済み：CLIツールの出力保存用）
    pub log_file: Option<PathBuf>,
}

/// 通知関連の設定（現在は実装されていない - ~/.climonitor/notify.sh が存在する場合のみ動作）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct NotificationSettings {
    /// 将来の実装用プレースホルダー
    #[serde(default)]
    _placeholder: bool,
}

/// UI関連の設定（現在は実装されていない - ハードコードされた値を使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct UiSettings {
    /// 将来の実装用プレースホルダー
    #[serde(default)]
    _placeholder: bool,
}


impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            r#type: default_connection_type(),
            tcp_bind_addr: default_tcp_bind_addr(),
            unix_socket_path: None,
            tcp_allowed_ips: Vec::new(),
        }
    }
}




// デフォルト値関数
fn default_connection_type() -> String {
    "unix".to_string()
}

fn default_tcp_bind_addr() -> String {
    "127.0.0.1:3001".to_string()
}

impl Config {
    /// 設定ファイルから読み込み
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.as_ref().display()))?;

        Ok(config)
    }

    /// 設定ファイルに保存
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        // ディレクトリが存在しない場合は作成
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config file: {}", path.as_ref().display()))?;

        Ok(())
    }

    /// デフォルトの設定ファイルパスを取得
    pub fn default_config_path() -> Result<PathBuf> {
        let home_dir = home::home_dir().context("Failed to get home directory")?;

        Ok(home_dir.join(".climonitor").join("config.toml"))
    }

    /// 設定ファイルパスの候補を取得（優先順位順）
    pub fn config_path_candidates() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // 1. カレントディレクトリの .climonitor/config.toml
        if let Ok(current_dir) = std::env::current_dir() {
            paths.push(current_dir.join(".climonitor").join("config.toml"));
        }

        // 2. ホームディレクトリの .climonitor/config.toml
        if let Some(home_dir) = home::home_dir() {
            paths.push(home_dir.join(".climonitor").join("config.toml"));
        }

        // 3. XDG規格に従った設定ディレクトリ（Linux/Unix）
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            paths.push(
                PathBuf::from(xdg_config_home)
                    .join("climonitor")
                    .join("config.toml"),
            );
        } else if let Some(home_dir) = home::home_dir() {
            paths.push(
                home_dir
                    .join(".config")
                    .join("climonitor")
                    .join("config.toml"),
            );
        }

        paths
    }

    /// 設定ファイルを自動検出して読み込み
    pub fn load_auto() -> Result<Option<(Self, PathBuf)>> {
        for path in Self::config_path_candidates() {
            if path.exists() {
                let config = Self::from_file(&path)?;
                return Ok(Some((config, path)));
            }
        }
        Ok(None)
    }

    /// 環境変数で設定を上書き
    pub fn apply_env_overrides(&mut self) {
        // 接続設定
        if let Ok(tcp_addr) = std::env::var("CLIMONITOR_TCP_ADDR") {
            self.connection.r#type = "tcp".to_string();
            self.connection.tcp_bind_addr = tcp_addr;
        }

        if let Ok(socket_path) = std::env::var("CLIMONITOR_SOCKET_PATH") {
            self.connection.r#type = "unix".to_string();
            self.connection.unix_socket_path = Some(PathBuf::from(socket_path));
        }

        // ログ設定
        if let Ok(verbose) = std::env::var("CLIMONITOR_VERBOSE") {
            self.logging.verbose = verbose == "1" || verbose.to_lowercase() == "true";
        }

        if let Ok(log_file) = std::env::var("CLIMONITOR_LOG_FILE") {
            self.logging.log_file = Some(PathBuf::from(log_file));
        }
    }

    /// 設定からConnectionConfigを生成
    pub fn to_connection_config(&self) -> ConnectionConfig {
        match self.connection.r#type.as_str() {
            "tcp" => ConnectionConfig::Tcp {
                bind_addr: self.connection.tcp_bind_addr.clone(),
                allowed_ips: self.connection.tcp_allowed_ips.clone(),
            },
            _ => ConnectionConfig::Unix {
                socket_path: self
                    .connection
                    .unix_socket_path
                    .clone()
                    .unwrap_or_else(|| std::env::temp_dir().join("climonitor.sock")),
            },
        }
    }

    /// 設定のサンプルを生成
    pub fn sample() -> Self {
        let mut config = Self::default();

        // サンプル値を設定
        config.connection.r#type = "unix".to_string();
        config.connection.tcp_bind_addr = "127.0.0.1:3001".to_string();
        config.connection.unix_socket_path = Some(PathBuf::from("/tmp/climonitor.sock"));

        config.logging.verbose = false;
        config.logging.log_file = Some(PathBuf::from("~/.climonitor/climonitor.log"));

        // 通知設定とUI設定は現在未実装

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.connection.r#type, "unix");
        assert_eq!(config.connection.tcp_bind_addr, "127.0.0.1:3001");
        assert!(!config.logging.verbose);
        assert!(!config.notification._placeholder);
        assert!(!config.ui._placeholder);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::sample();
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // TOMLとして正しくシリアライズできることを確認
        assert!(toml_str.contains("[connection]"));
        assert!(toml_str.contains("[logging]"));
        assert!(toml_str.contains("[notification]"));
        assert!(toml_str.contains("[ui]"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_content = r#"
[connection]
type = "tcp"
tcp_bind_addr = "0.0.0.0:3002"

[logging]
verbose = true

# [notification] と [ui] セクションは未実装
"#;

        let config: Config = toml::from_str(toml_content).unwrap();
        assert_eq!(config.connection.r#type, "tcp");
        assert_eq!(config.connection.tcp_bind_addr, "0.0.0.0:3002");
        assert!(config.logging.verbose);
        // 通知とUI設定は未実装のためテストなし
    }

    #[test]
    fn test_config_file_operations() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_climonitor_config.toml");

        // 設定ファイルを作成
        let config = Config::sample();
        config.save_to_file(&config_path).unwrap();

        // 設定ファイルから読み込み
        let loaded_config = Config::from_file(&config_path).unwrap();

        // 基本的な設定が正しく保存・読み込みされることを確認
        assert_eq!(loaded_config.connection.r#type, config.connection.r#type);
        assert_eq!(
            loaded_config.connection.tcp_bind_addr,
            config.connection.tcp_bind_addr
        );

        // テスト用ファイルを削除
        std::fs::remove_file(&config_path).ok();
    }

    #[test]
    fn test_env_overrides() {
        let mut config = Config::default();

        // 環境変数を設定
        std::env::set_var("CLIMONITOR_TCP_ADDR", "192.168.1.1:4000");
        std::env::set_var("CLIMONITOR_VERBOSE", "true");

        config.apply_env_overrides();

        assert_eq!(config.connection.r#type, "tcp");
        assert_eq!(config.connection.tcp_bind_addr, "192.168.1.1:4000");
        assert!(config.logging.verbose);

        // 環境変数をクリア
        std::env::remove_var("CLIMONITOR_TCP_ADDR");
        std::env::remove_var("CLIMONITOR_VERBOSE");
    }

    #[test]
    fn test_to_connection_config() {
        let mut config = Config::default();

        // Unix設定をテスト
        config.connection.r#type = "unix".to_string();
        config.connection.unix_socket_path = Some(PathBuf::from("/tmp/test.sock"));

        match config.to_connection_config() {
            ConnectionConfig::Unix { socket_path } => {
                assert_eq!(socket_path, PathBuf::from("/tmp/test.sock"));
            }
            _ => panic!("Expected Unix connection config"),
        }

        // TCP設定をテスト
        config.connection.r#type = "tcp".to_string();
        config.connection.tcp_bind_addr = "localhost:8080".to_string();

        match config.to_connection_config() {
            ConnectionConfig::Tcp {
                bind_addr,
                allowed_ips,
            } => {
                assert_eq!(bind_addr, "localhost:8080");
                assert!(allowed_ips.is_empty());
            }
            _ => panic!("Expected TCP connection config"),
        }

        // TCP IP許可リスト設定をテスト
        config.connection.tcp_allowed_ips = vec![
            "127.0.0.1".to_string(),
            "192.168.1.0/24".to_string(),
            "localhost".to_string(),
        ];

        match config.to_connection_config() {
            ConnectionConfig::Tcp {
                bind_addr,
                allowed_ips,
            } => {
                assert_eq!(bind_addr, "localhost:8080");
                assert_eq!(allowed_ips.len(), 3);
                assert!(allowed_ips.contains(&"127.0.0.1".to_string()));
                assert!(allowed_ips.contains(&"192.168.1.0/24".to_string()));
                assert!(allowed_ips.contains(&"localhost".to_string()));
            }
            _ => panic!("Expected TCP connection config"),
        }
    }

    #[test]
    fn test_ip_allow_list_functionality() {
        use crate::ConnectionConfig;
        use std::net::SocketAddr;
        use std::str::FromStr;

        // 許可リストが空の場合（全て許可）
        let config = ConnectionConfig::Tcp {
            bind_addr: "127.0.0.1:3001".to_string(),
            allowed_ips: vec![],
        };

        let addr = SocketAddr::from_str("192.168.1.100:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        // 特定IPを許可
        let config = ConnectionConfig::Tcp {
            bind_addr: "127.0.0.1:3001".to_string(),
            allowed_ips: vec!["127.0.0.1".to_string(), "192.168.1.100".to_string()],
        };

        // 許可されたIP
        let addr = SocketAddr::from_str("127.0.0.1:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        let addr = SocketAddr::from_str("192.168.1.100:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        // 許可されていないIP
        let addr = SocketAddr::from_str("192.168.1.101:12345").unwrap();
        assert!(!config.is_ip_allowed(&addr));

        // CIDR記法のテスト
        let config = ConnectionConfig::Tcp {
            bind_addr: "127.0.0.1:3001".to_string(),
            allowed_ips: vec!["192.168.1.0/24".to_string()],
        };

        // ネットワーク内のIP
        let addr = SocketAddr::from_str("192.168.1.50:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        let addr = SocketAddr::from_str("192.168.1.255:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        // ネットワーク外のIP
        let addr = SocketAddr::from_str("192.168.2.1:12345").unwrap();
        assert!(!config.is_ip_allowed(&addr));

        // 特別なパターンのテスト
        let config = ConnectionConfig::Tcp {
            bind_addr: "127.0.0.1:3001".to_string(),
            allowed_ips: vec!["localhost".to_string()],
        };

        // localhostパターン
        let addr = SocketAddr::from_str("127.0.0.1:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        let addr = SocketAddr::from_str("[::1]:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));

        // localhost以外
        let addr = SocketAddr::from_str("192.168.1.1:12345").unwrap();
        assert!(!config.is_ip_allowed(&addr));

        // Unix socketは常に許可
        let config = ConnectionConfig::Unix {
            socket_path: PathBuf::from("/tmp/test.sock"),
        };

        let addr = SocketAddr::from_str("0.0.0.0:12345").unwrap();
        assert!(config.is_ip_allowed(&addr));
    }
}
