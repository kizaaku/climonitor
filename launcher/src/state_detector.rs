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
        
        // その他の意味のある内容も抽出
        for line in &clean_lines {
            if line.trim().is_empty() {
                continue;
            }
            
            if let Some(meaningful_content) = self.extract_meaningful_content(line) {
                // UI blockで既に処理済みでない場合のみ追加
                if !filtered_lines.contains(&meaningful_content) {
                    filtered_lines.push(meaningful_content);
                }
            }
        }
        
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
        let mut lines_consumed = 0;
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
        lines_consumed = box_start_index + 1;

        // ボックス内のコンテンツを収集
        for line in lines.iter().skip(box_start_index + 1) {
            lines_consumed += 1;
            
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
        let status_start = lines_consumed;
        for (idx, line) in lines.iter().skip(status_start).take(3).enumerate() {
            if !line.trim().is_empty() {
                if self.verbose {
                    eprintln!("📍 [UI_STATUS] Line {}: {}", idx + 1, line.trim());
                }
                block_lines.push(format!("UI_STATUS: {}", line.trim()));
            }
            lines_consumed += 1;
        }

        Some(UiBlock {
            content: block_lines,
            lines_consumed,
        })
    }

    /// フィルタ済みバッファから状態を検出
    fn detect_state_from_filtered_buffer(&self, filtered_buffer: &[String]) -> SessionState {
        // 1. UI要素から状態を検出（最優先）
        for line in filtered_buffer {
            if let Some(state) = self.detect_from_ui_content(line) {
                if self.verbose && state != self.current_state {
                    eprintln!("🎯 [UI_STATE_TRIGGER] {} triggered by: {}", state, line);
                }
                return state;
            }
        }

        // 2. 従来の構造化された内容から検出
        for line in filtered_buffer {
            if let Some(state) = self.detect_from_structured_content(line) {
                if self.verbose && state != self.current_state {
                    eprintln!("🎯 [STATE_TRIGGER] {} triggered by: {}", state, line);
                }
                return state;
            }
        }

        // 2. ツール完了の推測：現在Busyで、"esc to interrupt"がない場合
        if self.current_state == SessionState::Busy {
            let has_interrupt = filtered_buffer.iter().any(|line| {
                line.contains("esc to interrupt") || line.contains("Auto-updating")
            });
            
            if !has_interrupt && !filtered_buffer.is_empty() {
                if self.verbose {
                    eprintln!("🔍 [COMPLETION_INFERENCE] No active tool indicators → Idle");
                }
                return SessionState::Idle;
            }
        }

        // 3. 従来のパターンマッチング（フォールバック）
        for line in filtered_buffer {
            if self.is_pattern_match(line, &self.patterns.error_patterns) {
                return SessionState::Error;
            }
            if self.is_pattern_match(line, &self.patterns.waiting_patterns) {
                return SessionState::WaitingForInput;
            }
            if self.is_pattern_match(line, &self.patterns.busy_patterns) {
                return SessionState::Busy;
            }
            if self.is_pattern_match(line, &self.patterns.idle_patterns) {
                return SessionState::Idle;
            }
        }

        // 状態変化なし
        self.current_state.clone()
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

    /// 行を処理すべきかどうかを判定
    fn should_process_line(&self, line: &str) -> bool {
        // 1. カーソル制御のみの行をスキップ
        if self.is_cursor_control_only(line) {
            return false;
        }

        // 2. 空行や意味のない行をスキップ
        let clean = self.strip_ansi_enhanced(line);
        if clean.trim().is_empty() {
            return false;
        }

        // 3. 繰り返し描画される装飾要素をスキップ
        if self.is_decorative_element(&clean) {
            return false;
        }

        true
    }

    /// カーソル制御のみの行かどうかを判定
    fn is_cursor_control_only(&self, line: &str) -> bool {
        let trimmed = line.trim();

        // カーソル移動やクリアのみのパターン
        if trimmed.starts_with('\x1b') {
            // よくあるカーソル制御パターン
            let patterns = [
                "[2K[1A[2K",    // 行クリア + カーソル上移動
                "[?25l[?2004h", // カーソル非表示 + bracketed paste
                "[G",           // カーソルを行頭に移動
            ];

            return patterns.iter().any(|pattern| trimmed.contains(pattern));
        }

        false
    }

    /// 装飾要素かどうかを判定
    fn is_decorative_element(&self, clean_line: &str) -> bool {
        let trimmed = clean_line.trim();

        // ボックス描画文字のみで構成される行
        if trimmed
            .chars()
            .all(|c| matches!(c, '─' | '│' | '╭' | '╮' | '╯' | '╰' | ' '))
        {
            return true;
        }

        // ショートカットヘルプ行（ステータスと混在する場合を除く）
        if trimmed == "? for shortcuts" {
            return true;
        }

        // ステータス情報のないショートカットヘルプ行のみフィルタ
        if trimmed.starts_with("? for shortcuts")
            && !trimmed.contains("◯")
            && !trimmed.contains("⧉")
            && !trimmed.contains("✗")
        {
            return true;
        }

        false
    }

    /// 意味のある内容を抽出
    fn extract_meaningful_content(&self, clean_line: &str) -> Option<String> {
        let trimmed = clean_line.trim();

        // 1. ユーザー入力エリア（ccmanager参考）
        if trimmed.starts_with("│ > ") {
            let content = trimmed
                .trim_start_matches("│ > ")
                .trim_end_matches(" │")
                .trim();
            if !content.is_empty() {
                return Some(format!("USER_INPUT: {}", content));
            }
        }

        // 2. Claude の承認プロンプト（ccmanager パターン）
        if trimmed.contains("│ Do you want")
            || trimmed.contains("│ Would you like")
            || trimmed.contains("│ May I")
        {
            return Some(format!("APPROVAL_PROMPT: {}", trimmed));
        }

        // 3. ステータス情報（重要なもののみ抽出）
        if trimmed.contains("◯") || trimmed.contains("✗") {
            // 重要なステータス部分のみを抽出
            let status_part = if let Some(pos) = trimmed.find("◯") {
                &trimmed[pos..]
            } else if let Some(pos) = trimmed.find("✗") {
                &trimmed[pos..]
            } else {
                trimmed
            };

            return Some(format!("STATUS: {}", status_part.trim()));
        }

        // ⧉ In はファイル名表示なので無視（状態検出に使わない）

        // 4. エラーメッセージ
        if trimmed.contains("Error:") || trimmed.contains("failed") || trimmed.contains("API Error")
        {
            return Some(format!("ERROR: {}", trimmed));
        }

        // 5. ツール実行・完了メッセージ
        if trimmed.contains("esc to interrupt") {
            return Some(format!("TOOL_STATUS: {}", trimmed));
        }

        // 6. その他の重要そうな内容（絵文字や特定キーワード含む）
        if trimmed.contains("🤔")
            || trimmed.contains("⏳")
            || trimmed.contains("proceed?")
            || trimmed.contains("y/n")
        {
            return Some(format!("INTERACTION: {}", trimmed));
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


    /// 構造化された内容から状態を検出
    fn detect_from_structured_content(&self, line: &str) -> Option<SessionState> {
        // ccmanager のパターンを参考にした高精度検出

        // 1. 承認プロンプト（最高優先度）
        if line.starts_with("APPROVAL_PROMPT:") {
            if line.contains("Do you want")
                || line.contains("Would you like")
                || line.contains("May I")
            {
                if self.verbose {
                    eprintln!("🔍 [STATE] Approval prompt detected → WaitingForInput");
                }
                return Some(SessionState::WaitingForInput);
            }
        }

        // 2. エラー状態（高優先度）
        if line.starts_with("ERROR:") {
            if self.verbose {
                eprintln!("🔍 [STATE] Error detected → Error");
            }
            return Some(SessionState::Error);
        }

        // 3. ステータス行からの検出（高優先度）
        if line.starts_with("STATUS:") {
            if line.contains("✗") || line.contains("failed") {
                if self.verbose {
                    eprintln!("🔍 [STATE] Status error detected → Error");
                }
                return Some(SessionState::Error);
            }
            // ⧉ In は単なるファイル名表示なので無視
            if line.contains("◯ IDE connected") {
                if self.verbose {
                    eprintln!("🔍 [STATE] IDE connected → Idle");
                }
                return Some(SessionState::Idle);
            }
        }

        // 4. ツール状態（中優先度）
        if line.starts_with("TOOL_STATUS:") {
            if line.contains("esc to interrupt") {
                if self.verbose {
                    eprintln!("🔍 [STATE] Tool execution detected (esc to interrupt) → Busy");
                }
                return Some(SessionState::Busy); // ツール実行中
            }
            if line.contains("Auto-updating") {
                if self.verbose {
                    eprintln!("🔍 [STATE] Auto-updating detected → Busy");
                }
                return Some(SessionState::Busy);
            }
            if line.contains("Tool:") {
                if self.verbose {
                    eprintln!("🔍 [STATE] Tool execution detected → Busy");
                }
                return Some(SessionState::Busy);
            }
            if line.contains("✅")
                || line.contains("completed")
                || line.contains("finished")
                || line.contains("done")
            {
                if self.verbose {
                    eprintln!("🔍 [STATE] Tool completed → Idle");
                }
                return Some(SessionState::Idle); // ツール完了
            }
        }

        // 5. インタラクション（中優先度）
        if line.starts_with("INTERACTION:") {
            if line.contains("proceed?") || line.contains("y/n") {
                return Some(SessionState::WaitingForInput);
            }
        }

        // 6. ユーザー入力（低優先度、参考程度）
        if line.starts_with("USER_INPUT:") {
            return Some(SessionState::Idle); // ユーザーが入力中は基本的にIdle
        }

        None
    }

    /// パターンマッチングの実行
    fn is_pattern_match(&self, line: &str, patterns: &[String]) -> bool {
        let line_lower = line.to_lowercase();
        patterns.iter().any(|pattern| {
            let pattern_lower = pattern.to_lowercase();
            line_lower.contains(&pattern_lower) || line.contains(pattern)
        })
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
