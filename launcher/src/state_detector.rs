// state_detector.rs - 状態検出の抽象化レイヤー

use std::collections::VecDeque;
use crate::session_state::SessionState;
use ccmonitor_shared::SessionStatus;

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

    /// バッファに行を追加
    pub fn add_line(&mut self, line: &str) {
        // ANSI エスケープシーケンスを除去
        let clean_line = self.strip_ansi(line);
        
        self.output_buffer.push_back(clean_line);
        
        // バッファサイズを制限
        while self.output_buffer.len() > self.max_buffer_lines {
            self.output_buffer.pop_front();
        }
    }

    /// 出力バッファから状態を検出
    pub fn detect_state(&self) -> SessionState {
        let recent_lines: Vec<&String> = self.output_buffer
            .iter()
            .rev()
            .take(10) // 最後の10行を確認
            .collect();

        // 最新の行から優先的にチェック（最新情報を優先）
        if let Some(last_line) = recent_lines.first() {
            // 最新行でのエラーパターン（強い優先度）
            if self.is_pattern_match(last_line, &self.patterns.error_patterns) {
                return SessionState::Error;
            }
            // 最新行での入力待ちパターン（強い優先度）
            if self.is_pattern_match(last_line, &self.patterns.waiting_patterns) {
                return SessionState::WaitingForInput;
            }
            // 最新行でのアイドルパターン（完了メッセージなど）
            if self.is_pattern_match(last_line, &self.patterns.idle_patterns) {
                return SessionState::Idle;
            }
            // 最新行でのビジーパターン
            if self.is_pattern_match(last_line, &self.patterns.busy_patterns) {
                return SessionState::Busy;
            }
        }

        // 最新行で決まらない場合は、最近の数行を確認
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

        // どのパターンにもマッチしない場合は現在の状態を維持
        self.current_state.clone()
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
        CliToolType::Claude => Box::new(crate::claude_state_detector::ClaudeStateDetector::new(verbose)),
        CliToolType::Gemini => Box::new(crate::gemini_state_detector::GeminiStateDetector::new(verbose)),
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