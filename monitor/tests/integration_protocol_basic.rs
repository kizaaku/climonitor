// 基本的なプロトコル統合テスト

#[cfg(test)]
mod common;

use climonitor_shared::{CliToolType, LauncherToMonitor, SessionStatus};
use common::{create_test_launcher_message, generate_test_id, TestMessageType};

#[test]
fn test_protocol_serialization_connect() {
    // Connect メッセージのシリアライゼーション/デシリアライゼーションテスト
    let launcher_id = generate_test_id();
    let original_message =
        create_test_launcher_message(launcher_id.clone(), TestMessageType::Register);

    // JSONにシリアライズ
    let json_str = serde_json::to_string(&original_message).unwrap();
    println!("Serialized: {json_str}");

    // JSONからデシリアライズ
    let deserialized_message: LauncherToMonitor = serde_json::from_str(&json_str).unwrap();

    // 元のメッセージと一致することを確認
    match (&original_message, &deserialized_message) {
        (
            LauncherToMonitor::Connect {
                launcher_id: orig_id,
                tool_type: orig_tool,
                ..
            },
            LauncherToMonitor::Connect {
                launcher_id: deser_id,
                tool_type: deser_tool,
                ..
            },
        ) => {
            assert_eq!(orig_id, deser_id);
            assert_eq!(orig_tool, deser_tool);
        }
        _ => panic!("メッセージタイプが一致しません"),
    }
}

#[test]
fn test_protocol_serialization_state_update() {
    // StateUpdate メッセージのシリアライゼーション/デシリアライゼーションテスト
    let launcher_id = generate_test_id();
    let original_message =
        create_test_launcher_message(launcher_id.clone(), TestMessageType::StatusUpdate);

    // JSONにシリアライズ
    let json_str = serde_json::to_string(&original_message).unwrap();

    // JSONからデシリアライズ
    let deserialized_message: LauncherToMonitor = serde_json::from_str(&json_str).unwrap();

    // 元のメッセージと一致することを確認
    match (&original_message, &deserialized_message) {
        (
            LauncherToMonitor::StateUpdate {
                launcher_id: orig_id,
                status: orig_status,
                ..
            },
            LauncherToMonitor::StateUpdate {
                launcher_id: deser_id,
                status: deser_status,
                ..
            },
        ) => {
            assert_eq!(orig_id, deser_id);
            assert_eq!(orig_status, deser_status);
            assert_eq!(*orig_status, SessionStatus::Busy);
        }
        _ => panic!("メッセージタイプが一致しません"),
    }
}

// ProcessMetrics テストは削除済み（機能削除のため）

#[test]
fn test_protocol_serialization_disconnect() {
    // Disconnect メッセージのシリアライゼーション/デシリアライゼーションテスト
    let launcher_id = generate_test_id();
    let original_message =
        create_test_launcher_message(launcher_id.clone(), TestMessageType::Disconnect);

    // JSONにシリアライズ
    let json_str = serde_json::to_string(&original_message).unwrap();

    // JSONからデシリアライズ
    let deserialized_message: LauncherToMonitor = serde_json::from_str(&json_str).unwrap();

    // 元のメッセージと一致することを確認
    match (&original_message, &deserialized_message) {
        (
            LauncherToMonitor::Disconnect {
                launcher_id: orig_id,
                ..
            },
            LauncherToMonitor::Disconnect {
                launcher_id: deser_id,
                ..
            },
        ) => {
            assert_eq!(orig_id, deser_id);
        }
        _ => panic!("メッセージタイプが一致しません"),
    }
}

#[test]
fn test_session_status_display() {
    // SessionStatus の表示テスト
    let statuses = vec![
        SessionStatus::Connected,
        SessionStatus::Busy,
        SessionStatus::WaitingInput,
        SessionStatus::Idle,
        SessionStatus::Error,
    ];

    for status in statuses {
        let display_str = format!("{status}");
        let icon = status.icon();
        let label = status.label();

        assert!(!display_str.is_empty());
        assert!(!icon.is_empty());
        assert!(!label.is_empty());

        // アイコンと日本語ラベルが含まれていることを確認
        assert!(display_str.contains(icon));
        assert!(display_str.contains(label));
    }
}

#[test]
fn test_cli_tool_type_serialization() {
    // CliToolType のシリアライゼーションテスト
    let claude_type = CliToolType::Claude;
    let gemini_type = CliToolType::Gemini;

    // シリアライズ
    let claude_json = serde_json::to_string(&claude_type).unwrap();
    let gemini_json = serde_json::to_string(&gemini_type).unwrap();

    // デシリアライズ
    let claude_deserialized: CliToolType = serde_json::from_str(&claude_json).unwrap();
    let gemini_deserialized: CliToolType = serde_json::from_str(&gemini_json).unwrap();

    assert_eq!(claude_type, claude_deserialized);
    assert_eq!(gemini_type, gemini_deserialized);
}
