// claude_tool.rs - Claude固有のツール実装

use crate::cli_tool::CliTool;
use anyhow::Result;
use portable_pty::CommandBuilder;
use std::path::Path;

/// Claude固有のツール実装
pub struct ClaudeTool;

impl ClaudeTool {
    pub fn new() -> Self {
        Self
    }
}

impl CliTool for ClaudeTool {
    fn command_name(&self) -> &str {
        "claude"
    }

    fn setup_environment(&self, cmd: &mut CommandBuilder) {
        // Claude Code用の環境変数設定
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("FORCE_COLOR", "1");

        // TTY環境であることを明示
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            cmd.env("TERM_PROGRAM", term_program);
        }

        // デバッグログ設定（必要に応じて）
        // cmd.env("ANTHROPIC_LOG", "debug");
    }

    fn guess_project_name(&self, args: &[String], working_dir: &Path) -> Option<String> {
        // --project 引数から取得を試行
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

    fn validate_args(&self, args: &[String]) -> Result<()> {
        // Claude固有の引数検証（現在は特に制限なし）
        Ok(())
    }
}

impl Default for ClaudeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_claude_tool_command_name() {
        let tool = ClaudeTool::new();
        assert_eq!(tool.command_name(), "claude");
    }

    #[test]
    fn test_claude_tool_project_name_from_args() {
        let tool = ClaudeTool::new();
        let args = vec!["--project".to_string(), "test-project".to_string()];
        let working_dir = PathBuf::from("/tmp");
        
        let result = tool.guess_project_name(&args, &working_dir);
        assert_eq!(result, Some("test-project".to_string()));
    }

    #[test]
    fn test_claude_tool_project_name_from_dir() {
        let tool = ClaudeTool::new();
        let args = vec![];
        let working_dir = PathBuf::from("/home/user/my-project");
        
        let result = tool.guess_project_name(&args, &working_dir);
        assert_eq!(result, Some("my-project".to_string()));
    }

    #[test]
    fn test_claude_tool_validate_args() {
        let tool = ClaudeTool::new();
        let args = vec!["--help".to_string()];
        
        assert!(tool.validate_args(&args).is_ok());
    }
}