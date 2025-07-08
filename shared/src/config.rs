use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::transport::ConnectionConfig;

/// メインの設定構造体
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// gRPC関連の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcSettings {
    /// gRPCサーバーのバインドアドレス
    #[serde(default = "default_grpc_bind_addr")]
    pub bind_addr: String,

    /// IP許可リスト
    #[serde(default)]
    pub allowed_ips: Vec<String>,
}

fn default_grpc_bind_addr() -> String {
    "127.0.0.1:50051".to_string()
}

/// 接続関連の設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionSettings {
    /// Unix socket接続時のソケットパス
    pub unix_socket_path: Option<PathBuf>,

    /// gRPC接続設定
    pub grpc: Option<GrpcSettings>,
}

/// ログ関連の設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingSettings {
    /// 詳細ログを有効にするか
    #[serde(default)]
    pub verbose: bool,

    /// ログファイルのパス（実装済み：CLIツールの出力保存用）
    pub log_file: Option<PathBuf>,
}

/// 通知関連の設定（現在は実装されていない - ~/.climonitor/notify.sh が存在する場合のみ動作）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationSettings {
    /// 将来の実装用プレースホルダー
    #[serde(default)]
    _placeholder: bool,
}

/// UI関連の設定（現在は実装されていない - ハードコードされた値を使用）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiSettings {
    /// 将来の実装用プレースホルダー
    #[serde(default)]
    _placeholder: bool,
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
        if let Ok(socket_path) = std::env::var("CLIMONITOR_SOCKET_PATH") {
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
        // gRPC設定がある場合はgRPCを優先
        if let Some(ref grpc_config) = self.connection.grpc {
            ConnectionConfig::Grpc {
                bind_addr: grpc_config.bind_addr.clone(),
                allowed_ips: grpc_config.allowed_ips.clone(),
            }
        } else {
            #[cfg(unix)]
            {
                ConnectionConfig::Unix {
                    socket_path: self
                        .connection
                        .unix_socket_path
                        .clone()
                        .unwrap_or_else(|| std::env::temp_dir().join("climonitor.sock")),
                }
            }
            #[cfg(not(unix))]
            {
                // Unix以外のプラットフォームではgRPCをデフォルト使用
                ConnectionConfig::default_grpc()
            }
        }
    }

    /// 設定のサンプルを生成
    pub fn sample() -> Self {
        let mut config = Self::default();

        // サンプル値を設定
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
        assert!(config.connection.unix_socket_path.is_none());
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
unix_socket_path = "/tmp/test.sock"

[logging]
verbose = true

# [notification] と [ui] セクションは未実装
"#;

        let config: Config = toml::from_str(toml_content).unwrap();
        assert_eq!(
            config.connection.unix_socket_path,
            Some(PathBuf::from("/tmp/test.sock"))
        );
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
        assert_eq!(
            loaded_config.connection.unix_socket_path,
            config.connection.unix_socket_path
        );

        // テスト用ファイルを削除
        std::fs::remove_file(&config_path).ok();
    }

    #[test]
    fn test_env_overrides() {
        let mut config = Config::default();

        // 環境変数を設定
        std::env::set_var("CLIMONITOR_SOCKET_PATH", "/tmp/test.sock");
        std::env::set_var("CLIMONITOR_VERBOSE", "true");

        config.apply_env_overrides();

        assert_eq!(
            config.connection.unix_socket_path,
            Some(PathBuf::from("/tmp/test.sock"))
        );
        assert!(config.logging.verbose);

        // 環境変数をクリア
        std::env::remove_var("CLIMONITOR_SOCKET_PATH");
        std::env::remove_var("CLIMONITOR_VERBOSE");
    }

    #[cfg(unix)]
    #[test]
    fn test_to_connection_config() {
        let mut config = Config::default();

        // Unix設定をテスト
        config.connection.unix_socket_path = Some(PathBuf::from("/tmp/test.sock"));

        match config.to_connection_config() {
            ConnectionConfig::Unix { socket_path } => {
                assert_eq!(socket_path, PathBuf::from("/tmp/test.sock"));
            }
            ConnectionConfig::Grpc { .. } => {
                panic!("Expected Unix config, got Grpc");
            }
        }

        // デフォルトパスをテスト
        config.connection.unix_socket_path = None;
        match config.to_connection_config() {
            ConnectionConfig::Unix { socket_path } => {
                assert_eq!(socket_path, std::env::temp_dir().join("climonitor.sock"));
            }
            ConnectionConfig::Grpc { .. } => {
                panic!("Expected Unix config, got Grpc");
            }
        }
    }
}
