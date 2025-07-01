// screen_claude_detector.rs - Screen buffer based Claude state detector

use crate::screen_state_detector::ScreenStateDetector;
use crate::session_state::SessionState;
use crate::state_detector::{StateDetector, StatePatterns};
use ccmonitor_shared::SessionStatus;

/// スクリーンバッファベースのClaude状態検出器
pub struct ScreenClaudeStateDetector {
    screen_detector: ScreenStateDetector,
}

impl ScreenClaudeStateDetector {
    pub fn new(verbose: bool) -> Self {
        // Claude固有のパターンを設定
        let patterns = StatePatterns {
            error_patterns: vec![
                "Error:".to_string(),
                "failed".to_string(),
                "API Error".to_string(),
                "Connection failed".to_string(),
                "✗".to_string(),
            ],
            waiting_patterns: vec![
                "Do you want".to_string(),
                "Would you like".to_string(),
                "May I".to_string(),
                "proceed?".to_string(),
                "y/n".to_string(),
                "Continue?".to_string(),
            ],
            busy_patterns: vec![
                "esc to interrupt".to_string(),
                "Musing".to_string(),
                "Auto-updating".to_string(),
                "Tool:".to_string(),
                "Wizarding".to_string(),
                "Baking".to_string(),
                "⚒".to_string(),
                "✳".to_string(),
                "✢".to_string(),
            ],
            idle_patterns: vec![
                "◯ IDE connected".to_string(),
                "Successfully".to_string(),
                "completed".to_string(),
                "finished".to_string(),
                "✅".to_string(),
            ],
        };

        let screen_detector = ScreenStateDetector::new(patterns, verbose);

        Self { screen_detector }
    }
}

impl StateDetector for ScreenClaudeStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        self.screen_detector.process_output(output)
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

    fn get_ui_above_text(&self) -> Option<String> {
        self.screen_detector.get_ui_above_text()
    }
}
