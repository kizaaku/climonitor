// テストフィクスチャとダミーデータ生成
// Note: 統合テスト用共通関数は複数の統合テストファイルから使用されるが、
// Rustコンパイラーは各統合テストを独立してコンパイルするため
// dead_code警告が発生する。実際には使用されているため警告を抑制。

#![cfg(test)]
#![allow(dead_code)]

use chrono::Utc;
use climonitor_shared::{
    CliToolType, LauncherInfo, LauncherStatus, LauncherToMonitor, SessionInfo, SessionStatus,
};
use std::path::PathBuf;

/// テスト用のランチャーからモニターへのメッセージを生成
pub fn create_test_launcher_message(
    launcher_id: String,
    message_type: TestMessageType,
) -> LauncherToMonitor {
    match message_type {
        TestMessageType::Register => LauncherToMonitor::Connect {
            launcher_id,
            project: Some("test-project".to_string()),
            tool_type: CliToolType::Claude,
            claude_args: vec!["--help".to_string()],
            working_dir: PathBuf::from("/tmp/test"),
            timestamp: Utc::now(),
        },
        TestMessageType::StatusUpdate => LauncherToMonitor::StateUpdate {
            launcher_id,
            session_id: "test_session".to_string(),
            status: SessionStatus::Busy,
            ui_above_text: Some("test UI text".to_string()),
            timestamp: Utc::now(),
        },
        // ProcessMetrics は削除済み
        TestMessageType::ProcessMetrics => LauncherToMonitor::Disconnect {
            launcher_id,
            timestamp: Utc::now(),
        },
        TestMessageType::Disconnect => LauncherToMonitor::Disconnect {
            launcher_id,
            timestamp: Utc::now(),
        },
    }
}

#[derive(Debug, Clone)]
pub enum TestMessageType {
    Register,
    StatusUpdate,
    ProcessMetrics, // 削除済み機能だが、後方互換性のため維持
    Disconnect,
}

/// 一意なテストIDを生成
pub fn generate_test_id() -> String {
    format!("test_{}", uuid::Uuid::new_v4())
}

/// テスト用のLauncherInfoを作成
pub fn create_test_launcher_info(launcher_id: String, tool_type: CliToolType) -> LauncherInfo {
    LauncherInfo {
        id: launcher_id,
        project: Some("test-project".to_string()),
        tool_type,
        claude_args: vec!["--help".to_string()],
        working_dir: PathBuf::from("/tmp/test"),
        connected_at: Utc::now(),
        last_activity: Utc::now(),
        status: LauncherStatus::Connected,
    }
}

/// テスト用のSessionInfoを作成
pub fn create_test_session_info(launcher_id: String, status: SessionStatus) -> SessionInfo {
    SessionInfo {
        id: format!("{launcher_id}_session"),
        launcher_id,
        project: Some("test-project".to_string()),
        tool_type: Some(CliToolType::Claude),
        status,
        previous_status: None,
        // confidence フィールドは削除済み
        evidence: vec!["test evidence".to_string()],
        last_message: Some("test message".to_string()),
        created_at: Utc::now(),
        last_activity: Utc::now(),
        last_status_change: Utc::now(),
        launcher_context: Some("test context".to_string()),
        usage_reset_time: None,
        is_waiting_for_execution: false,
        ui_above_text: Some("test UI text".to_string()),
    }
}
