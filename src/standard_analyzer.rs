use regex::Regex;
use crate::protocol::SessionStatus;

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
    
    // 状態推定用データ
    last_output_time: Option<chrono::DateTime<chrono::Utc>>,
    recent_outputs: Vec<String>,
    terminal_buffer: String,
    max_recent_outputs: usize,
    last_state: Option<SessionStatus>,
}

/// 解析結果
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub status: SessionStatus,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub message: Option<String>,
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
            
            last_output_time: None,
            recent_outputs: Vec::new(),
            terminal_buffer: String::new(),
            max_recent_outputs: 20,
            last_state: None,
        }
    }

    /// 出力行を解析（ccmanager風のターミナルバッファ解析統合）
    pub fn analyze_output(&mut self, output: &str, stream: &str) -> AnalysisResult {
        let now = chrono::Utc::now();
        self.last_output_time = Some(now);
        
        // ターミナルバッファを更新
        self.update_terminal_buffer(output);
        
        // 最近の出力を記録
        self.add_recent_output(output.to_string());
        
        // 複数解析手法を組み合わせ
        let line_result = self.analyze_single_line(output, stream);
        let buffer_result = self.analyze_terminal_buffer();
        let context_result = self.analyze_context();
        
        // 最適な結果を選択
        self.combine_results(vec![line_result, buffer_result, context_result])
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
        
        // バッファサイズ制限（最新の2000文字を保持）
        if self.terminal_buffer.len() > 2000 {
            let start = self.terminal_buffer.len() - 2000;
            self.terminal_buffer = self.terminal_buffer[start..].to_string();
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
            },
            AnalysisResult {
                status: SessionStatus::WaitingInput,
                confidence: 0.9,
                evidence: vec!["Evidence 2".to_string()],
                message: None,
            },
        ];
        
        let combined = analyzer.combine_results(results);
        assert_eq!(combined.status, SessionStatus::WaitingInput);
        assert_eq!(combined.confidence, 0.9);
        assert_eq!(combined.evidence.len(), 2);
    }
}