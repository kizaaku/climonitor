// screen_gemini_detector.rs - Screen buffer based Gemini state detector

use crate::screen_buffer::ScreenBuffer;
use crate::state_detector::StateDetector;
use climonitor_shared::SessionStatus;
use std::time::Instant;

/// スクリーンバッファベースのGemini状態検出器
pub struct ScreenGeminiStateDetector {
    screen_buffer: ScreenBuffer,
    current_state: SessionStatus,
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
            current_state: SessionStatus::Connected,
            last_state_change: None,
            verbose,
        }
    }

    /// 画面内容から状態パターンをチェック
    fn check_screen_patterns(&self, screen_lines: &[String]) -> Option<SessionStatus> {
        for line in screen_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 入力待ち状態（最優先）
            if line.contains("Waiting for user confirmation") {
                if self.verbose {
                    eprintln!(
                        "⏳ [GEMINI_CONFIRMATION] Screen-wide confirmation detected: {trimmed}"
                    );
                }
                return Some(SessionStatus::WaitingInput);
            }

            // 実行中状態
            if line.contains("(esc to cancel") {
                if self.verbose {
                    eprintln!("⚡ [GEMINI_BUSY] Processing detected: {trimmed}");
                }
                return Some(SessionStatus::Busy);
            }
        }
        None
    }

    /// Gemini固有の状態検出: スピナーとUI boxの組み合わせで判定
    fn detect_gemini_state(&mut self) -> Option<SessionStatus> {
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
                        return Some(SessionStatus::Idle);
                    }
                }

                // 全てのscreen_linesから状態パターンをチェック
                if let Some(state) = self.check_screen_patterns(&screen_lines) {
                    return Some(state);
                }

                // UI boxがあるがアクティブな操作が検出されない場合はIdle
                if self.verbose {
                    eprintln!("🔵 [GEMINI_IDLE] UI box present but no active operations");
                }
                return Some(SessionStatus::Idle);
            }
        }

        // UI boxがない場合も同じパターンチェックを使用
        if let Some(state) = self.check_screen_patterns(&screen_lines) {
            return Some(state);
        }

        // デバッグ: 検知されない場合の画面内容を確認
        if self.verbose {
            eprintln!("🤔 [GEMINI_DEBUG] No state detected. Screen content:");
            for (i, line) in screen_lines.iter().enumerate() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    eprintln!("  {i:2}: '{trimmed}'");
                }
            }
        }

        // 何も検出されない場合は現在の状態を維持
        None
    }
}

impl StateDetector for ScreenGeminiStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionStatus> {
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

    fn current_state(&self) -> &SessionStatus {
        &self.current_state
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
