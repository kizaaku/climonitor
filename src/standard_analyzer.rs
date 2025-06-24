use regex::Regex;
use crate::protocol::SessionStatus;
use crate::ansi_utils::{clean_for_analysis, contains_claude_ui_elements};

/// 非DEBUGモード対応の標準出力解析
pub struct StandardAnalyzer {
    // ccmanager風のターミナルバッファ解析パターン
    waiting_confirmation_pattern: Regex,
    busy_execution_pattern: Regex,
    
    // 既存の出力パターン（正規表現）
    tool_execution_pattern: Regex,
    waiting_input_pattern: Regex,
    completion_pattern: Regex,
    error_pattern: Regex,
    api_request_pattern: Regex,
    
    // Claude UI box detection patterns
    box_start_pattern: Regex,
    box_end_pattern: Regex,
    usage_limit_pattern: Regex,
    
    // 状態推定用データ
    last_output_time: Option<chrono::DateTime<chrono::Utc>>,
    recent_outputs: Vec<String>,
    terminal_buffer: String,
    max_recent_outputs: usize,
    last_state: Option<SessionStatus>,
    
    // Claude UI parsing state
    in_ui_box: bool,
    ui_box_content: Vec<String>,
    last_context_before_box: String,
    usage_limit_reset_time: Option<String>,
}

/// 解析結果
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub status: SessionStatus,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub message: Option<String>,
    pub launcher_context: Option<String>,
    pub usage_reset_time: Option<String>,
    pub is_waiting_for_execution: bool,
}

impl StandardAnalyzer {
    pub fn new() -> Self {
        Self {
            // ccmanager風のターミナルバッファ解析パターン
            waiting_confirmation_pattern: Regex::new(r"(?i)(do you want|would you like|press enter to continue|esc to interrupt|continue\?|proceed\?|\[y/n\]|\[yes/no\])").unwrap(),
            busy_execution_pattern: Regex::new(r"(?i)(esc to interrupt|executing|running|in progress|processing|loading|please wait)").unwrap(),
            
            // Claude標準出力パターン（強化版）
            tool_execution_pattern: Regex::new(r"(?i)(using tool|executing|running|calling|\btool\b.*started|\btool\b.*running)").unwrap(),
            waiting_input_pattern: Regex::new(r"(?i)(what would you like|press enter|continue|what's next|\?|\.\.\.)$").unwrap(),
            completion_pattern: Regex::new(r"(?i)(finished|done|completed|success|✅|✓|task completed|execution complete)").unwrap(),
            error_pattern: Regex::new(r"(?i)(error|failed|exception|❌|✗|cannot|unable|\bfail\b|\bbug\b|\bissue\b)").unwrap(),
            api_request_pattern: Regex::new(r"(?i)(thinking|processing|analyzing|generating|requesting|api call|making request)").unwrap(),
            
            // Claude UI box detection patterns (handle ANSI escape sequences)
            box_start_pattern: Regex::new(r"(?:\x1b\[[0-9;]*m)*[╭┌]").unwrap(),
            box_end_pattern: Regex::new(r"(?:\x1b\[[0-9;]*m)*[╯└]").unwrap(),
            usage_limit_pattern: Regex::new(r"(?i)(?:approaching\s+usage\s+limit|usage\s+limit).*?resets?\s+at\s+(\d{1,2}[ap]m|\d{1,2}:\d{2})").unwrap(),
            
            last_output_time: None,
            recent_outputs: Vec::new(),
            terminal_buffer: String::new(),
            max_recent_outputs: 20,
            last_state: None,
            
            // Claude UI parsing state
            in_ui_box: false,
            ui_box_content: Vec::new(),
            last_context_before_box: String::new(),
            usage_limit_reset_time: None,
        }
    }

    /// 出力行を解析（ccmanager風のターミナルバッファ解析統合）
    pub fn analyze_output(&mut self, output: &str, stream: &str) -> AnalysisResult {
        let now = chrono::Utc::now();
        self.last_output_time = Some(now);
        
        // Clean output for analysis
        let clean_output = clean_for_analysis(output);
        
        // Claude UI box detection and parsing
        self.parse_claude_ui_output(&clean_output);
        
        // ターミナルバッファを更新
        self.update_terminal_buffer(&clean_output);
        
        // 最近の出力を記録
        self.add_recent_output(clean_output.clone());
        
        // 複数解析手法を組み合わせ
        let line_result = self.analyze_single_line(&clean_output, stream);
        let buffer_result = self.analyze_terminal_buffer();
        let context_result = self.analyze_context();
        let ui_result = self.analyze_claude_ui_state();
        
        // 最適な結果を選択
        self.combine_results(vec![line_result, buffer_result, context_result, ui_result])
    }
    
    /// 単一行解析（ccmanager風の4状態対応）
    fn analyze_single_line(&self, output: &str, stream: &str) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut status = SessionStatus::Busy;

        let confidence = if self.error_pattern.is_match(output) {
            status = SessionStatus::Error;
            evidence.push(format!("Error pattern detected in {} line", stream));
            0.9
        }
        // 入力待ちパターン（確認待ち）
        else if self.waiting_input_pattern.is_match(output) {
            status = SessionStatus::WaitingInput;
            evidence.push(format!("Input waiting pattern detected in {} line", stream));
            0.9
        }
        // ツール実行パターン（実行中）
        else if self.tool_execution_pattern.is_match(output) {
            status = SessionStatus::Busy;
            evidence.push(format!("Tool execution pattern detected in {} line", stream));
            0.8
        }
        // API処理パターン（実行中）
        else if self.api_request_pattern.is_match(output) {
            status = SessionStatus::Busy;
            evidence.push(format!("API processing pattern detected in {} line", stream));
            0.6
        }
        // 完了パターン（アイドル）
        else if self.completion_pattern.is_match(output) {
            status = SessionStatus::Idle;
            evidence.push(format!("Completion pattern detected in {} line", stream));
            0.7
        }
        else {
            evidence.push("No specific pattern matched in line".to_string());
            0.1
        };

        AnalysisResult {
            status,
            confidence,
            evidence,
            message: Some(output.to_string()),
            launcher_context: None,
            usage_reset_time: None,
            is_waiting_for_execution: false,
        }
    }
    
    /// ccmanager風のターミナルバッファ全体解析（4状態対応）
    pub fn analyze_terminal_buffer(&self) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut status = SessionStatus::Idle;
        
        let confidence = if self.waiting_confirmation_pattern.is_match(&self.terminal_buffer) {
            status = SessionStatus::WaitingInput;
            evidence.push("Confirmation prompt detected in terminal buffer".to_string());
            0.95
        }
        // 2. 実行中状態の検出 - ccmanager風
        else if self.busy_execution_pattern.is_match(&self.terminal_buffer) {
            status = SessionStatus::Busy;
            evidence.push("Busy execution pattern detected in terminal buffer".to_string());
            0.90
        }
        // 3. エラー状態の検出
        else if self.error_pattern.is_match(&self.terminal_buffer) {
            status = SessionStatus::Error;
            evidence.push("Error pattern detected in terminal buffer".to_string());
            0.85
        }
        // 4. 完了/アイドル状態の検出
        else if self.completion_pattern.is_match(&self.terminal_buffer) {
            status = SessionStatus::Idle;
            evidence.push("Completion pattern detected in terminal buffer".to_string());
            0.75
        } else {
            evidence.push("No clear pattern in terminal buffer".to_string());
            0.3
        };
        
        AnalysisResult {
            status,
            confidence,
            evidence,
            message: None,
            launcher_context: None,
            usage_reset_time: None,
            is_waiting_for_execution: false,
        }
    }

    /// プロセス状態から解析（4状態対応）
    pub fn analyze_process_state(
        &self,
        cpu_percent: f32,
        child_count: u32,
        network_active: bool,
    ) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut confidence = 0.0;
        let mut status = SessionStatus::Idle;

        // 高CPU使用率 = 実行中
        if cpu_percent > 10.0 {
            status = SessionStatus::Busy;
            confidence = 0.7;
            evidence.push(format!("High CPU usage: {:.1}% - busy", cpu_percent));
        }

        // 子プロセス存在 = 実行中
        if child_count > 0 {
            status = SessionStatus::Busy;
            confidence = 0.8;
            evidence.push(format!("Child processes: {} - busy", child_count));
        }

        // ネットワーク活動 = 実行中
        if network_active {
            status = SessionStatus::Busy;
            confidence = 0.6;
            evidence.push("Network activity detected - busy".to_string());
        }

        // 低活動 = 確認待ちまたはアイドル
        if cpu_percent < 1.0 && child_count == 0 && !network_active {
            status = SessionStatus::WaitingInput;
            confidence = 0.5;
            evidence.push("Low activity - possibly waiting for input".to_string());
        }

        AnalysisResult {
            status,
            confidence,
            evidence,
            message: None,
            launcher_context: None,
            usage_reset_time: None,
            is_waiting_for_execution: false,
        }
    }

    /// 文脈解析（最近の出力から推測）- 4状態対応
    fn analyze_context(&self) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut confidence = 0.3; // 低信頼度
        let mut status = SessionStatus::Busy;

        // 最近の出力がない = アイドル
        if let Some(last_time) = self.last_output_time {
            let silence_duration = chrono::Utc::now() - last_time;
            
            if silence_duration.num_minutes() > 5 {
                status = SessionStatus::Idle;
                confidence = 0.9;
                evidence.push(format!("No output for {} minutes - idle", silence_duration.num_minutes()));
            } else if silence_duration.num_seconds() > 30 {
                status = SessionStatus::WaitingInput;
                confidence = 0.6;
                evidence.push(format!("No output for {} seconds - possibly waiting", silence_duration.num_seconds()));
            }
        }

        // 最近の出力パターン分析
        let recent_text = self.recent_outputs.join(" ");
        if recent_text.contains("?") || recent_text.ends_with("...") {
            status = SessionStatus::WaitingInput;
            confidence = 0.7;
            evidence.push("Question or incomplete statement detected".to_string());
        }

        AnalysisResult {
            status,
            confidence,
            evidence,
            message: None,
            launcher_context: None,
            usage_reset_time: None,
            is_waiting_for_execution: false,
        }
    }

    /// 最近の出力を追加
    fn add_recent_output(&mut self, output: String) {
        self.recent_outputs.push(output);
        if self.recent_outputs.len() > self.max_recent_outputs {
            self.recent_outputs.remove(0);
        }
    }
    
    /// ターミナルバッファを更新（ccmanager風）
    fn update_terminal_buffer(&mut self, output: &str) {
        self.terminal_buffer.push_str(output);
        self.terminal_buffer.push('\n');
        
        // バッファサイズ制限（最新の2000文字を保持、UTF-8セーフ）
        if self.terminal_buffer.len() > 2000 {
            // 文字境界を考慮して安全に切り詰める
            let mut chars: Vec<char> = self.terminal_buffer.chars().collect();
            if chars.len() > 2000 {
                let start = chars.len() - 2000;
                chars.drain(0..start);
                self.terminal_buffer = chars.into_iter().collect();
            }
        }
    }

    /// 複数の解析結果を統合（改良版）
    pub fn combine_results(&mut self, results: Vec<AnalysisResult>) -> AnalysisResult {
        if results.is_empty() {
            return AnalysisResult {
                status: SessionStatus::Idle,
                confidence: 0.0,
                evidence: vec!["No analysis results".to_string()],
                message: None,
                launcher_context: None,
                usage_reset_time: None,
                is_waiting_for_execution: false,
            };
        }

        // 最も信頼度の高い結果を選択
        let best_result = results.iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        // 証拠を統合（重複排除）
        let mut all_evidence: Vec<String> = results.iter()
            .flat_map(|r| r.evidence.iter().cloned())
            .collect();
        all_evidence.dedup();
        
        // 状態遷移のスムージング（急激な変化を抑制）
        let final_status = if let Some(last_state) = &self.last_state {
            if best_result.confidence < 0.7 && *last_state != best_result.status {
                // 信頼度が低い場合は前の状態を維持
                last_state.clone()
            } else {
                best_result.status.clone()
            }
        } else {
            best_result.status.clone()
        };
        
        // 最後の状態を更新
        self.last_state = Some(final_status.clone());

        AnalysisResult {
            status: final_status,
            confidence: best_result.confidence,
            evidence: all_evidence,
            message: best_result.message.clone(),
            launcher_context: Some(self.last_context_before_box.clone()).filter(|s| !s.is_empty()),
            usage_reset_time: self.usage_limit_reset_time.clone(),
            is_waiting_for_execution: self.in_ui_box && self.is_execution_waiting_box(),
        }
    }
    
    /// Parse Claude UI output for box detection and context extraction
    fn parse_claude_ui_output(&mut self, output: &str) {
        // Output is already cleaned by analyze_output method
        let clean_output = output;
        
        // Check for box start using improved pattern
        if clean_output.contains('╭') || clean_output.contains('┌') {
            if !self.in_ui_box {
                // Starting new UI box - capture context before box
                self.last_context_before_box = self.get_recent_context_before_box();
                self.in_ui_box = true;
                self.ui_box_content.clear();
            }
        }
        
        // If we're in a UI box, collect content
        if self.in_ui_box {
            self.ui_box_content.push(clean_output.to_string());
        }
        
        // Check for box end using improved pattern
        if clean_output.contains('╯') || clean_output.contains('└') {
            if self.in_ui_box {
                self.in_ui_box = false;
                // Parse any additional info after box end
                self.parse_post_box_info();
            }
        }
        
        // Check for usage limit reset time with improved detection
        if contains_claude_ui_elements(clean_output) {
            if let Some(usage_info) = self.extract_usage_limit_info(clean_output) {
                self.usage_limit_reset_time = Some(usage_info);
            }
        }
    }
    
    /// Extract usage limit information from text
    fn extract_usage_limit_info(&self, text: &str) -> Option<String> {
        // Look for usage limit patterns in the text
        if text.to_lowercase().contains("usage limit") || text.to_lowercase().contains("approaching") {
            // Extract time information using regex
            let time_pattern = regex::Regex::new(r"(\d{1,2}:\d{2}|\d{1,2}[ap]m)").unwrap();
            if let Some(time_match) = time_pattern.find(text) {
                return Some(time_match.as_str().to_string());
            }
        }
        None
    }
    
    /// Get recent context before the current UI box
    fn get_recent_context_before_box(&self) -> String {
        // Look for the last few lines that might contain launcher context
        let recent_lines: Vec<String> = self.recent_outputs
            .iter()
            .rev()
            .take(5)
            .filter(|line| {
                !line.trim().is_empty() && 
                !line.contains('╭') && !line.contains('┌') &&
                !line.contains('╯') && !line.contains('└')
            })
            .cloned()
            .collect();
        
        // Take the most relevant context (last non-empty line)
        recent_lines.first().cloned().unwrap_or_default()
    }
    
    /// Parse information that appears after box end
    fn parse_post_box_info(&mut self) {
        // Look at the collected UI box content for usage limit info
        let box_content = self.ui_box_content.join(" ");
        if let Some(usage_info) = self.extract_usage_limit_info(&box_content) {
            self.usage_limit_reset_time = Some(usage_info);
        }
    }
    
    /// Check if the current UI box indicates execution waiting
    fn is_execution_waiting_box(&self) -> bool {
        let box_text = self.ui_box_content.join(" ").to_lowercase();
        
        // Check for patterns that indicate waiting for execution
        box_text.contains(">") && (
            box_text.is_empty() || 
            box_text.chars().filter(|c| c.is_alphabetic()).count() < 10
        )
    }
    
    /// Analyze Claude UI state from collected box information
    fn analyze_claude_ui_state(&self) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut confidence = 0.0;
        let mut status = SessionStatus::Idle;
        
        if self.in_ui_box {
            if self.is_execution_waiting_box() {
                status = SessionStatus::WaitingInput;
                confidence = 0.95;
                evidence.push("Claude UI box detected - waiting for execution".to_string());
            } else {
                status = SessionStatus::Busy;
                confidence = 0.8;
                evidence.push("Claude UI box detected - showing content".to_string());
            }
        }
        
        if self.usage_limit_reset_time.is_some() {
            evidence.push("Usage limit reset time detected".to_string());
        }
        
        if !self.last_context_before_box.is_empty() {
            evidence.push("Launcher context captured".to_string());
        }
        
        AnalysisResult {
            status,
            confidence,
            evidence,
            message: None,
            launcher_context: Some(self.last_context_before_box.clone()).filter(|s| !s.is_empty()),
            usage_reset_time: self.usage_limit_reset_time.clone(),
            is_waiting_for_execution: self.in_ui_box && self.is_execution_waiting_box(),
        }
    }

}


impl Default for StandardAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_execution_detection() {
        let mut analyzer = StandardAnalyzer::new();
        let result = analyzer.analyze_output("Using tool: Read file", "stdout");
        
        assert_eq!(result.status, SessionStatus::Busy);
        assert!(result.confidence > 0.7);
        assert!(!result.evidence.is_empty());
    }

    #[test]
    fn test_waiting_input_detection() {
        let mut analyzer = StandardAnalyzer::new();
        let result = analyzer.analyze_output("What would you like me to do next?", "stdout");
        
        assert_eq!(result.status, SessionStatus::WaitingInput);
        assert!(result.confidence > 0.8);
    }

    #[test]
    fn test_error_detection() {
        let mut analyzer = StandardAnalyzer::new();
        let result = analyzer.analyze_output("Error: File not found", "stderr");
        
        assert_eq!(result.status, SessionStatus::Error);
        assert!(result.confidence > 0.8);
    }

    #[test]
    fn test_process_state_analysis() {
        let analyzer = StandardAnalyzer::new();
        
        // 高CPU使用率
        let result = analyzer.analyze_process_state(50.0, 0, false);
        assert_eq!(result.status, SessionStatus::Busy);
        
        // 子プロセス存在
        let result = analyzer.analyze_process_state(5.0, 2, false);
        assert_eq!(result.status, SessionStatus::Busy);
        
        // 低活動
        let result = analyzer.analyze_process_state(0.5, 0, false);
        assert_eq!(result.status, SessionStatus::WaitingInput);
    }

    #[test]
    fn test_result_combination() {
        let mut analyzer = StandardAnalyzer::new();
        
        let results = vec![
            AnalysisResult {
                status: SessionStatus::Busy,
                confidence: 0.6,
                evidence: vec!["Evidence 1".to_string()],
                message: None,
                launcher_context: None,
                usage_reset_time: None,
                is_waiting_for_execution: false,
            },
            AnalysisResult {
                status: SessionStatus::WaitingInput,
                confidence: 0.9,
                evidence: vec!["Evidence 2".to_string()],
                message: None,
                launcher_context: None,
                usage_reset_time: None,
                is_waiting_for_execution: false,
            },
        ];
        
        let combined = analyzer.combine_results(results);
        assert_eq!(combined.status, SessionStatus::WaitingInput);
        assert_eq!(combined.confidence, 0.9);
        assert_eq!(combined.evidence.len(), 2);
    }
    
    #[test]
    fn test_claude_ui_box_detection() {
        let mut analyzer = StandardAnalyzer::new();
        
        // Test box start detection
        let result = analyzer.analyze_output("╭───────────────────────────────────────────────────╮", "stdout");
        assert!(analyzer.in_ui_box);
        
        // Test box content
        analyzer.analyze_output("│ ✻ Welcome to Claude Code!                         │", "stdout");
        analyzer.analyze_output("│                                                   │", "stdout");
        analyzer.analyze_output("│   /help for help, /status for your current setup  │", "stdout");
        
        // Test box end
        analyzer.analyze_output("╰───────────────────────────────────────────────────╯", "stdout");
        assert!(!analyzer.in_ui_box);
    }
    
    #[test]
    fn test_execution_waiting_detection() {
        let mut analyzer = StandardAnalyzer::new();
        
        // Simulate the execution waiting box
        analyzer.parse_claude_ui_output("╭───────────────────────────────────────────────────╮");
        analyzer.parse_claude_ui_output("│ >                                                 │");
        
        // Check that we're in a UI box and it's detected as execution waiting
        assert!(analyzer.in_ui_box);
        assert!(analyzer.is_execution_waiting_box());
        
        analyzer.parse_claude_ui_output("╰───────────────────────────────────────────────────╯");
        
        // After box ends, check final result
        let result = analyzer.analyze_claude_ui_state();
        // The box has ended, so we're no longer in it, but we can check that it was detected
        assert!(!analyzer.in_ui_box);
    }
    
    #[test]
    fn test_usage_limit_parsing() {
        let mut analyzer = StandardAnalyzer::new();
        
        // Test usage limit pattern
        analyzer.parse_claude_ui_output("Approaching usage limit · resets at 12pm");
        assert_eq!(analyzer.usage_limit_reset_time, Some("12pm".to_string()));
        
        // Test with different time format - check what's actually captured
        analyzer.parse_claude_ui_output("Usage limit reached · resets at 2:30pm");
        // The regex captures the time part, let's see what it actually captures
        assert!(analyzer.usage_limit_reset_time.is_some());
        let captured = analyzer.usage_limit_reset_time.as_ref().unwrap();
        assert!(captured.contains("2:30") || captured.contains("2:30pm"));
    }
    
    #[test]
    fn test_ansi_code_stripping() {
        let analyzer = StandardAnalyzer::new();
        
        let input = "\x1b[31m╭───────╮\x1b[0m";
        let cleaned = analyzer.strip_ansi_codes(input);
        assert_eq!(cleaned, "╭───────╮");
    }
    
    #[test]
    fn test_context_extraction() {
        let mut analyzer = StandardAnalyzer::new();
        
        // Add some context lines
        analyzer.add_recent_output("Previous command output".to_string());
        analyzer.add_recent_output("Another line".to_string());
        analyzer.add_recent_output("> log-fileを指定したときに出る内容と、ccmonitorが受け取っている内容は同じもの？".to_string());
        
        // Start a UI box
        analyzer.parse_claude_ui_output("╭───────────────────────────────────────────────────╮");
        
        // Check that context was captured
        assert!(!analyzer.last_context_before_box.is_empty());
        assert!(analyzer.last_context_before_box.contains("log-file"));
    }
}