// リグレッション検出のテスト

#[cfg(test)]
mod common;

use climonitor_shared::{CliToolType, LauncherToMonitor, SessionStatus};
use common::{create_test_launcher_message, TestMessageType};

#[test]
fn test_protocol_backward_compatibility() {
    // プロトコルの下位互換性テスト
    // 既知のJSONフォーマットが正しくデシリアライズできることを確認

    let claude_connect_json = r#"{
        "Connect": {
            "launcher_id": "test_launcher_123",
            "project": "test-project",
            "tool_type": "Claude",
            "claude_args": ["--help"],
            "working_dir": "/tmp/test",
            "timestamp": "2024-01-01T00:00:00Z"
        }
    }"#;

    let result: Result<LauncherToMonitor, _> = serde_json::from_str(claude_connect_json);
    assert!(result.is_ok());

    let message = result.unwrap();
    match message {
        LauncherToMonitor::Connect {
            launcher_id,
            tool_type,
            ..
        } => {
            assert_eq!(launcher_id, "test_launcher_123");
            assert_eq!(tool_type, CliToolType::Claude);
        }
        _ => panic!("予期しないメッセージタイプ"),
    }
}

#[test]
fn test_gemini_tool_support() {
    // Geminiツールサポートのリグレッションテスト
    let gemini_connect_json = r#"{
        "Connect": {
            "launcher_id": "gemini_launcher_456",
            "project": "gemini-project",
            "tool_type": "Gemini",
            "claude_args": ["--model", "gemini-pro"],
            "working_dir": "/tmp/gemini",
            "timestamp": "2024-01-01T00:00:00Z"
        }
    }"#;

    let result: Result<LauncherToMonitor, _> = serde_json::from_str(gemini_connect_json);
    assert!(result.is_ok());

    let message = result.unwrap();
    match message {
        LauncherToMonitor::Connect {
            launcher_id,
            tool_type,
            ..
        } => {
            assert_eq!(launcher_id, "gemini_launcher_456");
            assert_eq!(tool_type, CliToolType::Gemini);
        }
        _ => panic!("予期しないメッセージタイプ"),
    }
}

#[test]
fn test_session_status_enum_completeness() {
    // SessionStatus enumの完全性テスト
    // 新しい状態が追加された場合のリグレッション検出

    let all_statuses = vec![
        SessionStatus::Connected,
        SessionStatus::Busy,
        SessionStatus::WaitingInput,
        SessionStatus::Idle,
        SessionStatus::Error,
    ];

    // 各状態がJSON シリアライゼーション/デシリアライゼーションできることを確認
    for status in all_statuses {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: SessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);

        // アイコンとラベルが空でないことを確認
        assert!(!status.icon().is_empty());
        assert!(!status.label().is_empty());
    }
}

#[test]
fn test_cli_tool_type_enum_stability() {
    // CliToolType enumの安定性テスト
    // 既存の値が変更されていないことを確認

    let claude_json = serde_json::to_string(&CliToolType::Claude).unwrap();
    let gemini_json = serde_json::to_string(&CliToolType::Gemini).unwrap();

    // 既知の文字列表現が変わっていないことを確認
    assert_eq!(claude_json, "\"Claude\"");
    assert_eq!(gemini_json, "\"Gemini\"");

    // 逆方向の変換も確認
    let claude_from_json: CliToolType = serde_json::from_str("\"Claude\"").unwrap();
    let gemini_from_json: CliToolType = serde_json::from_str("\"Gemini\"").unwrap();

    assert_eq!(claude_from_json, CliToolType::Claude);
    assert_eq!(gemini_from_json, CliToolType::Gemini);
}

#[test]
fn test_message_type_structure_stability() {
    // メッセージタイプ構造の安定性テスト
    // 既存フィールドが削除されていないことを確認

    let test_launcher_id = "stability_test_launcher".to_string();

    // 各メッセージタイプが期待される構造を持っていることを確認
    let connect_msg =
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::Register);
    let state_msg =
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::StatusUpdate);
    let metrics_msg =
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::ProcessMetrics);
    let disconnect_msg =
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::Disconnect);

    // 各メッセージがシリアライズ可能であることを確認
    assert!(serde_json::to_string(&connect_msg).is_ok());
    assert!(serde_json::to_string(&state_msg).is_ok());
    assert!(serde_json::to_string(&metrics_msg).is_ok());
    assert!(serde_json::to_string(&disconnect_msg).is_ok());

    // 必須フィールドが存在することを確認
    match connect_msg {
        LauncherToMonitor::Connect {
            launcher_id,
            tool_type,
            working_dir,
            ..
        } => {
            assert_eq!(launcher_id, "stability_test_launcher");
            assert_eq!(tool_type, CliToolType::Claude);
            assert!(!working_dir.as_os_str().is_empty());
        }
        _ => panic!("Connectメッセージの構造が変わっています"),
    }

    match state_msg {
        LauncherToMonitor::StateUpdate {
            launcher_id,
            status,
            ..
        } => {
            assert_eq!(launcher_id, "stability_test_launcher");
            assert_eq!(status, SessionStatus::Busy);
        }
        _ => panic!("StateUpdateメッセージの構造が変わっています"),
    }
}

#[test]
fn test_timestamp_field_presence() {
    // タイムスタンプフィールドの存在確認テスト
    // プロトコルメッセージにタイムスタンプが含まれていることを確認

    let test_launcher_id = "timestamp_test_launcher".to_string();
    let messages = vec![
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::Register),
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::StatusUpdate),
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::ProcessMetrics),
        create_test_launcher_message(test_launcher_id.clone(), TestMessageType::Disconnect),
    ];

    for message in messages {
        let json_value: serde_json::Value = serde_json::to_value(&message).unwrap();

        // 各メッセージタイプにタイムスタンプフィールドが存在することを確認
        match message {
            LauncherToMonitor::Connect { .. } => {
                assert!(json_value["Connect"]["timestamp"].is_string());
            }
            LauncherToMonitor::StateUpdate { .. } => {
                assert!(json_value["StateUpdate"]["timestamp"].is_string());
            }
            LauncherToMonitor::ProcessMetrics { .. } => {
                assert!(json_value["ProcessMetrics"]["timestamp"].is_string());
            }
            LauncherToMonitor::Disconnect { .. } => {
                assert!(json_value["Disconnect"]["timestamp"].is_string());
            }
            _ => panic!("予期しないメッセージタイプ"),
        }
    }
}

#[test]
fn test_error_handling_robustness() {
    // エラーハンドリングの堅牢性テスト
    // 不正なJSONに対する適切なエラー処理を確認

    let invalid_jsons = vec![
        "{}",                     // 空のオブジェクト
        r#"{"InvalidType": {}}"#, // 未知のメッセージタイプ
        r#"{"Connect": {}}"#,     // 必須フィールドなし
        "invalid json",           // 不正なJSON
        "",                       // 空文字列
    ];

    for invalid_json in invalid_jsons {
        let result: Result<LauncherToMonitor, _> = serde_json::from_str(invalid_json);
        assert!(
            result.is_err(),
            "不正なJSON '{invalid_json}' が受け入れられました"
        );
    }
}

#[test]
fn test_unicode_support() {
    // Unicode サポートのテスト
    // 日本語などの多バイト文字が正しく処理されることを確認

    let unicode_project = "日本語プロジェクト🤖";
    let unicode_ui_text = "実行中です... 🔄";

    // Connect メッセージでのUnicode
    let connect_with_unicode = LauncherToMonitor::Connect {
        launcher_id: "unicode_test".to_string(),
        project: Some(unicode_project.to_string()),
        tool_type: CliToolType::Claude,
        claude_args: vec!["--project".to_string(), unicode_project.to_string()],
        working_dir: "/tmp/unicode_test".into(),
        timestamp: chrono::Utc::now(),
    };

    // StateUpdate メッセージでのUnicode
    let state_with_unicode = LauncherToMonitor::StateUpdate {
        launcher_id: "unicode_test".to_string(),
        session_id: "unicode_session".to_string(),
        status: SessionStatus::Busy,
        ui_above_text: Some(unicode_ui_text.to_string()),
        timestamp: chrono::Utc::now(),
    };

    // シリアライゼーション/デシリアライゼーション確認
    let connect_json = serde_json::to_string(&connect_with_unicode).unwrap();
    let state_json = serde_json::to_string(&state_with_unicode).unwrap();

    let connect_deserialized: LauncherToMonitor = serde_json::from_str(&connect_json).unwrap();
    let state_deserialized: LauncherToMonitor = serde_json::from_str(&state_json).unwrap();

    // Unicode文字が保持されていることを確認
    match connect_deserialized {
        LauncherToMonitor::Connect {
            project,
            claude_args,
            ..
        } => {
            assert_eq!(project, Some(unicode_project.to_string()));
            assert!(claude_args.contains(&unicode_project.to_string()));
        }
        _ => panic!("Connect メッセージの構造が異なります"),
    }

    match state_deserialized {
        LauncherToMonitor::StateUpdate { ui_above_text, .. } => {
            assert_eq!(ui_above_text, Some(unicode_ui_text.to_string()));
        }
        _ => panic!("StateUpdate メッセージの構造が異なります"),
    }
}
