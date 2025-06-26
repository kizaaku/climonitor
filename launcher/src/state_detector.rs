// state_detector.rs - 状態検出の抽象化レイヤー

use crate::session_state::SessionState;
use ccmonitor_shared::SessionStatus;
use std::collections::VecDeque;

/// UIブロック（╭╮╰╯で囲まれた部分）の解析結果
#[derive(Debug)]
struct UiBlock {
    content: Vec<String>,
    lines_consumed: usize,
}

/// 状態検出パターンの定義
#[derive(Debug, Clone)]
pub struct StatePatterns {
    pub error_patterns: Vec<String>,
    pub waiting_patterns: Vec<String>,
    pub busy_patterns: Vec<String>,
    pub idle_patterns: Vec<String>,
}

impl StatePatterns {
    pub fn new() -> Self {
        Self {
            error_patterns: Vec::new(),
            waiting_patterns: Vec::new(),
            busy_patterns: Vec::new(),
            idle_patterns: Vec::new(),
        }
    }
}

/// 状態検出器の共通インターフェース
pub trait StateDetector: Send + Sync {
    /// 新しい出力を処理して状態を更新
    fn process_output(&mut self, output: &str) -> Option<SessionState>;

    /// 現在の状態を取得
    fn current_state(&self) -> &SessionState;

    /// SessionStateをプロトコル用のSessionStatusに変換
    fn to_session_status(&self) -> SessionStatus;

    /// 状態検出パターンを取得
    fn get_patterns(&self) -> &StatePatterns;

    /// デバッグ用：現在のバッファを表示
    fn debug_buffer(&self);
}

/// 基本的な状態検出器の実装
pub struct BaseStateDetector {
    /// 生の出力バッファ（最後の20行を保持）
    raw_buffer: VecDeque<String>,
    /// 現在の状態
    current_state: SessionState,
    /// バッファサイズ（20行）
    buffer_size: usize,
    /// デバッグモード
    verbose: bool,
    /// 状態検出パターン
    patterns: StatePatterns,
}

impl BaseStateDetector {
    pub fn new(patterns: StatePatterns, verbose: bool) -> Self {
        Self {
            raw_buffer: VecDeque::new(),
            current_state: SessionState::Connected,
            buffer_size: 20,
            verbose,
            patterns,
        }
    }

    /// 20行バッファ全体から状態を検出
    fn detect_state_from_buffer(&self) -> SessionState {
        if self.raw_buffer.is_empty() {
            return self.current_state.clone();
        }

        if self.verbose {
            eprintln!("🔍 [BUFFER_ANALYSIS] Processing {} lines as whole buffer", self.raw_buffer.len());
        }

        // バッファ全体を一括でスマートフィルタリング
        let filtered_buffer = self.smart_filter_buffer();

        if self.verbose {
            eprintln!("📜 [FILTERED_BUFFER] After filtering:");
            for (i, line) in filtered_buffer.iter().enumerate() {
                eprintln!("  {:2}: {}", i + 1, line);
            }
        }

        // フィルタ済みバッファをそのまま状態検出に渡す
        self.detect_state_from_filtered_buffer(&filtered_buffer)
    }

    /// バッファ全体を一括でスマートフィルタリング
    fn smart_filter_buffer(&self) -> Vec<String> {
        let mut filtered_lines = Vec::new();
        
        // 20行全体をクリーンアップ（ANSI除去のみ）
        let clean_lines: Vec<String> = self.raw_buffer
            .iter()
            .rev() // 最新から古い順
            .map(|line| self.strip_ansi_enhanced(line))
            .collect();
        
        // ╭╮╰╯パターンを検出してユーザー入力とステータスを抽出
        self.extract_ui_blocks(&clean_lines, &mut filtered_lines);
        
        // UIボックス基準のみの検出完了
        
        filtered_lines
    }

    /// ╭╮╰╯で囲まれたUIブロックを検出・抽出
    fn extract_ui_blocks(&self, clean_lines: &[String], filtered_lines: &mut Vec<String>) {
        let mut i = 0;
        
        while i < clean_lines.len() {
            let line = &clean_lines[i];
            
            // ╭で始まるボックスを検出
            if line.trim_start().starts_with('╭') {
                if let Some(ui_block) = self.parse_ui_block(&clean_lines[i..]) {
                    filtered_lines.extend(ui_block.content);
                    i += ui_block.lines_consumed;
                    continue;
                }
            }
            i += 1;
        }
    }

    /// UIブロックをパース
    fn parse_ui_block(&self, lines: &[String]) -> Option<UiBlock> {
        if lines.is_empty() {
            return None;
        }

        let mut block_lines = Vec::new();
        let mut _lines_consumed = 0;
        let mut found_bottom = false;
        let mut box_start_index = 0;

        // ╭で始まる行のインデックスを見つける
        for (idx, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with('╭') {
                box_start_index = idx;
                break;
            }
        }

        // UIブロックの上の行を抽出（実行状況情報）
        if box_start_index > 0 {
            for i in 0..box_start_index {
                let upper_line = &lines[i];
                if !upper_line.trim().is_empty() {
                    if self.verbose {
                        eprintln!("🔝 [UI_UPPER] Line {}: {}", i + 1, upper_line.trim());
                    }
                    
                    // 実行状況を示す情報を抽出
                    if upper_line.contains("esc to interrupt") ||
                       upper_line.contains("Auto-updating") ||
                       upper_line.contains("Tool:") ||
                       upper_line.contains("Wizarding") ||
                       upper_line.contains("Baking") {
                        block_lines.push(format!("UI_EXECUTION: {}", upper_line.trim()));
                    } else {
                        block_lines.push(format!("UI_CONTEXT: {}", upper_line.trim()));
                    }
                }
            }
        }

        // ╭で始まる行を確認
        if !lines[box_start_index].trim_start().starts_with('╭') {
            return None;
        }
        _lines_consumed = box_start_index + 1;

        // ボックス内のコンテンツを収集
        for line in lines.iter().skip(box_start_index + 1) {
            _lines_consumed += 1;
            
            // ╰で終わるボックスを検出
            if line.trim_start().starts_with('╰') {
                found_bottom = true;
                break;
            }
            
            // ユーザー入力内容を抽出（│で囲まれた部分）
            if line.contains('│') {
                let content = line.trim();
                if content.starts_with('│') && content.ends_with('│') {
                    let inner_content = content.trim_start_matches('│')
                                               .trim_end_matches('│')
                                               .trim();
                    if !inner_content.is_empty() {
                        block_lines.push(format!("USER_INPUT: {}", inner_content));
                    }
                }
            }
        }

        if !found_bottom {
            return None;
        }

        // ボックスの下3行をステータス要素として収集
        let status_start = _lines_consumed;
        for (idx, line) in lines.iter().skip(status_start).take(3).enumerate() {
            if !line.trim().is_empty() {
                if self.verbose {
                    eprintln!("📍 [UI_STATUS] Line {}: {}", idx + 1, line.trim());
                }
                block_lines.push(format!("UI_STATUS: {}", line.trim()));
            }
            _lines_consumed += 1;
        }

        Some(UiBlock {
            content: block_lines,
            lines_consumed: _lines_consumed,
        })
    }

    /// フィルタ済みバッファから状態を検出（UIボックス基準のみ）
    fn detect_state_from_filtered_buffer(&self, filtered_buffer: &[String]) -> SessionState {
        // UIボックス基準での状態検出のみ
        for line in filtered_buffer {
            if let Some(state) = self.detect_from_ui_content(line) {
                if self.verbose && state != self.current_state {
                    eprintln!("🎯 [UI_STATE_TRIGGER] {} triggered by: {}", state, line);
                }
                return state;
            }
        }

        // UIボックスが見つからない場合のデフォルト処理
        if filtered_buffer.is_empty() {
            if self.verbose {
                eprintln!("🔍 [NO_UI_BOX] Empty buffer, maintaining current state");
            }
            return self.current_state.clone();
        }

        // UIボックスはあるが状態を決定できない場合はIdle
        if self.verbose {
            eprintln!("🔍 [UI_BOX_FOUND] UI elements present but no state indicators → Idle");
        }
        SessionState::Idle
    }

    /// UI要素から状態を検出
    fn detect_from_ui_content(&self, line: &str) -> Option<SessionState> {
        // UI実行情報（最優先）
        if line.starts_with("UI_EXECUTION:") {
            let exec_content = line.trim_start_matches("UI_EXECUTION:").trim();
            
            if exec_content.contains("esc to interrupt") ||
               exec_content.contains("Wizarding") ||
               exec_content.contains("Baking") ||
               exec_content.contains("Auto-updating") {
                if self.verbose {
                    eprintln!("⚡ [UI_EXECUTION_DETECTED] {} → Busy", exec_content);
                }
                return Some(SessionState::Busy); // ツール実行中
            }
            
            if exec_content.contains("Tool:") {
                if self.verbose {
                    eprintln!("🔧 [UI_TOOL_DETECTED] {} → Busy", exec_content);
                }
                return Some(SessionState::Busy);
            }
        }

        // UI文脈情報
        if line.starts_with("UI_CONTEXT:") {
            let context_content = line.trim_start_matches("UI_CONTEXT:").trim();
            if self.verbose {
                eprintln!("💭 [UI_CONTEXT_DETECTED] {}", context_content);
            }
        }

        // ユーザー入力要素
        if line.starts_with("USER_INPUT:") {
            let content = line.trim_start_matches("USER_INPUT:").trim();
            if !content.is_empty() {
                if self.verbose {
                    eprintln!("📝 [USER_INPUT_DETECTED] {}", content);
                }
                return Some(SessionState::Idle); // ユーザーが入力中
            }
        }

        // UIステータス要素
        if line.starts_with("UI_STATUS:") {
            let status_content = line.trim_start_matches("UI_STATUS:").trim();
            
            // ステータス内容から状態を推測
            if status_content.contains("◯ IDE connected") {
                return Some(SessionState::Idle);
            }
            if status_content.contains("⧉ In") {
                return Some(SessionState::Busy); // ファイル編集中
            }
            if status_content.contains("✗") || status_content.contains("failed") {
                return Some(SessionState::Error);
            }
            if status_content.contains("esc to interrupt") {
                return Some(SessionState::Busy); // ツール実行中
            }
            
            if self.verbose {
                eprintln!("📊 [UI_STATUS_DETECTED] {}", status_content);
            }
        }

        None
    }





    /// 強化されたANSI除去
    fn strip_ansi_enhanced(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // ANSI エスケープシーケンスの開始
                if chars.peek() == Some(&'[') {
                    chars.next(); // '[' をスキップ

                    // パラメータとコマンド文字をスキップ
                    while let Some(ch) = chars.next() {
                        if ch.is_ascii_alphabetic() || ch == '~' {
                            break; // 終端文字で終了
                        }
                    }
                } else if chars.peek() == Some(&']') {
                    // OSC (Operating System Command) シーケンス
                    chars.next(); // ']' をスキップ
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' || (ch == '\x1b' && chars.peek() == Some(&'\\')) {
                            if ch == '\x1b' {
                                chars.next(); // '\' をスキップ
                            }
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




    /// ANSI エスケープシーケンスを除去（簡易版）
    #[allow(dead_code)]
    fn strip_ansi(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                // ANSI エスケープシーケンスをスキップ
                chars.next(); // '['をスキップ
                while let Some(ch) = chars.next() {
                    if ch.is_ascii_alphabetic() {
                        break; // 終端文字で終了
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// 現在の状態を設定
    pub fn set_current_state(&mut self, state: SessionState) {
        self.current_state = state;
    }

    /// 出力バッファの参照を取得
    pub fn get_raw_buffer(&self) -> &VecDeque<String> {
        &self.raw_buffer
    }

    /// verboseフラグを取得
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }
}

impl StateDetector for BaseStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        if self.verbose && !output.trim().is_empty() {
            eprintln!("🔄 [PROCESS_OUTPUT] Adding lines to buffer");
        }
        
        // 出力を行ごとに分割して生バッファに追加
        for line in output.lines() {
            self.raw_buffer.push_back(line.to_string());
            
            // バッファサイズを20行に制限
            while self.raw_buffer.len() > self.buffer_size {
                self.raw_buffer.pop_front();
            }
        }

        // バッファが20行貯まったら（または変化があったら）状態を検出
        let new_state = self.detect_state_from_buffer();

        // 状態が変化した場合のみ通知
        if new_state != self.current_state {
            let old_state = self.current_state.clone();
            self.current_state = new_state.clone();

            if self.verbose {
                eprintln!("🎯 [STATE_CHANGE] {} → {}", old_state, new_state);
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
            SessionState::Connected => SessionStatus::Idle, // Connectedは一時的なのでIdleとして扱う
        }
    }

    fn get_patterns(&self) -> &StatePatterns {
        &self.patterns
    }

    fn debug_buffer(&self) {
        if self.verbose {
            println!("🔍 Buffer contents ({} lines):", self.raw_buffer.len());
            for (i, line) in self.raw_buffer.iter().enumerate() {
                println!("  {}: {}", i, line);
            }
        }
    }
}

/// 状態検出器のファクトリー
use crate::cli_tool::CliToolType;

pub fn create_state_detector(tool_type: CliToolType, verbose: bool) -> Box<dyn StateDetector> {
    match tool_type {
        CliToolType::Claude => Box::new(crate::claude_state_detector::ClaudeStateDetector::new(
            verbose,
        )),
        CliToolType::Gemini => Box::new(crate::gemini_state_detector::GeminiStateDetector::new(
            verbose,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_patterns() {
        let mut patterns = StatePatterns::new();
        patterns.error_patterns.push("error:".to_string());
        patterns.busy_patterns.push("🔧".to_string());

        let detector = BaseStateDetector::new(patterns, false);

        assert_eq!(detector.current_state(), &SessionState::Connected);
        assert!(!detector.get_patterns().error_patterns.is_empty());
    }

    #[test]
    fn test_ansi_stripping() {
        let patterns = StatePatterns::new();
        let detector = BaseStateDetector::new(patterns, false);
        let text_with_ansi = "\x1b[32mGreen text\x1b[0m normal";
        let cleaned = detector.strip_ansi(text_with_ansi);
        assert_eq!(cleaned, "Green text normal");
    }
}
