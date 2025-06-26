// state_detector.rs - 状態検出の抽象化レイヤー

use crate::session_state::SessionState;
use ccmonitor_shared::SessionStatus;
use std::collections::VecDeque;

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
    /// 出力バッファ（最後の30行を保持）
    output_buffer: VecDeque<String>,
    /// 現在の状態
    current_state: SessionState,
    /// 最大バッファサイズ
    #[allow(dead_code)]
    max_buffer_lines: usize,
    /// デバッグモード
    verbose: bool,
    /// 状態検出パターン
    patterns: StatePatterns,
}

impl BaseStateDetector {
    pub fn new(patterns: StatePatterns, verbose: bool) -> Self {
        Self {
            output_buffer: VecDeque::new(),
            current_state: SessionState::Connected,
            max_buffer_lines: 30,
            verbose,
            patterns,
        }
    }

    /// バッファに行を追加（スマートフィルタリング）
    pub fn add_line(&mut self, line: &str) {
        // スマートフィルタリングを適用
        if !self.should_process_line(line) {
            // フィルタされた行は表示しない（ノイズ削減）
            return;
        }

        // ANSI エスケープシーケンスを除去
        let clean_line = self.strip_ansi_enhanced(line);

        // 意味のある内容を抽出
        if let Some(meaningful_content) = self.extract_meaningful_content(&clean_line) {
            self.output_buffer.push_back(meaningful_content.clone());

            if self.verbose {
                eprintln!("✨ [EXTRACTED] {}", meaningful_content);
            }

            // バッファサイズを制限（20行に拡張）
            while self.output_buffer.len() > 20 {
                self.output_buffer.pop_front();
            }
        }
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

    /// 出力バッファから状態を検出（スマートフィルタリング版）
    pub fn detect_state(&self) -> SessionState {
        let recent_lines: Vec<&String> = self
            .output_buffer
            .iter()
            .rev()
            .take(10) // 最後の10行を確認
            .collect();

        // バッファ履歴は状態変化時のみ表示（ノイズ削減）

        // 1. 構造化された内容から優先的に検出
        for line in &recent_lines {
            if let Some(state) = self.detect_from_structured_content(line) {
                if self.verbose && state != self.current_state {
                    eprintln!("\n🎯 [STATE_CHANGE] {} → {}", self.current_state, state);
                    eprintln!("📜 [BUFFER] Recent lines:");
                    for (i, buffer_line) in recent_lines.iter().enumerate() {
                        let marker = if buffer_line == line { "➤" } else { " " };
                        eprintln!("  {}{:2}: {}", marker, i + 1, buffer_line);
                    }
                    eprintln!("");
                }
                return state;
            }
        }

        // 2. 従来のパターンマッチング（フォールバック）
        for line in &recent_lines {
            if self.is_pattern_match(line, &self.patterns.error_patterns) {
                return SessionState::Error;
            }
        }

        for line in &recent_lines {
            if self.is_pattern_match(line, &self.patterns.waiting_patterns) {
                return SessionState::WaitingForInput;
            }
        }

        for line in &recent_lines {
            if self.is_pattern_match(line, &self.patterns.busy_patterns) {
                return SessionState::Busy;
            }
        }

        for line in &recent_lines {
            if self.is_pattern_match(line, &self.patterns.idle_patterns) {
                return SessionState::Idle;
            }
        }

        // 3. ツール完了の推測：現在Busyで、最近のバッファに"esc to interrupt"がない場合
        if self.current_state == SessionState::Busy {
            let interrupt_lines: Vec<_> = recent_lines
                .iter()
                .filter(|line| line.contains("esc to interrupt") || line.contains("Auto-updating"))
                .collect();

            if interrupt_lines.is_empty() && !recent_lines.is_empty() {
                if self.verbose {
                    eprintln!(
                        "\n🎯 [STATE_CHANGE] {} → {}",
                        self.current_state,
                        SessionState::Idle
                    );
                    eprintln!(
                        "🔍 [REASON] No active tool indicators found (esc to interrupt absent)"
                    );
                    eprintln!("📜 [BUFFER] Recent lines:");
                    for (i, buffer_line) in recent_lines.iter().enumerate() {
                        eprintln!("   {:2}: {}", i + 1, buffer_line);
                    }
                    eprintln!("");
                }
                return SessionState::Idle;
            }
        }

        // どのパターンにもマッチしない場合は現在の状態を維持
        self.current_state.clone()
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
    pub fn get_output_buffer(&self) -> &VecDeque<String> {
        &self.output_buffer
    }

    /// verboseフラグを取得
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }
}

impl StateDetector for BaseStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        // 出力を行ごとに分割してバッファに追加
        for line in output.lines() {
            self.add_line(line);
        }

        // 状態を検出
        let new_state = self.detect_state();

        // 状態が変化した場合のみ通知
        if new_state != self.current_state {
            let old_state = self.current_state.clone();
            self.current_state = new_state.clone();

            if self.verbose {
                println!("🔄 State changed: {} -> {}", old_state, new_state);
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
            println!("🔍 Buffer contents ({} lines):", self.output_buffer.len());
            for (i, line) in self.output_buffer.iter().enumerate() {
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
