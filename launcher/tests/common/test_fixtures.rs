// Launcher側テストフィクスチャ
// Note: 統合テスト用共通関数は複数の統合テストファイルから使用されるが、
// Rustコンパイラーは各統合テストを独立してコンパイルするため
// dead_code警告が発生する。実際には使用されているため警告を抑制。

#![cfg(test)]
#![allow(dead_code)]

use climonitor_shared::CliToolType;
use std::path::PathBuf;

/// テスト用のCLIツール引数生成
pub fn create_test_tool_args(tool_type: CliToolType) -> Vec<String> {
    match tool_type {
        CliToolType::Claude => vec!["--project".to_string(), "test-project".to_string()],
        CliToolType::Gemini => vec!["--model".to_string(), "gemini-pro".to_string()],
    }
}

/// テスト用の作業ディレクトリ
pub fn create_test_working_dir() -> PathBuf {
    PathBuf::from("/tmp/test")
}
