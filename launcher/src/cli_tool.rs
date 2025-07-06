// cli_tool.rs - CLI ツール共通インターフェース

use portable_pty::{CommandBuilder, PtySize};
use std::path::Path;
use terminal_size::{terminal_size, Height, Width};

/// CLI ツールの共通インターフェース
pub trait CliTool: Send + Sync {
    /// ツールのコマンド名を取得
    fn command_name(&self) -> &str;

    /// ツール固有の環境変数を設定
    fn setup_environment(&self, cmd: &mut CommandBuilder);

    /// プロジェクト名を推測
    fn guess_project_name(&self, args: &[String], working_dir: &Path) -> Option<String>;

    /// ツール固有のコマンド文字列生成
    fn to_command_string(&self, args: &[String]) -> String {
        let mut parts = vec![self.command_name().to_string()];
        parts.extend(args.iter().cloned());
        parts.join(" ")
    }
}

// CliToolTypeはsharedライブラリから使用
pub use climonitor_shared::CliToolType;

/// CLI ツールのファクトリー
pub struct CliToolFactory;

impl CliToolFactory {
    /// ツールタイプに基づいてCliToolを作成
    pub fn create_tool(tool_type: CliToolType) -> Box<dyn CliTool> {
        match tool_type {
            CliToolType::Claude => Box::new(crate::claude_tool::ClaudeTool::new()),
            CliToolType::Gemini => Box::new(crate::gemini_tool::GeminiTool::new()),
        }
    }
}

/// PTY サイズの設定
pub fn get_pty_size() -> PtySize {
    // ターミナルサイズを取得、失敗時は80x24をデフォルトとする
    match terminal_size() {
        Some((Width(cols), Height(rows))) => PtySize {
            rows, // 通常のPTYサイズを使用（ink互換性のため）
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        None => PtySize {
            rows: 24, // 通常のデフォルトサイズ
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    }
}

/// 共通のPTY設定を行う
pub fn setup_common_pty_environment(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("FORCE_COLOR", "1");
}

/// Windows用PTY設定を改善する関数
#[cfg(windows)]
pub fn create_optimized_pty_system() -> Box<dyn portable_pty::PtySystem + Send> {
    // Windows ConPTYの設定を最適化
    // portable-pty 0.9.0では内部的にPSEUDOCONSOLE_INHERIT_CURSORが使用されている可能性があります
    portable_pty::native_pty_system()
}

#[cfg(not(windows))]
pub fn create_optimized_pty_system() -> Box<dyn portable_pty::PtySystem + Send> {
    portable_pty::native_pty_system()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_tool_type_from_command() {
        assert_eq!(
            CliToolType::from_command("claude"),
            Some(CliToolType::Claude)
        );
        assert_eq!(
            CliToolType::from_command("gemini"),
            Some(CliToolType::Gemini)
        );
        assert_eq!(CliToolType::from_command("unknown"), None);
    }

    #[test]
    fn test_cli_tool_type_to_command() {
        assert_eq!(CliToolType::Claude.to_command(), "claude");
        assert_eq!(CliToolType::Gemini.to_command(), "gemini");
    }

    #[test]
    fn test_get_pty_size() {
        let size = get_pty_size();
        assert!(size.rows > 0);
        assert!(size.cols > 0);
    }
}
