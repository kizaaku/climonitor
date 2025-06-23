use crate::session::{SessionMessage, SessionStatus, MessageContent, ContentItem};
use chrono::Utc;

/// セッション状態を判定するモジュール
/// 
/// このモジュールはClaudeセッションメッセージから現在の状態を判定する
/// 判定ルール:
/// - tool_useメッセージから1秒経過 → Approve (承認待ち)
/// - textメッセージ → Finish (完了)
/// - userメッセージ → Active (作業中)

pub struct StatusDetector;

impl StatusDetector {
    /// メッセージからセッション状態を判定する
    /// 
    /// # 判定ルール
    /// 1. エラー検出（最優先）
    /// 2. メッセージ内容による判定:
    ///    - tool_use: 1秒後にApprove（承認待ち）、それまではActive
    ///    - text: Finish（完了）
    ///    - user: Active（作業中）
    /// 3. 5分以上経過: Idle
    pub fn determine_status(msg: &SessionMessage) -> SessionStatus {
        // エラーチェック（最優先）
        if Self::has_error(msg) {
            return SessionStatus::Error;
        }
        
        // 5分以上経過チェック
        let now = Utc::now();
        let time_diff = now.signed_duration_since(msg.timestamp);
        if time_diff.num_minutes() > 5 {
            return SessionStatus::Idle;
        }
        
        // メッセージタイプ別判定
        match msg.message_type.as_str() {
            "assistant" => Self::analyze_assistant_message(msg),
            "user" => SessionStatus::Active, // ユーザー入力後、作業中
            _ => SessionStatus::Active, // 基本は作業中
        }
    }
    
    /// エラーの存在をチェック
    fn has_error(msg: &SessionMessage) -> bool {
        if let Some(result) = &msg.tool_use_result {
            result.contains("Error") || result.contains("error")
        } else {
            false
        }
    }
    
    /// Assistantメッセージを解析
    fn analyze_assistant_message(msg: &SessionMessage) -> SessionStatus {
        // メッセージ内容のタイプで判定
        if let MessageContent::Assistant { content, .. } = &msg.message {
            for item in content {
                match item {
                    ContentItem::ToolUse { .. } => {
                        // tool_useメッセージから1秒経過チェック
                        let now = Utc::now();
                        let time_diff = now.signed_duration_since(msg.timestamp);
                        
                        if time_diff.num_seconds() >= 1 {
                            return SessionStatus::Approve; // 承認待ち
                        } else {
                            return SessionStatus::Active; // ツール実行中
                        }
                    },
                    ContentItem::Text { .. } => {
                        return SessionStatus::Finish; // テキスト応答 = 完了
                    }
                }
            }
        }
        
        // デフォルト: 時間による判定
        Self::fallback_status(msg)
    }
    
    /// フォールバック処理（時間による判定）
    fn fallback_status(msg: &SessionMessage) -> SessionStatus {
        let now = Utc::now();
        let time_diff = now.signed_duration_since(msg.timestamp);
        
        if time_diff.num_minutes() > 5 {
            SessionStatus::Idle
        } else {
            SessionStatus::Active
        }
    }

    /// セッションとファイル情報を基にした高度なステータス判定
    /// ファイルタイムスタンプと最後のtool_use実行時刻を考慮
    pub fn determine_status_with_context(
        msg: &SessionMessage, 
        last_tool_use: Option<chrono::DateTime<chrono::Utc>>,
        file_modified_time: Option<chrono::DateTime<chrono::Utc>>
    ) -> SessionStatus {
        // エラーチェック（最優先）
        if Self::has_error(msg) {
            return SessionStatus::Error;
        }
        
        let now = Utc::now();
        
        // ファイル更新時刻を基準とした活動判定
        let activity_time = file_modified_time.unwrap_or(msg.timestamp);
        let time_since_activity = now.signed_duration_since(activity_time);
        
        // 5分以上経過していたらIdle
        if time_since_activity.num_minutes() >= 5 {
            return SessionStatus::Idle;
        }
        
        // tool_use後1秒経過チェック（最優先で判定）
        if let Some(tool_use_time) = last_tool_use {
            let time_since_tool_use = now.signed_duration_since(tool_use_time);
            
            // デバッグログ
            if std::env::var("CCMONITOR_DEBUG").is_ok() {
                eprintln!("Tool use check: {}s since tool_use", time_since_tool_use.num_seconds());
            }
            
            // tool_use実行から1秒経過していたらApprove
            if time_since_tool_use.num_seconds() >= 1 {
                if std::env::var("CCMONITOR_DEBUG").is_ok() {
                    eprintln!("Returning Approve status due to tool_use timeout");
                }
                return SessionStatus::Approve;
            } else {
                // 1秒未満の場合はActive（ツール実行中）
                if std::env::var("CCMONITOR_DEBUG").is_ok() {
                    eprintln!("Returning Active status - tool still executing");
                }
                return SessionStatus::Active;
            }
        }
        
        // メッセージタイプ別判定
        match msg.message_type.as_str() {
            "assistant" => {
                if let MessageContent::Assistant { content, .. } = &msg.message {
                    for item in content {
                        match item {
                            ContentItem::ToolUse { .. } => {
                                return SessionStatus::Active; // ツール実行中
                            },
                            ContentItem::Text { .. } => {
                                return SessionStatus::Finish; // テキスト応答 = 完了
                            }
                        }
                    }
                }
                SessionStatus::Active
            },
            "user" => SessionStatus::Active, // ユーザー入力後は作業中
            _ => SessionStatus::Active, // 基本は作業中
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use crate::session::{MessageContent, ContentItem};
    
    fn create_test_message(msg_type: &str, content_type: &str, text: Option<&str>, tool_name: Option<&str>) -> SessionMessage {
        let content_item = match content_type {
            "text" => ContentItem::Text { 
                text: text.unwrap_or("Test message").to_string() 
            },
            "tool_use" => ContentItem::ToolUse {
                id: "test_id".to_string(),
                name: tool_name.unwrap_or("test_tool").to_string(),
                input: json!({}),
            },
            _ => panic!("Invalid content type"),
        };

        let message_content = match msg_type {
            "user" => MessageContent::User {
                role: "user".to_string(),
                content: vec![content_item],
            },
            "assistant" => MessageContent::Assistant {
                role: "assistant".to_string(),
                content: vec![content_item],
            },
            _ => panic!("Invalid message type"),
        };

        SessionMessage {
            parent_uuid: None,
            user_type: "test".to_string(),
            cwd: "/test/project".to_string(),
            session_id: "test_session".to_string(),
            version: "1.0".to_string(),
            message_type: msg_type.to_string(),
            message: message_content,
            uuid: "test_uuid".to_string(),
            timestamp: Utc::now(),
            tool_use_result: None,
        }
    }

    #[test]
    fn test_user_message_status() {
        let msg = create_test_message("user", "text", Some("Hello"), None);
        let status = StatusDetector::determine_status(&msg);
        assert_eq!(status, SessionStatus::Active);
    }

    #[test]
    fn test_assistant_text_status() {
        let msg = create_test_message("assistant", "text", Some("Response"), None);
        let status = StatusDetector::determine_status(&msg);
        assert_eq!(status, SessionStatus::Finish);
    }

    #[test]
    fn test_assistant_tool_use_immediate() {
        let msg = create_test_message("assistant", "tool_use", None, Some("Read"));
        let status = StatusDetector::determine_status(&msg);
        assert_eq!(status, SessionStatus::Active);
    }

    #[test]
    fn test_assistant_tool_use_after_delay() {
        let mut msg = create_test_message("assistant", "tool_use", None, Some("Read"));
        // 2秒前のタイムスタンプに設定
        msg.timestamp = Utc::now() - chrono::Duration::seconds(2);
        
        let status = StatusDetector::determine_status(&msg);
        assert_eq!(status, SessionStatus::Approve);
    }

    #[test]
    fn test_error_detection() {
        let mut msg = create_test_message("assistant", "text", Some("Test"), None);
        msg.tool_use_result = Some("Error: File not found".to_string());
        
        let status = StatusDetector::determine_status(&msg);
        assert_eq!(status, SessionStatus::Error);
    }

    #[test]
    fn test_idle_status() {
        let mut msg = create_test_message("user", "text", Some("Old message"), None);
        // 6分前のタイムスタンプに設定
        msg.timestamp = Utc::now() - chrono::Duration::minutes(6);
        
        let status = StatusDetector::determine_status(&msg);
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_context_based_status() {
        let msg = create_test_message("assistant", "tool_use", None, Some("Read"));
        let tool_use_time = Some(Utc::now() - chrono::Duration::seconds(2));
        let file_time = Some(Utc::now());
        
        let status = StatusDetector::determine_status_with_context(&msg, tool_use_time, file_time);
        assert_eq!(status, SessionStatus::Approve);
    }
}