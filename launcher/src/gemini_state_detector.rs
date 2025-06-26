// gemini_state_detector.rs - Gemini CLI固有の状態検出器

use crate::state_detector::{StateDetector, StatePatterns};
use crate::session_state::SessionState;
use crate::screen_state_detector::ScreenStateDetector;
use ccmonitor_shared::SessionStatus;

/// Gemini CLI固有の状態検出器 (Screen-based)
pub struct GeminiStateDetector {
    screen_detector: ScreenStateDetector,
}

impl GeminiStateDetector {
    pub fn new(verbose: bool) -> Self {
        let patterns = Self::create_gemini_patterns();
        Self {
            screen_detector: ScreenStateDetector::new(patterns, verbose),
        }
    }

    /// Gemini CLI固有の状態検出パターンを作成
    fn create_gemini_patterns() -> StatePatterns {
        let mut patterns = StatePatterns::new();

        // エラーパターン（Gemini CLI用）
        patterns.error_patterns.extend(vec![
            "error:".to_string(),
            "ERROR:".to_string(),
            "failed".to_string(),
            "FAILED".to_string(),
            "exception".to_string(),
            "Exception".to_string(),
            "❌".to_string(),
            "✗".to_string(),
            "Error".to_string(),
            "Failed".to_string(),
            "fatal".to_string(),
            "Fatal".to_string(),
            "abort".to_string(),
            "Abort".to_string(),
            "crashed".to_string(),
            "timeout".to_string(),
            "invalid".to_string(),
            "unauthorized".to_string(),
            "forbidden".to_string(),
            "not found".to_string(),
            "bad request".to_string(),
            "rate limit".to_string(),
            "quota exceeded".to_string(),
            "API error".to_string(),
            "connection failed".to_string(),
            "network error".to_string(),
        ]);

        // ユーザー入力待ちパターン（Gemini CLI特有）
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
            "Enter".to_string(),           // "Enter your input"
            "Press".to_string(),           // "Press any key"
            "Type".to_string(),            // "Type your message"
            "Input".to_string(),           // "Input required"
            "Waiting".to_string(),         // "Waiting for response"
            "[y/N]".to_string(),           // Default no prompt
            "[Y/n]".to_string(),           // Default yes prompt
            "Please".to_string(),          // "Please enter..."
            "Choose".to_string(),          // "Choose an option"
            "Select".to_string(),          // "Select a choice"
            "prompt>".to_string(),         // Gemini prompt marker
            "gemini>".to_string(),         // Gemini CLI prompt
            "chat>".to_string(),           // Chat mode prompt
            "waiting for input".to_string(),
            "input required".to_string(),
            "user input".to_string(),
        ]);

        // ビジー状態パターン（Gemini CLI特有）
        patterns.busy_patterns.extend(vec![
            "processing".to_string(),
            "generating".to_string(),
            "thinking".to_string(),
            "analyzing".to_string(),
            "working".to_string(),
            "executing".to_string(),
            "running".to_string(),
            "loading".to_string(),
            "sending".to_string(),
            "receiving".to_string(),
            "connecting".to_string(),
            "authenticating".to_string(),
            "requesting".to_string(),
            "fetching".to_string(),
            "parsing".to_string(),
            "interpreting".to_string(),
            "calculating".to_string(),
            "searching".to_string(),
            "querying".to_string(),
            "streaming".to_string(),
            "🤖".to_string(),              // Robot/AI emoji
            "🧠".to_string(),              // Brain emoji
            "⚡".to_string(),              // Lightning emoji
            "🔄".to_string(),              // Refresh/processing emoji
            "🔍".to_string(),              // Search emoji
            "📡".to_string(),              // Satellite/communication emoji
            "💭".to_string(),              // Thought bubble emoji
            "Processing".to_string(),      // "Processing your request"
            "Generating".to_string(),      // "Generating response"
            "Thinking".to_string(),        // "Thinking..."
            "Working".to_string(),         // "Working on it"
            "Please wait".to_string(),     // "Please wait..."
            "AI is".to_string(),           // "AI is thinking"
            "Gemini is".to_string(),       // "Gemini is processing"
            "Model is".to_string(),        // "Model is generating"
            "API call".to_string(),        // "API call in progress"
            "Streaming".to_string(),       // "Streaming response"
            "Loading".to_string(),         // "Loading model"
            "Initializing".to_string(),    // "Initializing..."
        ]);

        // アイドル状態パターン（Gemini CLI特有）
        patterns.idle_patterns.extend(vec![
            "ready".to_string(),
            "completed".to_string(),
            "finished".to_string(),
            "done".to_string(),
            "success".to_string(),
            "successful".to_string(),
            "complete".to_string(),
            "✅".to_string(),
            "✓".to_string(),
            "🎉".to_string(),
            "👍".to_string(),
            "✨".to_string(),
            "🌟".to_string(),
            "Response".to_string(),        // "Response completed"
            "Generated".to_string(),       // "Generated successfully"
            "Complete".to_string(),        // "Complete"
            "Finished".to_string(),        // "Finished generating"
            "Ready".to_string(),           // "Ready for input"
            "Available".to_string(),       // "Model available"
            "Connected".to_string(),       // "Connected to Gemini"
            "Authenticated".to_string(),   // "Authenticated successfully"
            "Session".to_string(),         // "Session established"
            "Welcome".to_string(),         // "Welcome to Gemini"
            "Hello".to_string(),           // "Hello! How can I help?"
            "How can I".to_string(),       // "How can I help you?"
            "What would".to_string(),      // "What would you like to know?"
            "Ask me".to_string(),          // "Ask me anything"
            "I'm here".to_string(),        // "I'm here to help"
            "% ".to_string(),              // Shell prompt
            "$ ".to_string(),              // Shell prompt  
            "> ".to_string(),              // Generic prompt
            "gemini> ".to_string(),        // Gemini CLI prompt
            "chat> ".to_string(),          // Chat mode prompt
            "# ".to_string(),              // Root prompt
            "→ ".to_string(),              // Arrow prompt
            "λ ".to_string(),              // Lambda prompt
        ]);

        patterns
    }

    /// Gemini固有の追加処理（将来の拡張用）
    pub fn process_gemini_specific(&mut self, output: &str) -> Option<SessionState> {
        // Gemini CLI特有の処理をここに追加
        // 例：ストリーミング応答の検出、API呼び出しステータスの識別など
        
        // 現在はスクリーンベース処理をそのまま使用
        self.screen_detector.process_output(output)
    }
}

impl StateDetector for GeminiStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        self.process_gemini_specific(output)
    }

    fn current_state(&self) -> &SessionState {
        self.screen_detector.current_state()
    }

    fn to_session_status(&self) -> SessionStatus {
        self.screen_detector.to_session_status()
    }

    fn get_patterns(&self) -> &StatePatterns {
        self.screen_detector.get_patterns()
    }

    fn debug_buffer(&self) {
        self.screen_detector.debug_buffer()
    }

    fn get_ui_execution_context(&self) -> Option<String> {
        self.screen_detector.get_ui_execution_context()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_patterns() {
        let detector = GeminiStateDetector::new(false);
        let patterns = detector.get_patterns();
        
        // Gemini固有パターンが含まれることを確認
        assert!(patterns.busy_patterns.contains(&"🤖".to_string()));
        assert!(patterns.busy_patterns.contains(&"Gemini is".to_string()));
        assert!(patterns.idle_patterns.contains(&"How can I".to_string()));
    }

    #[test]
    fn test_gemini_state_detection() {
        let mut detector = GeminiStateDetector::new(false);
        
        // Gemini固有パターンのテスト
        assert_eq!(detector.process_output("🤖 Gemini is processing your request..."), Some(SessionState::Busy));
        
        detector = GeminiStateDetector::new(false);
        assert_eq!(detector.process_output("How can I help you today?"), Some(SessionState::Idle));
        
        detector = GeminiStateDetector::new(false);
        assert_eq!(detector.process_output("Type your message:"), Some(SessionState::WaitingForInput));
    }
}