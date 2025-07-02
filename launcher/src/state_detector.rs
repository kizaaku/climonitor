// state_detector.rs - 状態検出の抽象化レイヤー

use climonitor_shared::SessionStatus;

/// 状態検出器の共通インターフェース
pub trait StateDetector: Send + Sync {
    /// 新しい出力を処理して状態を更新
    fn process_output(&mut self, output: &str) -> Option<SessionStatus>;

    /// 現在の状態を取得
    fn current_state(&self) -> &SessionStatus;

    /// デバッグ用：現在のバッファを表示
    fn debug_buffer(&self);

    /// UI box上の⏺文字以降のテキストを取得
    fn get_ui_above_text(&self) -> Option<String>;

    /// ターミナルサイズ変更時のscreen buffer再初期化
    fn resize_screen_buffer(&mut self, rows: usize, cols: usize);
}

/// 状態検出器のファクトリー
use crate::cli_tool::CliToolType;

pub fn create_state_detector(tool_type: CliToolType, verbose: bool) -> Box<dyn StateDetector> {
    match tool_type {
        CliToolType::Claude => {
            Box::new(crate::screen_claude_detector::ScreenClaudeStateDetector::new(verbose))
        }
        CliToolType::Gemini => {
            Box::new(crate::screen_gemini_detector::ScreenGeminiStateDetector::new(verbose))
        }
    }
}
