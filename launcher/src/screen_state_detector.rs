// screen_state_detector.rs - Screen buffer based state detection

use crate::screen_buffer::{ScreenBuffer, UIBox};
use crate::session_state::SessionState;
use crate::state_detector::{StateDetector, StatePatterns};
use ccmonitor_shared::SessionStatus;
use std::io::Write;
use std::time::{Duration, Instant};

/// RAWモード対応のデバッグ出力（改行を正しく処理）
fn debug_println_raw(msg: &str) {
    let mut stderr = std::io::stderr();
    let _ = write!(stderr, "\r\n{}\r\n", msg);
    let _ = stderr.flush();
}

/// スクリーンバッファベースの状態検出器
pub struct ScreenStateDetector {
    screen_buffer: ScreenBuffer,
    current_state: SessionState,
    ui_execution_context: Option<String>,
    ui_above_text: Option<String>,
    patterns: StatePatterns,
    verbose: bool,
    last_busy_time: Option<Instant>,
}

impl ScreenStateDetector {
    pub fn new(patterns: StatePatterns, verbose: bool) -> Self {
        // 実際のターミナルサイズを取得
        let pty_size = crate::cli_tool::get_pty_size();
        let screen_buffer =
            ScreenBuffer::new(pty_size.rows as usize, pty_size.cols as usize, verbose);

        if verbose {
            debug_println_raw(&format!(
                "🖥️  [SCREEN_INIT] Initialized screen buffer with {}x{} (rows x cols)",
                pty_size.rows, pty_size.cols
            ));
        }

        Self {
            screen_buffer,
            current_state: SessionState::Connected,
            ui_execution_context: None,
            ui_above_text: None,
            patterns,
            verbose,
            last_busy_time: None,
        }
    }

    /// スクリーンから状態を検出
    fn detect_state_from_screen(&mut self) -> SessionState {
        let ui_boxes = self.screen_buffer.find_ui_boxes();

        if self.verbose {
            debug_println_raw(&format!(
                "🖥️  [SCREEN_ANALYSIS] Found {} UI boxes",
                ui_boxes.len()
            ));

            // スクリーン内容をデバッグ出力
            let lines = self.screen_buffer.get_screen_lines();
            debug_println_raw("📺 [CURRENT_SCREEN] Screen content:");
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    debug_println_raw(&format!("  {:2}: {}", i, trimmed));
                }
            }
        }

        // 最新のUI box（画面下部にある）を使用
        if let Some(latest_box) = ui_boxes.last() {
            if self.verbose {
                debug_println_raw(&format!(
                    "📦 [LATEST_UI_BOX] Analyzing box at rows {}-{}",
                    latest_box.start_row, latest_box.end_row
                ));
            }

            // UI box上の行から実行コンテキストを検出
            self.analyze_execution_context(&latest_box.above_lines);

            // UI boxの内容から状態を判定
            if let Some(state) = self.analyze_ui_box_content(latest_box) {
                // Busyからの遷移に100ms遅延を適用
                if self.current_state == SessionState::Busy && state != SessionState::Busy {
                    let now = Instant::now();
                    if let Some(busy_time) = self.last_busy_time {
                        if now.duration_since(busy_time) < Duration::from_millis(100) {
                            if self.verbose {
                                debug_println_raw(&format!(
                                    "⏱️  [DELAY_TRANSITION] Delaying {} → {} ({}ms elapsed)",
                                    self.current_state,
                                    state,
                                    now.duration_since(busy_time).as_millis()
                                ));
                            }
                            return self.current_state.clone();
                        }
                    }
                }

                // Busyに遷移する際の時刻を記録
                if state == SessionState::Busy && self.current_state != SessionState::Busy {
                    self.last_busy_time = Some(Instant::now());
                }

                if self.verbose && state != self.current_state {
                    debug_println_raw(&format!(
                        "🎯 [STATE_DETECTED] {} → {}",
                        self.current_state, state
                    ));
                }
                return state;
            }
        }

        // UI boxが見つからない場合は現在の状態を維持し、ui_above_textをクリア
        if self.verbose {
            debug_println_raw(&format!(
                "🔍 [NO_UI_BOX] No UI box found, maintaining state: {:?}",
                self.current_state
            ));
            if self.ui_above_text.is_some() {
                debug_println_raw(
                    "🗑️  [CLEAR_UI_TEXT] Clearing ui_above_text (no UI box in current screen)",
                );
            }
        }
        self.ui_above_text = None;
        self.current_state.clone()
    }

    /// 実行コンテキストを分析してui_execution_contextを更新
    fn analyze_execution_context(&mut self, above_lines: &[String]) {
        self.ui_execution_context = None;
        // ui_above_textは一旦保存してから処理
        let mut new_ui_above_text = None;

        // UI BOXに最も近い⏺文字から始まる行を探す（逆順でスキャン）
        for line in above_lines.iter().rev() {
            // 実行コンテキスト検出（順序は関係ないので先に処理）
            if line.contains("esc to interrupt")
                || line.contains("Musing")
                || line.contains("Auto-updating")
                || line.contains("Tool:")
                || line.contains("Wizarding")
                || line.contains("Baking")
            {
                let context = Self::extract_short_context(line);
                self.ui_execution_context = Some(context.clone());

                if self.verbose {
                    debug_println_raw(&format!(
                        "⚡ [EXECUTION_CONTEXT] Found: {} → {}",
                        line.trim(),
                        context
                    ));
                }
            }

            // ⏺文字以降のテキスト抽出（UI BOXに最も近い行を優先）
            if new_ui_above_text.is_none() {
                if let Some(text_after_circle) = Self::extract_text_after_circle(line) {
                    new_ui_above_text = Some(text_after_circle.clone());

                    if self.verbose {
                        debug_println_raw(&format!(
                            "⏺ [UI_ABOVE_TEXT] Found closest: {}",
                            text_after_circle
                        ));
                    }
                }
            }
        }

        // 現在の画面内容に基づいてui_above_textを更新
        if let Some(new_text) = new_ui_above_text {
            self.ui_above_text = Some(new_text);
        } else {
            // 現在のUI BOX上に⏺文字がない場合はクリア
            if self.verbose && self.ui_above_text.is_some() {
                debug_println_raw(
                    "🗑️  [CLEAR_UI_TEXT] No ⏺ text in current screen, clearing ui_above_text",
                );
            }
            self.ui_above_text = None;
        }
    }

    /// ⏺文字以降のテキストを抽出（色違いの⏺にも対応、1行のみ）
    fn extract_text_after_circle(line: &str) -> Option<String> {
        // ⏺文字（Unicode: U+23FA）のバリエーションを検索
        // ANSIエスケープシーケンスで色が変わっても文字自体は同じ
        if let Some(pos) = line.find('⏺') {
            let after_circle = &line[pos + '⏺'.len_utf8()..];
            let trimmed = after_circle.trim();

            // ANSIエスケープシーケンスを除去して実際のテキストのみを取得
            let clean_text = Self::strip_ansi_sequences(trimmed);

            // 改行文字または文の終端で分割し、最初のセンテンスのみを取得
            let first_sentence = clean_text
                .split(&['\n', '\r'][..])
                .next()
                .unwrap_or("")
                .trim();

            // さらに長すぎる場合は句読点で区切る
            let result = if first_sentence.len() > 100 {
                first_sentence
                    .split(&['。', '.', '!', '?'][..])
                    .next()
                    .unwrap_or(first_sentence)
                    .trim()
            } else {
                first_sentence
            };

            if !result.is_empty() {
                return Some(result.to_string());
            }
        }
        None
    }

    /// ANSIエスケープシーケンスを除去
    fn strip_ansi_sequences(text: &str) -> String {
        // 簡易的なANSI除去（CSI sequenceとOSC sequenceを対象）
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // ESC文字が来たらエスケープシーケンスをスキップ
                if chars.peek() == Some(&'[') {
                    chars.next(); // '['をスキップ
                                  // CSI sequence: 数字、セミコロン、スペースなどをスキップしてアルファベットまで
                    while let Some(&next_ch) = chars.peek() {
                        chars.next();
                        if next_ch.is_ascii_alphabetic() {
                            break;
                        }
                    }
                } else if chars.peek() == Some(&']') {
                    chars.next(); // ']'をスキップ
                                  // OSC sequence: BELまたはST (ESC \) まで
                    while let Some(next_ch) = chars.next() {
                        if next_ch == '\x07' {
                            // BEL
                            break;
                        }
                        if next_ch == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next(); // '\'をスキップ
                            break;
                        }
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// UI boxの内容から状態を判定
    fn analyze_ui_box_content(&self, ui_box: &UIBox) -> Option<SessionState> {
        if self.verbose {
            debug_println_raw(&format!(
                "🔍 [ANALYZING_UI_BOX] {} content lines",
                ui_box.content_lines.len()
            ));
            for (i, line) in ui_box.content_lines.iter().enumerate() {
                debug_println_raw(&format!("  Content[{}]: '{}'", i, line));
            }
        }

        // 1. UI box内容での承認プロンプト検出（最優先）
        for content_line in &ui_box.content_lines {
            if content_line.contains("Do you want")
                || content_line.contains("Would you like")
                || content_line.contains("May I")
                || content_line.contains("proceed?")
                || content_line.contains("y/n")
            {
                if self.verbose {
                    debug_println_raw(&format!("⏳ [APPROVAL_DETECTED] {}", content_line));
                }
                return Some(SessionState::WaitingForInput);
            }
        }

        // 2. 上の行（実行コンテキスト）での実行状態検出
        for above_line in &ui_box.above_lines {
            if above_line.contains("esc to interrupt")
                || above_line.contains("Musing")
                || above_line.contains("Auto-updating")
                || above_line.contains("Tool:")
                || above_line.contains("Wizarding")
                || above_line.contains("Baking")
            {
                if self.verbose {
                    debug_println_raw(&format!("⚡ [EXECUTION_ACTIVE] {}", above_line.trim()));
                }
                return Some(SessionState::Busy);
            }
        }

        // 3. 下の行（ステータス）でのエラー検出
        for below_line in &ui_box.below_lines {
            if below_line.contains("✗")
                || below_line.contains("failed")
                || below_line.contains("Error")
            {
                if self.verbose {
                    debug_println_raw(&format!("🔴 [ERROR_DETECTED] {}", below_line.trim()));
                }
                return Some(SessionState::Error);
            }

            if below_line.contains("◯ IDE connected") {
                if self.verbose {
                    debug_println_raw(&format!("💻 [IDE_CONNECTED] {}", below_line.trim()));
                }
                return Some(SessionState::Idle);
            }
        }

        // 4. UI boxが存在するがアクティブな操作が検出されない場合はIdle
        if self.verbose {
            debug_println_raw("🔵 [UI_BOX_IDLE] UI box present but no active operations detected");
        }
        Some(SessionState::Idle)
    }

    /// 実行コンテキストから短縮表示を抽出
    fn extract_short_context(full_context: &str) -> String {
        if full_context.contains("esc to interrupt") {
            "実行中".to_string()
        } else if full_context.contains("Musing") {
            "思考中".to_string()
        } else if full_context.contains("Auto-updating") {
            "更新中".to_string()
        } else if full_context.contains("Tool:") {
            "ツール".to_string()
        } else if full_context.contains("Wizarding") {
            "処理中".to_string()
        } else if full_context.contains("Baking") {
            "構築中".to_string()
        } else {
            // 最初の6文字を表示
            full_context.chars().take(6).collect()
        }
    }
}

impl StateDetector for ScreenStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        // UTF-8出力をバイト配列に変換してVTE parserに送信
        let bytes = output.as_bytes();
        self.screen_buffer.process_data(bytes);

        if self.verbose && !output.trim().is_empty() {
            debug_println_raw("🖥️  [SCREEN_UPDATE] Processing screen update");
        }

        // スクリーンバッファから状態を検出
        let new_state = self.detect_state_from_screen();

        // 状態が変化した場合のみ通知
        if new_state != self.current_state {
            let old_state = self.current_state.clone();
            self.current_state = new_state.clone();

            if self.verbose {
                debug_println_raw(&format!("🎯 [STATE_CHANGE] {} → {}", old_state, new_state));
            }

            Some(new_state)
        } else {
            None
        }
    }

    fn current_state(&self) -> &SessionState {
        &self.current_state
    }

    fn to_session_status(&self) -> SessionStatus {
        match &self.current_state {
            SessionState::Idle => SessionStatus::Idle,
            SessionState::Busy => SessionStatus::Busy,
            SessionState::WaitingForInput => SessionStatus::WaitingInput,
            SessionState::Error => SessionStatus::Error,
            SessionState::Connected => SessionStatus::Idle,
        }
    }

    fn get_patterns(&self) -> &StatePatterns {
        &self.patterns
    }

    fn debug_buffer(&self) {
        if self.verbose {
            debug_println_raw("🖥️  [SCREEN_BUFFER] Current screen content:");
            let lines = self.screen_buffer.get_screen_lines();
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    debug_println_raw(&format!("  {:2}: {}", i + 1, trimmed));
                }
            }
        }
    }

    fn get_ui_execution_context(&self) -> Option<String> {
        self.ui_execution_context.clone()
    }

    fn get_ui_above_text(&self) -> Option<String> {
        self.ui_above_text.clone()
    }
}

impl ScreenStateDetector {
    /// ターミナルサイズ変更時にscreen bufferを再初期化
    pub fn resize_screen_buffer(&mut self, rows: usize, cols: usize) {
        if self.verbose {
            debug_println_raw(&format!(
                "🔄 [SCREEN_RESIZE] Resizing screen buffer to {}x{} (rows x cols)",
                rows, cols
            ));
        }
        self.screen_buffer = ScreenBuffer::new(rows, cols, self.verbose);
    }

    /// 現在のscreen bufferサイズを取得
    pub fn get_screen_buffer_size(&self) -> (usize, usize) {
        let lines = self.screen_buffer.get_screen_lines();
        (
            lines.len(),
            if lines.is_empty() { 0 } else { lines[0].len() },
        )
    }

    /// 現在の画面行を取得（Claude固有状態検出用）
    pub fn get_screen_lines(&self) -> Vec<String> {
        self.screen_buffer.get_screen_lines()
    }
}
