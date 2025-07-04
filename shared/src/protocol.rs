use crate::cli_tool::CliToolType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// セッション状態
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SessionStatus {
    Connected,    // 🔗 接続済み
    Busy,         // 🟢 実行中
    WaitingInput, // 🟡 確認待ち
    Idle,         // 🔵 完了/アイドル
    Error,        // 🔴 エラー
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.icon(), self.label())
    }
}

impl SessionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Connected => "🔗",
            Self::Busy => "🟢",
            Self::WaitingInput => "🟡",
            Self::Idle => "🔵",
            Self::Error => "🔴",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Connected => "接続済み",
            Self::Busy => "実行中",
            Self::WaitingInput => "確認待ち",
            Self::Idle => "完了",
            Self::Error => "エラー",
        }
    }
}

/// launcher → monitor へのメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LauncherToMonitor {
    /// launcher接続
    Connect {
        launcher_id: String,
        project: Option<String>,
        tool_type: CliToolType,
        claude_args: Vec<String>, // 互換性のため保持（将来はtool_argsに変更予定）
        working_dir: PathBuf,
        timestamp: DateTime<Utc>,
    },
    /// セッション状態更新
    StateUpdate {
        launcher_id: String,
        session_id: String,
        status: SessionStatus,
        ui_above_text: Option<String>, // UI box上の⏺文字以降の具体的なテキスト
        timestamp: DateTime<Utc>,
    },
    /// プロセス監視情報
    ProcessMetrics {
        launcher_id: String,
        cpu_percent: f32,
        memory_mb: u64,
        child_count: u32,
        network_active: bool,
        timestamp: DateTime<Utc>,
    },
    /// 出力キャプチャ
    OutputCapture {
        launcher_id: String,
        stream: String, // "stdout" or "stderr"
        content: String,
        timestamp: DateTime<Utc>,
    },
    /// launcher切断
    Disconnect {
        launcher_id: String,
        timestamp: DateTime<Utc>,
    },
}

/// monitor → launcher へのメッセージ（将来の拡張用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorToLauncher {
    /// 接続確認
    Ack,
    /// 詳細情報要求
    RequestMetrics,
    /// シャットダウン指示
    Shutdown,
    /// ログファイル設定
    SetLogFile { log_file_path: Option<PathBuf> },
}

/// launcher情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherInfo {
    pub id: String,
    pub project: Option<String>,
    pub tool_type: CliToolType,
    pub claude_args: Vec<String>, // 互換性のため保持
    pub working_dir: PathBuf,
    pub connected_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub status: LauncherStatus,
}

/// launcher状態
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LauncherStatus {
    Connected,
    Active,
    Idle,
    Disconnected,
}

/// セッション情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub launcher_id: String,
    pub project: Option<String>,
    pub tool_type: Option<CliToolType>,
    pub status: SessionStatus,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub last_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub last_status_change: DateTime<Utc>,
    pub launcher_context: Option<String>,
    pub usage_reset_time: Option<String>,
    pub is_waiting_for_execution: bool,
    pub ui_above_text: Option<String>, // UI box上の⏺文字以降の具体的なテキスト
}

/// プロセス監視データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetrics {
    pub launcher_id: String,
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub child_count: u32,
    pub network_active: bool,
    pub timestamp: DateTime<Utc>,
}

/// 接続ID生成
pub fn generate_connection_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("launcher-{timestamp:x}")
}
