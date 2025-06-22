use crate::session::{SessionMessage, SessionStatus, MessageContent, ContentItem};
use chrono::Utc;

/// セッション状態を判定するモジュール
/// 
/// このモジュールはClaudeセッションメッセージから現在の状態を判定する
/// 新しい判定ルール:
/// - tool_useメッセージから1秒経過 → Waiting (自動承認なし)
/// - textメッセージ → Waiting (完了)
/// - userメッセージ → Active (Claude応答待ち)

pub struct StatusDetector;

impl StatusDetector {
    /// メッセージからセッション状態を判定する
    /// 
    /// # 判定ルール
    /// 1. エラー検出（最優先）
    /// 2. メッセージ内容による判定:
    ///    - tool_use: 1秒後にWaiting（自動承認待ち）、それまではActive
    ///    - text: Waiting（完了）
    ///    - user: Active（Claude応答待ち）
    /// 3. 5分以上経過: Idle
    pub fn determine_status(msg: &SessionMessage) -> SessionStatus {
        // エラーチェック（最優先）
        if Self::has_error(msg) {
            return SessionStatus::Error;
        }
        
        // メッセージタイプ別判定
        match msg.message_type.as_str() {
            "assistant" => Self::analyze_assistant_message(msg),
            "user" => SessionStatus::Active, // ユーザー入力後、Claude応答待ち
            _ => Self::fallback_status(msg),
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
                            return SessionStatus::Waiting; // 自動承認されていない
                        } else {
                            return SessionStatus::Active; // ツール実行中
                        }
                    },
                    ContentItem::Text { .. } => {
                        return SessionStatus::Waiting; // テキスト応答 = 完了
                    }
                }
            }
        }
        
        // デフォルト: 時間による判定
        Self::fallback_status(msg)
    }
    
    /// フォールバック処理（時間による判定）
    fn fallback_status(msg: &SessionMessage) -> SessionStatus {
        use chrono::Utc;
        
        let now = Utc::now();
        let time_diff = now.signed_duration_since(msg.timestamp);
        
        if time_diff.num_minutes() > 5 {
            SessionStatus::Idle
        } else {
            SessionStatus::Waiting
        }
    }
    
    /// メッセージ内容を抽出（将来の拡張用）
    #[allow(dead_code)]
    fn extract_message_content(message: &MessageContent) -> Option<String> {
        let content_items = match message {
            MessageContent::User { content, .. } => content,
            MessageContent::Assistant { content, .. } => content,
        };
        
        for item in content_items {
            if let ContentItem::Text { text } = item {
                return Some(text.clone());
            }
        }
        None
    }
    
    /// 現在のタスクを抽出（将来の拡張用）
    #[allow(dead_code)]
    fn extract_current_task(message: &MessageContent) -> Option<String> {
        let content_items = match message {
            MessageContent::Assistant { content, .. } => content,
            _ => return None,
        };
        
        for item in content_items {
            if let ContentItem::ToolUse { name, .. } = item {
                return Some(format!("Using: {}", name));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    
    #[test]
    fn test_tool_use_status() {
        // tool_use のテストケースを実装
        // 実際のメッセージ構造に合わせて追加予定
    }
    
    #[test] 
    fn test_end_turn_status() {
        // end_turn のテストケースを実装
        // 実際のメッセージ構造に合わせて追加予定
    }
    
    #[test]
    fn test_error_detection() {
        // エラー検出のテストケースを実装
        // 実際のメッセージ構造に合わせて追加予定
    }
}