// screen_claude_detector.rs - Screen buffer based Claude state detector

use crate::screen_buffer::ScreenBuffer;
use crate::session_state::SessionState;
use crate::state_detector::StateDetector;
use climonitor_shared::SessionStatus;
use std::time::Instant;

/// スクリーンバッファベースのClaude状態検出器
pub struct ScreenClaudeStateDetector {
    screen_buffer: ScreenBuffer,
    current_state: SessionState,
    previous_had_esc_interrupt: bool,
    last_state_change: Option<Instant>,
    verbose: bool,
}

impl ScreenClaudeStateDetector {
    pub fn new(verbose: bool) -> Self {
        // 実際のターミナルサイズを取得
        let pty_size = crate::cli_tool::get_pty_size();
        let screen_buffer =
            ScreenBuffer::new(pty_size.rows as usize, pty_size.cols as usize, verbose);

        if verbose {
            eprintln!(
                "🖥️  [CLAUDE_INIT] Initialized screen buffer with {}x{} (rows x cols)",
                pty_size.rows, pty_size.cols
            );
        }

        Self {
            screen_buffer,
            current_state: SessionState::Connected,
            previous_had_esc_interrupt: false,
            last_state_change: None,
            verbose,
        }
    }

    /// Claude固有の完了状態検出: "esc to interrupt"の有無で判定
    fn detect_claude_completion_state(&mut self) -> Option<SessionState> {
        // 現在の画面に"esc to interrupt"があるかチェック
        let screen_lines = self.screen_buffer.get_screen_lines();
        let has_esc_interrupt = screen_lines
            .iter()
            .any(|line| line.contains("esc to interrupt"));

        let now = Instant::now();

        if self.verbose {
            eprintln!(
                "🔍 [CLAUDE_STATE] esc_interrupt: {} → {}, current: {}",
                self.previous_had_esc_interrupt, has_esc_interrupt, self.current_state
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

        // 状態変化なし、基本的なUI box検出を実行
        self.previous_had_esc_interrupt = has_esc_interrupt;

        // UI boxからの基本的な状態検出
        let ui_boxes = self.screen_buffer.find_ui_boxes();
        if let Some(latest_box) = ui_boxes.last() {
            // 承認プロンプト検出
            for content_line in &latest_box.content_lines {
                if content_line.contains("Do you want")
                    || content_line.contains("Would you like")
                    || content_line.contains("May I")
                    || content_line.contains("proceed?")
                    || content_line.contains("y/n")
                {
                    return Some(SessionState::WaitingForInput);
                }
            }

            // IDE接続確認
            for below_line in &latest_box.below_lines {
                if below_line.contains("◯ IDE connected") {
                    return Some(SessionState::Idle);
                }
            }
        }

        None
    }
}

impl StateDetector for ScreenClaudeStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        // 画面バッファを更新
        let bytes = output.as_bytes();
        self.screen_buffer.process_data(bytes);

        // Claude固有の"esc to interrupt"ロジックを適用
        if let Some(new_state) = self.detect_claude_completion_state() {
            self.current_state = new_state.clone();
            return Some(new_state);
        }

        None
    }

    fn current_state(&self) -> &SessionState {
        &self.current_state
    }

    fn to_session_status(&self) -> SessionStatus {
        self.current_state.to_session_status()
    }

    fn debug_buffer(&self) {
        // デバッグ用に画面内容を表示
        let lines = self.screen_buffer.get_screen_lines();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_end();
            if !trimmed.is_empty() {
                eprintln!("  {i:2}: {trimmed}");
            }
        }
    }


    fn get_ui_above_text(&self) -> Option<String> {
        // UI box上のテキストを取得（基本実装）
        let ui_boxes = self.screen_buffer.find_ui_boxes();
        if let Some(latest_box) = ui_boxes.last() {
            for line in &latest_box.above_lines {
                if line.contains("⏺") {
                    return Some(line.trim().to_string());
                }
            }
        }
        None
    }

    fn resize_screen_buffer(&mut self, rows: usize, cols: usize) {
        self.screen_buffer = crate::screen_buffer::ScreenBuffer::new(rows, cols, self.verbose);
    }
}
