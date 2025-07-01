// screen_claude_detector.rs - Screen buffer based Claude state detector

use crate::cli_tool::CliToolType;
use crate::screen_state_detector::ScreenStateDetector;
use crate::session_state::SessionState;
use crate::state_detector::StateDetector;
use ccmonitor_shared::SessionStatus;
use std::time::Instant;

/// スクリーンバッファベースのClaude状態検出器
pub struct ScreenClaudeStateDetector {
    screen_detector: ScreenStateDetector,
    previous_had_esc_interrupt: bool,
    last_state_change: Option<Instant>,
    verbose: bool,
}

impl ScreenClaudeStateDetector {
    pub fn new(verbose: bool) -> Self {
        let screen_detector = ScreenStateDetector::new(CliToolType::Claude, verbose);

        Self {
            screen_detector,
            previous_had_esc_interrupt: false,
            last_state_change: None,
            verbose,
        }
    }

    /// Claude固有の完了状態検出: "esc to interrupt"の有無で判定
    fn detect_claude_completion_state(&mut self) -> Option<SessionState> {
        // 現在の画面に"esc to interrupt"があるかチェック
        let screen_lines = self.screen_detector.get_screen_lines();
        let has_esc_interrupt = screen_lines
            .iter()
            .any(|line| line.contains("esc to interrupt"));

        let now = Instant::now();

        if self.verbose {
            eprintln!(
                "🔍 [CLAUDE_STATE] esc_interrupt: {} → {}, current: {}",
                self.previous_had_esc_interrupt,
                has_esc_interrupt,
                self.screen_detector.current_state()
            );
        }

        // 状態変化の検出
        if self.previous_had_esc_interrupt && !has_esc_interrupt {
            // "esc to interrupt"が消えた = 実行完了
            if self.verbose {
                eprintln!("✅ [CLAUDE_COMPLETION] 'esc to interrupt' disappeared → Completing");
            }
            self.last_state_change = Some(now);
            self.previous_had_esc_interrupt = false;
            return Some(SessionState::Idle);
        } else if !self.previous_had_esc_interrupt && has_esc_interrupt {
            // "esc to interrupt"が現れた = 実行開始
            if self.verbose {
                eprintln!("🚀 [CLAUDE_START] 'esc to interrupt' appeared → Busy");
            }
            self.last_state_change = Some(now);
            self.previous_had_esc_interrupt = true;
            return Some(SessionState::Busy);
        }

        // 状態変化なし、基底クラスの判定を使用
        self.previous_had_esc_interrupt = has_esc_interrupt;
        None
    }
}

impl StateDetector for ScreenClaudeStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        // 基底クラスで画面バッファを更新
        let _base_state = self.screen_detector.process_output(output);

        // Claude固有の"esc to interrupt"ロジックを適用
        self.detect_claude_completion_state()
    }

    fn current_state(&self) -> &SessionState {
        self.screen_detector.current_state()
    }

    fn to_session_status(&self) -> SessionStatus {
        self.screen_detector.to_session_status()
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
