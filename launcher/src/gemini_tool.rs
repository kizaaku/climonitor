// gemini_tool.rs - Gemini固有のツール実装

use crate::cli_tool::CliTool;
use anyhow::Result;
use portable_pty::CommandBuilder;
use std::path::Path;

/// Gemini固有のツール実装
pub struct GeminiTool;

impl GeminiTool {
    pub fn new() -> Self {
        Self
    }
}

impl CliTool for GeminiTool {
    fn command_name(&self) -> &str {
        "gemini"
    }

    fn setup_environment(&self, cmd: &mut CommandBuilder) {
        // Gemini CLI用の環境変数設定
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("FORCE_COLOR", "1");

        // TTY環境であることを明示
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            cmd.env("TERM_PROGRAM", term_program);
        }

        // Gemini固有の環境変数があれば追加
        // 例: cmd.env("GEMINI_LOG", "debug");
    }

    fn guess_project_name(&self, args: &[String], working_dir: &Path) -> Option<String> {
        // Gemini固有のプロジェクト名推測ロジック
        // 現在はClaude同様の実装を使用
        
        // --project 引数から取得を試行（Geminiが対応している場合）
        if let Some(project_idx) = args.iter().position(|arg| arg == "--project") {
            if let Some(project_name) = args.get(project_idx + 1) {
                return Some(project_name.clone());
            }
        }

        // 作業ディレクトリ名から推測
        if let Some(dir_name) = working_dir.file_name() {
            if let Some(name_str) = dir_name.to_str() {
                return Some(name_str.to_string());
            }
        }

        // 現在のディレクトリ名から推測
        if let Ok(current_dir) = std::env::current_dir() {
            if let Some(dir_name) = current_dir.file_name() {
                if let Some(name_str) = dir_name.to_str() {
                    return Some(name_str.to_string());
                }
            }
        }

        None
    }

    fn validate_args(&self, _args: &[String]) -> Result<()> {
        // Gemini固有の引数検証（必要に応じて実装）
        // 現在は特に制限なし
        Ok(())
    }
}

impl Default for GeminiTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_gemini_tool_command_name() {
        let tool = GeminiTool::new();
        assert_eq!(tool.command_name(), "gemini");
    }

    #[test]
    fn test_gemini_tool_project_name_from_args() {
        let tool = GeminiTool::new();
        let args = vec!["--project".to_string(), "test-project".to_string()];
        let working_dir = PathBuf::from("/tmp");
        
        let result = tool.guess_project_name(&args, &working_dir);
        assert_eq!(result, Some("test-project".to_string()));
    }

    #[test]
    fn test_gemini_tool_project_name_from_dir() {
        let tool = GeminiTool::new();
        let args = vec![];
        let working_dir = PathBuf::from("/home/user/my-project");
        
        let result = tool.guess_project_name(&args, &working_dir);
        assert_eq!(result, Some("my-project".to_string()));
    }

    #[test]
    fn test_gemini_tool_validate_args() {
        let tool = GeminiTool::new();
        let args = vec!["--help".to_string()];
        
        assert!(tool.validate_args(&args).is_ok());
    }
}