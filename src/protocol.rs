use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// セッション状態
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,    // 🟢 作業中
    Approve,   // 🟡 承認待ち
    Finish,    // 🔵 完了
    Error,     // 🔴 エラー
    Idle,      // ⚪ アイドル
}

impl SessionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Active => "🟢",
            Self::Approve => "🟡", 
            Self::Finish => "🔵",
            Self::Error => "🔴",
            Self::Idle => "⚪",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "作業中",
            Self::Approve => "承認待ち",
            Self::Finish => "完了",
            Self::Error => "エラー",
            Self::Idle => "アイドル",
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
        claude_args: Vec<String>,
        working_dir: PathBuf,
        timestamp: DateTime<Utc>,
    },
    /// セッション状態更新
    StateUpdate {
        session_id: String,
        status: SessionStatus,
        confidence: f32,      // 推測の信頼度 (0.0-1.0)
        evidence: Vec<String>, // 判定根拠
        message: Option<String>, // 最新メッセージ
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
        stream: String,  // "stdout" or "stderr"
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
}

/// launcher情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherInfo {
    pub id: String,
    pub project: Option<String>,
    pub claude_args: Vec<String>,
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
    pub status: SessionStatus,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub last_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
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
    format!("launcher-{:x}", timestamp)
}

/// セッションID生成
pub fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("session-{:x}", timestamp)
}