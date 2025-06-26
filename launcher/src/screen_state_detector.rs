// screen_state_detector.rs - Screen buffer based state detection

use crate::screen_buffer::{ScreenBuffer, UIBox};
use crate::session_state::SessionState;
use crate::state_detector::{StateDetector, StatePatterns};
use ccmonitor_shared::SessionStatus;
use std::io::Write;

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
    patterns: StatePatterns,
    verbose: bool,
}

impl ScreenStateDetector {
    pub fn new(patterns: StatePatterns, verbose: bool) -> Self {
        // 標準的な端末サイズ（80x24）を使用
        // 実際のサイズはPTYから取得可能だが、簡略化のため固定
        let screen_buffer = ScreenBuffer::new(24, 80, verbose);
        
        Self {
            screen_buffer,
            current_state: SessionState::Connected,
            ui_execution_context: None,
            patterns,
            verbose,
        }
    }

    /// スクリーンから状態を検出
    fn detect_state_from_screen(&mut self) -> SessionState {
        let ui_boxes = self.screen_buffer.find_ui_boxes();
        
        if self.verbose {
            debug_println_raw(&format!("🖥️  [SCREEN_ANALYSIS] Found {} UI boxes", ui_boxes.len()));
            
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
                debug_println_raw(&format!("📦 [LATEST_UI_BOX] Analyzing box at rows {}-{}", 
                    latest_box.start_row, latest_box.end_row));
            }

            // UI box上の行から実行コンテキストを検出
            self.analyze_execution_context(&latest_box.above_lines);

            // UI boxの内容から状態を判定
            if let Some(state) = self.analyze_ui_box_content(latest_box) {
                if self.verbose && state != self.current_state {
                    debug_println_raw(&format!("🎯 [STATE_DETECTED] {} → {}", self.current_state, state));
                }
                return state;
            }
        }

        // UI boxが見つからない場合は現在の状態を維持
        if self.verbose {
            debug_println_raw(&format!("🔍 [NO_UI_BOX] No UI box found, maintaining state: {:?}", self.current_state));
        }
        self.current_state.clone()
    }

    /// 実行コンテキストを分析してui_execution_contextを更新
    fn analyze_execution_context(&mut self, above_lines: &[String]) {
        self.ui_execution_context = None;
        
        for line in above_lines {
            if line.contains("esc to interrupt") ||
               line.contains("Musing") ||
               line.contains("Auto-updating") ||
               line.contains("Tool:") ||
               line.contains("Wizarding") ||
               line.contains("Baking") {
                
                let context = Self::extract_short_context(line);
                self.ui_execution_context = Some(context.clone());
                
                if self.verbose {
                    debug_println_raw(&format!("⚡ [EXECUTION_CONTEXT] Found: {} → {}", line.trim(), context));
                }
                break;
            }
        }
    }

    /// UI boxの内容から状態を判定
    fn analyze_ui_box_content(&self, ui_box: &UIBox) -> Option<SessionState> {
        if self.verbose {
            debug_println_raw(&format!("🔍 [ANALYZING_UI_BOX] {} content lines", ui_box.content_lines.len()));
            for (i, line) in ui_box.content_lines.iter().enumerate() {
                debug_println_raw(&format!("  Content[{}]: '{}'", i, line));
            }
        }

        // 1. UI box内容での承認プロンプト検出（最優先）
        for content_line in &ui_box.content_lines {
            if content_line.contains("Do you want") ||
               content_line.contains("Would you like") ||
               content_line.contains("May I") ||
               content_line.contains("proceed?") ||
               content_line.contains("y/n") {
                if self.verbose {
                    debug_println_raw(&format!("⏳ [APPROVAL_DETECTED] {}", content_line));
                }
                return Some(SessionState::WaitingForInput);
            }
        }

        // 2. 上の行（実行コンテキスト）での実行状態検出
        for above_line in &ui_box.above_lines {
            if above_line.contains("esc to interrupt") ||
               above_line.contains("Musing") ||
               above_line.contains("Auto-updating") ||
               above_line.contains("Tool:") ||
               above_line.contains("Wizarding") ||
               above_line.contains("Baking") {
                if self.verbose {
                    debug_println_raw(&format!("⚡ [EXECUTION_ACTIVE] {}", above_line.trim()));
                }
                return Some(SessionState::Busy);
            }
        }

        // 3. 下の行（ステータス）でのエラー検出
        for below_line in &ui_box.below_lines {
            if below_line.contains("✗") || below_line.contains("failed") || below_line.contains("Error") {
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
}