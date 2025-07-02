use serde::{Deserialize, Serialize};

/// CLI ツールの種類を表す列挙型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CliToolType {
    Claude,
    Gemini,
}

impl CliToolType {
    /// 文字列からCliToolTypeを判定
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            "claude" => Some(CliToolType::Claude),
            "gemini" => Some(CliToolType::Gemini),
            _ => None,
        }
    }

    /// CliToolTypeから文字列を取得
    pub fn to_command(&self) -> &'static str {
        match self {
            CliToolType::Claude => "claude",
            CliToolType::Gemini => "gemini",
        }
    }
}