// state_detector.rs - 状態検出の抽象化レイヤー

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

    /// UI実行コンテキストを取得（数文字表示用）
    fn get_ui_execution_context(&self) -> Option<String>;
}

/// 状態検出器のファクトリー
use crate::cli_tool::CliToolType;

pub fn create_state_detector(tool_type: CliToolType, verbose: bool) -> Box<dyn StateDetector> {
    match tool_type {
        CliToolType::Claude => Box::new(crate::screen_claude_detector::ScreenClaudeStateDetector::new(
            verbose,
        )),
        CliToolType::Gemini => Box::new(crate::gemini_state_detector::GeminiStateDetector::new(
            verbose,
        )),
    }
}