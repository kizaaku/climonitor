// ツールラッパーの統合テスト

#[cfg(test)]
mod common;

use climonitor_launcher::claude_tool::ClaudeTool;
use climonitor_launcher::cli_tool::CliTool;
use climonitor_launcher::gemini_tool::GeminiTool;
use climonitor_shared::CliToolType;
use common::*;

#[test]
fn test_claude_tool_command_generation() {
    // Claude ツールのコマンド生成テスト
    let claude_tool = ClaudeTool::new();

    // 基本コマンド
    let command = claude_tool.command_name();
    assert_eq!(command, "claude");

    // プロジェクト名の抽出テスト（引数から）
    let args = vec!["--project".to_string(), "test-project".to_string()];
    let test_dir = create_test_working_dir();
    let project_name = claude_tool.guess_project_name(&args, &test_dir);
    assert_eq!(project_name, Some("test-project".to_string()));

    // ディレクトリからのプロジェクト名抽出
    let no_args: Vec<String> = vec![];
    let dir_project_name = claude_tool.guess_project_name(&no_args, &test_dir);
    assert_eq!(dir_project_name, Some("test".to_string()));
}

#[test]
fn test_gemini_tool_command_generation() {
    // Gemini ツールのコマンド生成テスト
    let gemini_tool = GeminiTool::new();

    // 基本コマンド
    let command = gemini_tool.command_name();
    assert_eq!(command, "gemini");

    // プロジェクト名の抽出テスト（Geminiは通常--projectを使わない）
    let args = vec!["--model".to_string(), "gemini-pro".to_string()];
    let test_dir = create_test_working_dir();
    let project_name = gemini_tool.guess_project_name(&args, &test_dir);
    // 引数からは取得できないので、ディレクトリ名が使われる
    assert_eq!(project_name, Some("test".to_string()));
}

#[test]
fn test_tool_args_handling() {
    // ツール引数の処理テスト
    let claude_args = create_test_tool_args(CliToolType::Claude);
    assert!(!claude_args.is_empty());
    assert!(claude_args.contains(&"--project".to_string()));
    assert!(claude_args.contains(&"test-project".to_string()));

    let gemini_args = create_test_tool_args(CliToolType::Gemini);
    assert!(!gemini_args.is_empty());
    assert!(gemini_args.contains(&"--model".to_string()));
    assert!(gemini_args.contains(&"gemini-pro".to_string()));
}

#[test]
fn test_unicode_project_names() {
    // Unicode（日本語）プロジェクト名の処理テスト
    let claude_tool = ClaudeTool::new();

    let japanese_args = vec!["--project".to_string(), "日本語プロジェクト".to_string()];
    let test_dir = create_test_working_dir();

    let project_name = claude_tool.guess_project_name(&japanese_args, &test_dir);
    assert_eq!(project_name, Some("日本語プロジェクト".to_string()));
}

#[test]
fn test_edge_case_project_names() {
    // エッジケースのプロジェクト名テスト
    let claude_tool = ClaudeTool::new();
    let test_dir = create_test_working_dir();

    // 空のプロジェクト名
    let empty_args = vec!["--project".to_string(), "".to_string()];
    let empty_project = claude_tool.guess_project_name(&empty_args, &test_dir);
    assert_eq!(empty_project, Some("".to_string()));

    // スペースを含むプロジェクト名
    let space_args = vec!["--project".to_string(), "project with spaces".to_string()];
    let space_project = claude_tool.guess_project_name(&space_args, &test_dir);
    assert_eq!(space_project, Some("project with spaces".to_string()));

    // 特殊文字を含むプロジェクト名
    let special_args = vec![
        "--project".to_string(),
        "project-with_special.chars".to_string(),
    ];
    let special_project = claude_tool.guess_project_name(&special_args, &test_dir);
    assert_eq!(
        special_project,
        Some("project-with_special.chars".to_string())
    );
}

#[test]
fn test_missing_project_args() {
    // プロジェクト引数がない場合のテスト
    let claude_tool = ClaudeTool::new();
    let test_dir = create_test_working_dir();

    // --project 引数がない場合（ディレクトリ名が使われる）
    let no_project_args = vec!["--help".to_string()];
    let no_project = claude_tool.guess_project_name(&no_project_args, &test_dir);
    assert_eq!(no_project, Some("test".to_string()));

    // --project はあるが値がない場合
    let incomplete_args = vec!["--project".to_string()];
    let incomplete_project = claude_tool.guess_project_name(&incomplete_args, &test_dir);
    // 値がない場合はディレクトリ名が使われる
    assert_eq!(incomplete_project, Some("test".to_string()));
}

#[test]
fn test_multiple_project_args() {
    // 複数のプロジェクト引数がある場合のテスト（最初のものが優先される）
    let claude_tool = ClaudeTool::new();
    let test_dir = create_test_working_dir();

    let multiple_args = vec![
        "--project".to_string(),
        "first-project".to_string(),
        "--project".to_string(),
        "second-project".to_string(),
    ];

    let project_name = claude_tool.guess_project_name(&multiple_args, &test_dir);
    // 実装では最初の--projectが優先される
    assert_eq!(project_name, Some("first-project".to_string()));
}

#[test]
fn test_directory_based_project_names() {
    // ディレクトリベースのプロジェクト名テスト
    let claude_tool = ClaudeTool::new();
    let no_args: Vec<String> = vec![];

    // 現在のディレクトリ
    let current_dir = std::env::current_dir().unwrap();
    let current_project = claude_tool.guess_project_name(&no_args, &current_dir);
    assert!(current_project.is_some());

    // ルートディレクトリ
    let root_dir = std::path::PathBuf::from("/");
    let root_project = claude_tool.guess_project_name(&no_args, &root_dir);
    // ルートディレクトリの場合、現在のディレクトリ名が使われる
    assert!(root_project.is_some());

    // 存在しないディレクトリ
    let nonexistent_dir = std::path::PathBuf::from("/nonexistent/directory");
    let nonexistent_project = claude_tool.guess_project_name(&no_args, &nonexistent_dir);
    assert_eq!(nonexistent_project, Some("directory".to_string()));
}

#[test]
fn test_command_string_generation() {
    // コマンド文字列生成のテスト
    let claude_tool = ClaudeTool::new();

    // 引数なし
    let no_args: Vec<String> = vec![];
    let cmd_str = claude_tool.to_command_string(&no_args);
    assert_eq!(cmd_str, "claude");

    // 引数あり
    let with_args = vec!["--project".to_string(), "test".to_string()];
    let cmd_str_with_args = claude_tool.to_command_string(&with_args);
    assert_eq!(cmd_str_with_args, "claude --project test");
}

#[test]
fn test_environment_setup() {
    // 環境変数設定のテスト（実際の設定は困難なので、関数が呼べることを確認）
    use portable_pty::CommandBuilder;

    let claude_tool = ClaudeTool::new();
    let mut cmd = CommandBuilder::new("echo");

    // setup_environment が呼べることを確認（実際の環境変数設定は検証困難）
    claude_tool.setup_environment(&mut cmd);

    // エラーなく完了すればOK - 環境変数設定の副作用は検証困難
}

#[test]
fn test_gemini_tool_specifics() {
    // Gemini固有の機能テスト
    let gemini_tool = GeminiTool::new();

    // コマンド名
    assert_eq!(gemini_tool.command_name(), "gemini");

    // コマンド文字列生成
    let args = vec!["--model".to_string(), "gemini-pro".to_string()];
    let cmd_str = gemini_tool.to_command_string(&args);
    assert_eq!(cmd_str, "gemini --model gemini-pro");

    // プロジェクト名推測（ディレクトリベース）
    let test_dir = create_test_working_dir();
    let project = gemini_tool.guess_project_name(&args, &test_dir);
    assert_eq!(project, Some("test".to_string()));
}

#[test]
fn test_tool_trait_consistency() {
    // 両ツールがCliToolトレイトを正しく実装していることを確認
    let claude_tool: Box<dyn CliTool> = Box::new(ClaudeTool::new());
    let gemini_tool: Box<dyn CliTool> = Box::new(GeminiTool::new());

    // 基本メソッドが呼べることを確認
    assert_eq!(claude_tool.command_name(), "claude");
    assert_eq!(gemini_tool.command_name(), "gemini");

    let test_dir = create_test_working_dir();
    let no_args: Vec<String> = vec![];

    let claude_project = claude_tool.guess_project_name(&no_args, &test_dir);
    let gemini_project = gemini_tool.guess_project_name(&no_args, &test_dir);

    // 両方ともプロジェクト名を推測できることを確認
    assert!(claude_project.is_some());
    assert!(gemini_project.is_some());
    assert_eq!(claude_project, gemini_project); // 同じディレクトリなので同じ結果
}
