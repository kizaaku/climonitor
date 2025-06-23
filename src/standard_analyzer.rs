use regex::Regex;
use crate::protocol::SessionStatus;

/// 非DEBUGモード対応の標準出力解析
pub struct StandardAnalyzer {
    // 出力パターン（正規表現）
    tool_execution_pattern: Regex,
    waiting_input_pattern: Regex,
    completion_pattern: Regex,
    error_pattern: Regex,
    api_request_pattern: Regex,
    
    // 状態推定用データ
    last_output_time: Option<chrono::DateTime<chrono::Utc>>,
    recent_outputs: Vec<String>,
    max_recent_outputs: usize,
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
            // Claude標準出力パターン
            tool_execution_pattern: Regex::new(r"(?i)(using tool|executing|running|calling)").unwrap(),
            waiting_input_pattern: Regex::new(r"(?i)(what would you like|press enter|continue|what's next|\?|\.\.\.)$").unwrap(),
            completion_pattern: Regex::new(r"(?i)(finished|done|completed|success|✅|✓)").unwrap(),
            error_pattern: Regex::new(r"(?i)(error|failed|exception|❌|✗|cannot|unable)").unwrap(),
            api_request_pattern: Regex::new(r"(?i)(thinking|processing|analyzing|generating)").unwrap(),
            
            last_output_time: None,
            recent_outputs: Vec::new(),
            max_recent_outputs: 10,
        }
    }

    /// 出力行を解析
    pub fn analyze_output(&mut self, output: &str, stream: &str) -> AnalysisResult {
        let now = chrono::Utc::now();
        self.last_output_time = Some(now);
        
        // 最近の出力を記録
        self.add_recent_output(output.to_string());
        
        // パターンマッチング
        let mut evidence = Vec::new();
        let mut confidence = 0.0;
        let mut status = SessionStatus::Active;

        // エラーパターン（最優先）
        if self.error_pattern.is_match(output) {
            status = SessionStatus::Error;
            confidence = 0.9;
            evidence.push(format!("Error pattern detected in {}", stream));
        }
        // ツール実行パターン
        else if self.tool_execution_pattern.is_match(output) {
            status = SessionStatus::Active;
            confidence = 0.8;
            evidence.push(format!("Tool execution pattern detected in {}", stream));
        }
        // 入力待ちパターン
        else if self.waiting_input_pattern.is_match(output) {
            status = SessionStatus::Approve;
            confidence = 0.9;
            evidence.push(format!("Input waiting pattern detected in {}", stream));
        }
        // 完了パターン
        else if self.completion_pattern.is_match(output) {
            status = SessionStatus::Finish;
            confidence = 0.7;
            evidence.push(format!("Completion pattern detected in {}", stream));
        }
        // API処理パターン
        else if self.api_request_pattern.is_match(output) {
            status = SessionStatus::Active;
            confidence = 0.6;
            evidence.push(format!("API processing pattern detected in {}", stream));
        }
        // 文脈による推測
        else {
            let contextual_result = self.analyze_context();
            status = contextual_result.status;
            confidence = contextual_result.confidence;
            evidence.extend(contextual_result.evidence);
        }

        AnalysisResult {
            status,
            confidence,
            evidence,
            message: Some(output.to_string()),
        }
    }

    /// プロセス状態から解析
    pub fn analyze_process_state(
        &self,
        cpu_percent: f32,
        child_count: u32,
        network_active: bool,
    ) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut confidence = 0.0;
        let mut status = SessionStatus::Idle;

        // 高CPU使用率 = アクティブ
        if cpu_percent > 10.0 {
            status = SessionStatus::Active;
            confidence = 0.7;
            evidence.push(format!("High CPU usage: {:.1}%", cpu_percent));
        }

        // 子プロセス存在 = ツール実行中
        if child_count > 0 {
            status = SessionStatus::Active;
            confidence = 0.8;
            evidence.push(format!("Child processes: {}", child_count));
        }

        // ネットワーク活動 = API通信中
        if network_active {
            status = SessionStatus::Active;
            confidence = 0.6;
            evidence.push("Network activity detected".to_string());
        }

        // 低活動 = 入力待ちまたはアイドル
        if cpu_percent < 1.0 && child_count == 0 && !network_active {
            status = SessionStatus::Approve; // 入力待ちと推測
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

    /// 文脈解析（最近の出力から推測）
    fn analyze_context(&self) -> AnalysisResult {
        let mut evidence = Vec::new();
        let mut confidence = 0.3; // 低信頼度
        let mut status = SessionStatus::Active;

        // 最近の出力がない = アイドル
        if let Some(last_time) = self.last_output_time {
            let silence_duration = chrono::Utc::now() - last_time;
            
            if silence_duration.num_minutes() > 5 {
                status = SessionStatus::Idle;
                confidence = 0.9;
                evidence.push(format!("No output for {} minutes", silence_duration.num_minutes()));
            } else if silence_duration.num_seconds() > 30 {
                status = SessionStatus::Approve;
                confidence = 0.6;
                evidence.push(format!("No output for {} seconds", silence_duration.num_seconds()));
            }
        }

        // 最近の出力パターン分析
        let recent_text = self.recent_outputs.join(" ");
        if recent_text.contains("?") || recent_text.ends_with("...") {
            status = SessionStatus::Approve;
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

    /// 複数の解析結果を統合
    pub fn combine_results(&self, results: Vec<AnalysisResult>) -> AnalysisResult {
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

        // 証拠を統合
        let all_evidence: Vec<String> = results.iter()
            .flat_map(|r| r.evidence.iter().cloned())
            .collect();

        AnalysisResult {
            status: best_result.status.clone(),
            confidence: best_result.confidence,
            evidence: all_evidence,
            message: best_result.message.clone(),
        }
    }

    /// 統計情報取得
    pub fn get_stats(&self) -> AnalyzerStats {
        AnalyzerStats {
            recent_outputs_count: self.recent_outputs.len(),
            last_output_time: self.last_output_time,
        }
    }
}

/// 解析統計
#[derive(Debug, Clone)]
pub struct AnalyzerStats {
    pub recent_outputs_count: usize,
    pub last_output_time: Option<chrono::DateTime<chrono::Utc>>,
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
        
        assert_eq!(result.status, SessionStatus::Active);
        assert!(result.confidence > 0.7);
        assert!(!result.evidence.is_empty());
    }

    #[test]
    fn test_waiting_input_detection() {
        let mut analyzer = StandardAnalyzer::new();
        let result = analyzer.analyze_output("What would you like me to do next?", "stdout");
        
        assert_eq!(result.status, SessionStatus::Approve);
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
        assert_eq!(result.status, SessionStatus::Active);
        
        // 子プロセス存在
        let result = analyzer.analyze_process_state(5.0, 2, false);
        assert_eq!(result.status, SessionStatus::Active);
        
        // 低活動
        let result = analyzer.analyze_process_state(0.5, 0, false);
        assert_eq!(result.status, SessionStatus::Approve);
    }

    #[test]
    fn test_result_combination() {
        let analyzer = StandardAnalyzer::new();
        
        let results = vec![
            AnalysisResult {
                status: SessionStatus::Active,
                confidence: 0.6,
                evidence: vec!["Evidence 1".to_string()],
                message: None,
            },
            AnalysisResult {
                status: SessionStatus::Approve,
                confidence: 0.9,
                evidence: vec!["Evidence 2".to_string()],
                message: None,
            },
        ];
        
        let combined = analyzer.combine_results(results);
        assert_eq!(combined.status, SessionStatus::Approve);
        assert_eq!(combined.confidence, 0.9);
        assert_eq!(combined.evidence.len(), 2);
    }
}