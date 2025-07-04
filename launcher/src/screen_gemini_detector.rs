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
    last_ui_context: Option<String>,
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
            last_ui_context: None,
            verbose,
        }
    }

    /// 画面内容から状態パターンをチェック
    fn check_screen_patterns(&self, screen_lines: &[String]) -> Option<SessionStatus> {
        for line in screen_lines {
            if let Some(state) = self.check_single_line_patterns(line) {
                return Some(state);
            }
        }
        None
    }

    /// 単一行のパターンチェック
    fn check_single_line_patterns(&self, line: &str) -> Option<SessionStatus> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 入力待ち状態（最優先）
        if line.contains("Waiting for user confirmation") {
            if self.verbose {
                eprintln!("⏳ [GEMINI_CONFIRMATION] Screen-wide confirmation detected: {trimmed}");
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

        None
    }

    /// Gemini固有の状態検出: シンプルなパターンマッチング
    fn detect_gemini_state(&mut self) -> Option<SessionStatus> {
        let screen_lines = self.screen_buffer.get_screen_lines();
        let ui_boxes = self.screen_buffer.find_ui_boxes();

        // 全ての画面内容から状態パターンをチェック
        if let Some(state) = self.check_screen_patterns(&screen_lines) {
            return Some(state);
        }

        // UI boxがある場合は、各UI boxとその上下の行をチェック
        if !ui_boxes.is_empty() {
            for ui_box in &ui_boxes {
                // UI boxの上下の行をチェック
                for line in &ui_box.above_lines {
                    if let Some(state) = self.check_single_line_patterns(line) {
                        return Some(state);
                    }
                }

                for line in &ui_box.below_lines {
                    if let Some(state) = self.check_single_line_patterns(line) {
                        return Some(state);
                    }
                }
            }

            // 特別な状態が検出されない場合はIdle
            if self.verbose {
                eprintln!("🔵 [GEMINI_IDLE] No busy or waiting patterns detected");
            }
            return Some(SessionStatus::Idle);
        }

        // UI boxがない場合も特別な状態が検出されない場合はIdle
        if self.verbose {
            eprintln!("🔵 [GEMINI_IDLE] No UI boxes, defaulting to Idle");
        }
        Some(SessionStatus::Idle)
    }

    /// 現在のバッファからUIコンテキストを直接取得（キャッシュなし）
    fn get_current_ui_context(&self) -> Option<String> {
        let screen_lines = self.screen_buffer.get_screen_lines();

        // 画面全体から行頭✦マーカーを探す（逆順で最新のものを取得）
        for line in screen_lines.iter().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with('✦') {
                let right_text = trimmed['✦'.len_utf8()..].trim();
                if !right_text.is_empty() {
                    return Some(right_text.to_string());
                }
            }
        }
        None
    }
}

impl StateDetector for ScreenGeminiStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionStatus> {
        // 基本的なスクリーンバッファ処理
        let bytes = output.as_bytes();
        self.screen_buffer.process_data(bytes);

        // 新しいUIコンテキストがある場合は更新
        let current_context = self.get_current_ui_context();
        if current_context.is_some() {
            self.last_ui_context = current_context;
        }

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
        // Gemini固有: 行頭✦の右側のテキストを取得（最新=一番下のもの）
        let screen_lines = self.screen_buffer.get_screen_lines();

        // 画面全体から行頭✦マーカーを探す（逆順で最新のものを取得）
        for line in screen_lines.iter().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with('✦') {
                let right_text = trimmed['✦'.len_utf8()..].trim();
                if !right_text.is_empty() {
                    return Some(right_text.to_string());
                }
            }
        }

        // バッファ内にコンテキストがない場合は前回の状態を保持
        self.last_ui_context.clone()
    }

    fn resize_screen_buffer(&mut self, rows: usize, cols: usize) {
        self.screen_buffer = crate::screen_buffer::ScreenBuffer::new(rows, cols, self.verbose);
    }
}
