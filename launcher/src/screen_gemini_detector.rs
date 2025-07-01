// screen_gemini_detector.rs - Screen buffer based Gemini state detector

use crate::screen_buffer::ScreenBuffer;
use crate::session_state::SessionState;
use crate::state_detector::StateDetector;
use climonitor_shared::SessionStatus;
use std::time::Instant;

/// スクリーンバッファベースのGemini状態検出器
pub struct ScreenGeminiStateDetector {
    screen_buffer: ScreenBuffer,
    current_state: SessionState,
    last_state_change: Option<Instant>,
    verbose: bool,
}

impl ScreenGeminiStateDetector {
    pub fn new(verbose: bool) -> Self {
        // 実際のターミナルサイズを取得
        let pty_size = crate::cli_tool::get_pty_size();
        let screen_buffer =
            ScreenBuffer::new(pty_size.rows as usize, pty_size.cols as usize, verbose);

        if verbose {
            eprintln!(
                "🖥️  [GEMINI_INIT] Initialized screen buffer with {}x{} (rows x cols)",
                pty_size.rows, pty_size.cols
            );
        }

        Self {
            screen_buffer,
            current_state: SessionState::Connected,
            last_state_change: None,
            verbose,
        }
    }

    /// Gemini固有の状態検出: スピナーとUI boxの組み合わせで判定
    fn detect_gemini_state(&mut self) -> Option<SessionState> {
        let screen_lines = self.screen_buffer.get_screen_lines();
        let ui_boxes = self.screen_buffer.find_ui_boxes();

        // UI boxがある場合は通常の検出ロジック（入力待ち状態など）
        if !ui_boxes.is_empty() {
            if let Some(latest_box) = ui_boxes.last() {
                // UI box内容での状態検出
                for content_line in &latest_box.content_lines {
                    let trimmed = content_line.trim();

                    // > から始まる行は完了状態（コマンド入力待ち）
                    if trimmed.starts_with('>') {
                        if self.verbose {
                            eprintln!("✅ [GEMINI_READY] Command prompt ready: {trimmed}");
                        }
                        return Some(SessionState::Idle);
                    }

                    // 入力待ち状態の検出
                    if content_line.contains("Allow execution?") {
                        if self.verbose {
                            eprintln!("⏳ [GEMINI_INPUT] Waiting for input: {trimmed}");
                        }
                        return Some(SessionState::WaitingForInput);
                    }
                }

                // UI box下の行での状態検出
                for below_line in &latest_box.below_lines {
                    if below_line.contains("◯ IDE connected") {
                        if self.verbose {
                            eprintln!("💻 [GEMINI_IDE] IDE connected detected");
                        }
                        return Some(SessionState::Idle);
                    }

                    // Gemini確認待ち状態の検出
                    if below_line.contains("Waiting for user confirmation") {
                        if self.verbose {
                            eprintln!(
                                "⏳ [GEMINI_CONFIRMATION] Waiting for user confirmation: {}",
                                below_line.trim()
                            );
                        }
                        return Some(SessionState::WaitingForInput);
                    }
                }

                // UI boxがあるがアクティブな操作が検出されない場合はIdle
                if self.verbose {
                    eprintln!("🔵 [GEMINI_IDLE] UI box present but no active operations");
                }
                return Some(SessionState::Idle);
            }
        }

        // UI boxがない場合：Gemini特有のスピナーパターンを検出
        for line in &screen_lines {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                // Gemini処理中パターンの検出
                if trimmed.contains("(esc to cancel") {
                    if self.verbose {
                        eprintln!("⚡ [GEMINI_BUSY] Processing detected: {trimmed}");
                    }
                    return Some(SessionState::Busy);
                }

                // エラーパターンの検出
                if trimmed.contains("✗") || trimmed.contains("failed") || trimmed.contains("Error")
                {
                    if self.verbose {
                        eprintln!("🔴 [GEMINI_ERROR] Error detected: {trimmed}");
                    }
                    return Some(SessionState::Error);
                }
            }
        }

        // 統計情報ボックスが表示されている場合はIdle（セッション終了後）
        for line in &screen_lines {
            if line.contains("Cumulative Stats") || line.contains("Input Tokens") {
                if self.verbose {
                    eprintln!("📊 [GEMINI_STATS] Stats displayed, session idle");
                }
                return Some(SessionState::Idle);
            }
        }

        // 何も検出されない場合は現在の状態を維持
        None
    }
}

impl StateDetector for ScreenGeminiStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        // 基本的なスクリーンバッファ処理
        let bytes = output.as_bytes();
        self.screen_buffer.process_data(bytes);

        // Gemini特有の検出ロジックを適用
        if let Some(gemini_state) = self.detect_gemini_state() {
            let now = Instant::now();

            // 状態変化の記録
            if gemini_state != self.current_state {
                self.last_state_change = Some(now);

                if self.verbose {
                    eprintln!(
                        "🎯 [GEMINI_STATE_CHANGE] {:?} → {:?}",
                        self.current_state, gemini_state
                    );
                }
            }

            // 状態を更新
            self.current_state = gemini_state.clone();
            return Some(gemini_state);
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
        let lines = self.screen_buffer.get_screen_lines();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_end();
            if !trimmed.is_empty() {
                eprintln!("  {i:2}: {trimmed}");
            }
        }
    }

    fn get_ui_execution_context(&self) -> Option<String> {
        let screen_lines = self.screen_buffer.get_screen_lines();
        for line in &screen_lines {
            if line.contains("(esc to cancel") {
                return Some("処理中".to_string());
            }
        }
        None
    }

    fn get_ui_above_text(&self) -> Option<String> {
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
