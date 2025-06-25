use std::collections::VecDeque;
use crate::protocol::SessionStatus;

/// Claude セッションの状態
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// アイドル状態（入力待ち）
    Idle,
    /// ビジー状態（処理中）
    Busy,
    /// ユーザー入力待ち（承認など）
    WaitingForInput,
    /// エラー状態
    Error,
    /// 接続中（初期状態）
    Connected,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Idle => write!(f, "⚪ Idle"),
            SessionState::Busy => write!(f, "🟢 Busy"),
            SessionState::WaitingForInput => write!(f, "⏳ Waiting"),
            SessionState::Error => write!(f, "🔴 Error"),
            SessionState::Connected => write!(f, "🔗 Connected"),
        }
    }
}

/// セッション状態検出器
pub struct SessionStateDetector {
    /// 出力バッファ（最後の30行を保持）
    output_buffer: VecDeque<String>,
    /// 現在の状態
    current_state: SessionState,
    /// 最大バッファサイズ
    max_buffer_lines: usize,
    /// デバッグモード
    verbose: bool,
}

impl SessionStateDetector {
    pub fn new(verbose: bool) -> Self {
        Self {
            output_buffer: VecDeque::new(),
            current_state: SessionState::Connected,
            max_buffer_lines: 30,
            verbose,
        }
    }

    /// 新しい出力を処理して状態を更新
    pub fn process_output(&mut self, output: &str) -> Option<SessionState> {
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

    /// 現在の状態を取得
    pub fn current_state(&self) -> &SessionState {
        &self.current_state
    }

    /// SessionStateをプロトコル用のSessionStatusに変換
    pub fn to_session_status(&self) -> SessionStatus {
        match &self.current_state {
            SessionState::Idle => SessionStatus::Idle,
            SessionState::Busy => SessionStatus::Busy,
            SessionState::WaitingForInput => SessionStatus::WaitingInput,
            SessionState::Error => SessionStatus::Error,
            SessionState::Connected => SessionStatus::Idle, // Connectedは一時的なのでIdleとして扱う
        }
    }

    /// バッファに行を追加
    fn add_line(&mut self, line: &str) {
        // ANSI エスケープシーケンスを除去
        let clean_line = self.strip_ansi(line);
        
        self.output_buffer.push_back(clean_line);
        
        // バッファサイズを制限
        while self.output_buffer.len() > self.max_buffer_lines {
            self.output_buffer.pop_front();
        }
    }

    /// 出力バッファから状態を検出
    fn detect_state(&self) -> SessionState {
        let recent_lines: Vec<&String> = self.output_buffer
            .iter()
            .rev()
            .take(10) // 最後の10行を確認
            .collect();

        // 最新の行から優先的にチェック（最新情報を優先）
        if let Some(last_line) = recent_lines.first() {
            // 最新行でのエラーパターン（強い優先度）
            if self.is_error_pattern(last_line) {
                return SessionState::Error;
            }
            // 最新行での入力待ちパターン（強い優先度）
            if self.is_waiting_pattern(last_line) {
                return SessionState::WaitingForInput;
            }
            // 最新行でのアイドルパターン（完了メッセージなど）
            if self.is_idle_pattern(last_line) {
                return SessionState::Idle;
            }
            // 最新行でのビジーパターン
            if self.is_busy_pattern(last_line) {
                return SessionState::Busy;
            }
        }

        // 最新行で決まらない場合は、最近の数行を確認
        for line in &recent_lines {
            if self.is_error_pattern(line) {
                return SessionState::Error;
            }
        }

        for line in &recent_lines {
            if self.is_waiting_pattern(line) {
                return SessionState::WaitingForInput;
            }
        }

        for line in &recent_lines {
            if self.is_busy_pattern(line) {
                return SessionState::Busy;
            }
        }

        for line in &recent_lines {
            if self.is_idle_pattern(line) {
                return SessionState::Idle;
            }
        }

        // どのパターンにもマッチしない場合は現在の状態を維持
        self.current_state.clone()
    }

    /// エラーパターンの検出
    fn is_error_pattern(&self, line: &str) -> bool {
        let line_lower = line.to_lowercase();
        line_lower.contains("error:") ||
        line_lower.contains("failed:") ||
        line_lower.contains("exception") ||
        line_lower.contains("❌") ||
        line_lower.contains("✗")
    }

    /// ユーザー入力待ちパターンの検出
    fn is_waiting_pattern(&self, line: &str) -> bool {
        let line_lower = line.to_lowercase();
        line_lower.contains("proceed?") ||
        line_lower.contains("continue?") ||
        line_lower.contains("confirm") ||
        line_lower.contains("y/n") ||
        line_lower.contains("press") ||
        line_lower.contains("wait") ||
        line_lower.contains("⏳") ||
        line_lower.contains("🤔")
    }

    /// ビジー状態パターンの検出
    fn is_busy_pattern(&self, line: &str) -> bool {
        let line_lower = line.to_lowercase();
        line_lower.contains("processing") ||
        line_lower.contains("executing") ||
        line_lower.contains("running") ||
        line_lower.contains("analyzing") ||
        line_lower.contains("thinking") ||
        line_lower.contains("working") ||
        line_lower.contains("applying") ||
        line_lower.contains("trying") ||
        line_lower.contains("retrying") ||
        line_lower.contains("分析中") ||
        line_lower.contains("処理中") ||
        line_lower.contains("実行中") ||
        line.contains("🔧") ||
        line.contains("⚙️") ||
        line.contains("📝") ||
        line.contains("📊") ||
        line.contains("🔍") ||
        line.contains("🚀") ||
        line_lower.starts_with("claude code:") // Claude Code のプロンプト
    }

    /// アイドル状態パターンの検出
    fn is_idle_pattern(&self, line: &str) -> bool {
        let line_lower = line.to_lowercase();
        line_lower.contains("ready") ||
        line_lower.contains("completed") ||
        line_lower.contains("finished") ||
        line_lower.contains("done") ||
        line_lower.contains("success") ||
        line_lower.contains("complete") ||
        line_lower.contains("完了") ||
        line_lower.contains("成功") ||
        line_lower.contains("正常") ||
        line.contains("✅") ||
        line.contains("✓") ||
        line.contains("🌟") ||
        line.contains("✨") ||
        line.contains("🎉") ||
        line_lower.ends_with("% ") || // シェルプロンプト
        line_lower.ends_with("$ ") ||  // シェルプロンプト
        line_lower.ends_with("> ") ||   // その他のプロンプト
        line_lower.contains("claude>") // Claude プロンプト
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

    /// デバッグ用：現在のバッファを表示
    pub fn debug_buffer(&self) {
        if self.verbose {
            println!("🔍 Buffer contents ({} lines):", self.output_buffer.len());
            for (i, line) in self.output_buffer.iter().enumerate() {
                println!("  {}: {}", i, line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_detection() {
        let mut detector = SessionStateDetector::new(false);
        
        // エラー状態のテスト
        assert_eq!(detector.process_output("Error: Something went wrong"), Some(SessionState::Error));
        assert_eq!(detector.current_state(), &SessionState::Error);
        
        // ビジー状態のテスト  
        detector = SessionStateDetector::new(false);
        assert_eq!(detector.process_output("🔧 Processing your request..."), Some(SessionState::Busy));
        assert_eq!(detector.current_state(), &SessionState::Busy);
        
        // アイドル状態のテスト
        detector = SessionStateDetector::new(false);
        assert_eq!(detector.process_output("✅ Task completed successfully"), Some(SessionState::Idle));
        assert_eq!(detector.current_state(), &SessionState::Idle);
    }

    #[test]
    fn test_ansi_stripping() {
        let detector = SessionStateDetector::new(false);
        let text_with_ansi = "\x1b[32mGreen text\x1b[0m normal";
        let cleaned = detector.strip_ansi(text_with_ansi);
        assert_eq!(cleaned, "Green text normal");
    }
}