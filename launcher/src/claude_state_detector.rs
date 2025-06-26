// claude_state_detector.rs - Claude Code固有の状態検出器

use crate::state_detector::{StateDetector, StatePatterns, BaseStateDetector};
use crate::session_state::SessionState;
use ccmonitor_shared::SessionStatus;

/// Claude Code固有の状態検出器
pub struct ClaudeStateDetector {
    base: BaseStateDetector,
}

impl ClaudeStateDetector {
    pub fn new(verbose: bool) -> Self {
        let patterns = Self::create_claude_patterns();
        Self {
            base: BaseStateDetector::new(patterns, verbose),
        }
    }

    /// Claude Code固有の状態検出パターンを作成
    fn create_claude_patterns() -> StatePatterns {
        let mut patterns = StatePatterns::new();

        // エラーパターン
        patterns.error_patterns.extend(vec![
            "error:".to_string(),
            "failed:".to_string(),
            "exception".to_string(),
            "❌".to_string(),
            "✗".to_string(),
            "Error:".to_string(),
            "Failed:".to_string(),
            "Exception".to_string(),
            "FAILED".to_string(),
            "ERROR".to_string(),
            "fatal".to_string(),
            "Fatal".to_string(),
            "abort".to_string(),
            "Abort".to_string(),
            "crashed".to_string(),
            "timeout".to_string(),
            "rejected".to_string(),
            "permission denied".to_string(),
            "not found".to_string(),
            "invalid".to_string(),
            "corrupt".to_string(),
        ]);

        // ユーザー入力待ちパターン（Claude Code特有）
        patterns.waiting_patterns.extend(vec![
            "proceed?".to_string(),
            "continue?".to_string(),
            "confirm".to_string(),
            "y/n".to_string(),
            "Y/n".to_string(),
            "press".to_string(),
            "wait".to_string(),
            "⏳".to_string(),
            "🤔".to_string(),
            "May I".to_string(),           // "May I use the X tool?"
            "Should I".to_string(),       // "Should I proceed?"
            "Would you like".to_string(), // "Would you like me to..."
            "permission".to_string(),     // Tool permission requests
            "approve".to_string(),        // Tool approval requests
            "authorize".to_string(),      // Authorization requests
            "[y/N]".to_string(),          // Default no prompt
            "[Y/n]".to_string(),          // Default yes prompt
            "Enter".to_string(),          // "Enter to continue"
            "Press".to_string(),          // "Press any key"
            "waiting".to_string(),        // "waiting for input"
            "input required".to_string(), // "input required"
        ]);

        // ビジー状態パターン（Claude Code特有）
        patterns.busy_patterns.extend(vec![
            "processing".to_string(),
            "executing".to_string(),
            "running".to_string(),
            "analyzing".to_string(),
            "thinking".to_string(),
            "working".to_string(),
            "applying".to_string(),
            "trying".to_string(),
            "retrying".to_string(),
            "generating".to_string(),
            "creating".to_string(),
            "building".to_string(),
            "compiling".to_string(),
            "Installing".to_string(),
            "Downloading".to_string(),
            "Uploading".to_string(),
            "Searching".to_string(),
            "Loading".to_string(),
            "Parsing".to_string(),
            "Validating".to_string(),
            "分析中".to_string(),
            "処理中".to_string(),
            "実行中".to_string(),
            "🔧".to_string(),
            "⚙️".to_string(),
            "📝".to_string(),
            "📊".to_string(),
            "🔍".to_string(),
            "🚀".to_string(),
            "⚡".to_string(),
            "🔄".to_string(),
            "🛠️".to_string(),
            "claude code:".to_string(),     // Claude Code のプロンプト
            "I'll".to_string(),            // "I'll help you..."
            "Let me".to_string(),          // "Let me analyze..."
            "I'm".to_string(),             // "I'm working on..."
            "Working on".to_string(),      // "Working on your request"
            "Tool:".to_string(),           // Claude tool execution
            "Using".to_string(),           // "Using the X tool"
            "Executing".to_string(),       // "Executing tool X"
            "Calling".to_string(),         // "Calling API..."
            "Requesting".to_string(),      // "Requesting permission"
            "Sending".to_string(),         // "Sending request"
            "Fetching".to_string(),        // "Fetching data"
        ]);

        // アイドル状態パターン（Claude Code特有）
        patterns.idle_patterns.extend(vec![
            "ready".to_string(),
            "completed".to_string(),
            "finished".to_string(),
            "done".to_string(),
            "success".to_string(),
            "complete".to_string(),
            "successful".to_string(),
            "完了".to_string(),
            "成功".to_string(),
            "正常".to_string(),
            "✅".to_string(),
            "✓".to_string(),
            "🌟".to_string(),
            "✨".to_string(),
            "🎉".to_string(),
            "👍".to_string(),
            "Great!".to_string(),
            "Perfect!".to_string(),
            "Excellent!".to_string(),
            "All set".to_string(),
            "Task completed".to_string(),
            "Request completed".to_string(),
            "Successfully".to_string(),     // "Successfully created..."
            "Created".to_string(),          // "Created file X"
            "Updated".to_string(),          // "Updated file X"
            "Saved".to_string(),            // "Saved changes"
            "Built".to_string(),            // "Built successfully"
            "Compiled".to_string(),         // "Compiled successfully"
            "Test passed".to_string(),      // "Test passed"
            "All tests".to_string(),        // "All tests passed"
            "No errors".to_string(),        // "No errors found"
            "% ".to_string(),               // シェルプロンプト
            "$ ".to_string(),               // シェルプロンプト
            "> ".to_string(),               // その他のプロンプト
            "claude>".to_string(),          // Claude プロンプト
            "# ".to_string(),               // ルートプロンプト
            "→ ".to_string(),               // カスタムプロンプト
            "λ ".to_string(),               // Lambda プロンプト
        ]);

        patterns
    }

    /// Claude固有の追加処理（将来の拡張用）
    pub fn process_claude_specific(&mut self, output: &str) -> Option<SessionState> {
        // Claude Code特有の処理をここに追加
        // 例：ツール実行シーケンスの検出、プロンプト/応答サイクルの識別など
        
        // 現在は基本処理をそのまま使用
        self.base.process_output(output)
    }
}

impl StateDetector for ClaudeStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        self.process_claude_specific(output)
    }

    fn current_state(&self) -> &SessionState {
        self.base.current_state()
    }

    fn to_session_status(&self) -> SessionStatus {
        self.base.to_session_status()
    }

    fn get_patterns(&self) -> &StatePatterns {
        self.base.get_patterns()
    }

    fn debug_buffer(&self) {
        self.base.debug_buffer()
    }

    fn get_ui_execution_context(&self) -> Option<String> {
        self.base.get_ui_execution_context()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_patterns() {
        let detector = ClaudeStateDetector::new(false);
        let patterns = detector.get_patterns();
        
        // Claude固有パターンが含まれることを確認
        assert!(patterns.waiting_patterns.contains(&"May I".to_string()));
        assert!(patterns.busy_patterns.contains(&"Tool:".to_string()));
        assert!(patterns.idle_patterns.contains(&"Successfully".to_string()));
    }

    #[test]
    fn test_claude_state_detection() {
        let mut detector = ClaudeStateDetector::new(false);
        
        // Claude固有パターンのテスト
        assert_eq!(detector.process_output("May I use the Edit tool?"), Some(SessionState::WaitingForInput));
        
        detector = ClaudeStateDetector::new(false);
        assert_eq!(detector.process_output("🔧 Tool: Reading file..."), Some(SessionState::Busy));
        
        detector = ClaudeStateDetector::new(false);
        assert_eq!(detector.process_output("✅ Successfully created the file"), Some(SessionState::Idle));
    }
}