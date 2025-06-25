use ccmonitor::session_state::{SessionStateDetector, SessionState};

/// 実際のClaude Codeの出力パターンを模擬したテスト
#[test]
fn test_real_claude_patterns() {
    let mut detector = SessionStateDetector::new(false);
    
    // Claude Code の実際の起動メッセージ風
    let startup_outputs = vec![
        "\x1b[2J\x1b[H", // 画面クリア
        "Claude Code v1.0.0",
        "Initializing session...",
        "\x1b[32m✓\x1b[0m Ready",
        "claude> ", // プロンプト
    ];
    
    for output in startup_outputs {
        detector.process_output(output);
    }
    
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_tool_execution_simulation() {
    let mut detector = SessionStateDetector::new(false);
    
    // ツール実行の模擬
    let tool_execution = vec![
        ("user input: create a new file", None),
        ("\x1b[33m🔧 Tool: Write\x1b[0m", Some(SessionState::Busy)),
        ("Creating file: example.py", None),
        ("Writing content...", None),
        ("\x1b[32m✅ File created successfully\x1b[0m", Some(SessionState::Idle)),
        ("claude> ", None),
    ];
    
    for (output, expected) in tool_execution {
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for: {}", output);
    }
}

#[test]
fn test_error_handling_simulation() {
    let mut detector = SessionStateDetector::new(false);
    
    // エラーハンドリングの模擬
    let error_sequence = vec![
        ("🔧 Running bash command: rm /protected/file", Some(SessionState::Busy)),
        ("rm: /protected/file: Permission denied", None),
        ("\x1b[31m❌ Error: Command failed\x1b[0m", Some(SessionState::Error)),
        ("exit code: 1", None),
        ("Would you like me to try with sudo? (y/n)", Some(SessionState::WaitingForInput)),
        ("user: y", None),
        ("🔧 Running: sudo rm /protected/file", Some(SessionState::Busy)),
        ("\x1b[32m✅ File removed successfully\x1b[0m", Some(SessionState::Idle)),
    ];
    
    for (output, expected) in error_sequence {
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for: {}", output);
    }
}

#[test]
fn test_interactive_session_simulation() {
    let mut detector = SessionStateDetector::new(false);
    
    // インタラクティブセッションの模擬
    let interactive_outputs = vec![
        "How can I help you today?",
        "user: Help me debug this Python script",
        "🔧 Reading script...",
        "📝 Analyzing code...",
        "I found a few issues. Let me show you:",
        "1. Missing import statement",
        "2. Undefined variable on line 15", 
        "3. Syntax error on line 23",
        "Would you like me to fix these? (y/n)",
        "user: yes", 
        "🔧 Applying fixes...",
        "✅ Fixed missing import",
        "✅ Fixed undefined variable", 
        "✅ Fixed syntax error",
        "✅ All fixes applied successfully!",
        "The script should now run without errors.",
        "claude> ",
    ];
    
    let mut last_state = SessionState::Connected;
    for output in interactive_outputs {
        if let Some(new_state) = detector.process_output(output) {
            last_state = new_state;
        }
    }
    
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test] 
fn test_rapid_state_changes() {
    let mut detector = SessionStateDetector::new(false);
    
    // 短時間での状態変化
    let rapid_changes = vec![
        ("🔧 Starting...", Some(SessionState::Busy)),
        ("❌ Failed", Some(SessionState::Error)),
        ("🔧 Retrying...", Some(SessionState::Busy)),
        ("✅ Success", Some(SessionState::Idle)),
        ("Confirm next step? (y/n)", Some(SessionState::WaitingForInput)),
        ("🔧 Processing...", Some(SessionState::Busy)),
        ("✅ Done", Some(SessionState::Idle)),
    ];
    
    for (output, expected) in rapid_changes {
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed rapid change for: {}", output);
    }
}

#[test]
fn test_noisy_output_filtering() {
    let mut detector = SessionStateDetector::new(false);
    
    // ノイズの多い出力での状態検出
    let noisy_output = r#"
\x1b[2K\x1b[1G
Loading...
...
Connecting to API...
.....................
\x1b[32m🔧 Executing query\x1b[0m
[DEBUG] HTTP request sent
[DEBUG] Response received: 200 OK
[INFO] Processing data...
[DEBUG] Parsing JSON response
[INFO] Found 42 results
[DEBUG] Filtering results...
\x1b[32m✅ Query completed successfully\x1b[0m
Results saved to output.json
claude> 
"#;
    
    detector.process_output(noisy_output);
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_unicode_and_emoji_handling() {
    let mut detector = SessionStateDetector::new(false);
    
    // Unicode文字と絵文字を含む出力
    let unicode_outputs = vec![
        ("📊 データを分析中...", Some(SessionState::Busy)),
        ("🔍 ファイルを検索しています", None),
        ("🌟 処理が完了しました ✨", Some(SessionState::Idle)),
        ("日本語のファイル名.txt を作成しますか？ (y/n)", Some(SessionState::WaitingForInput)),
        ("🚀 ファイル作成中...", Some(SessionState::Busy)),
        ("✅ 正常に作成されました 🎉", Some(SessionState::Idle)),
    ];
    
    for (output, expected) in unicode_outputs {
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for Unicode output: {}", output);
    }
}

#[test]
fn test_shell_prompt_detection() {
    let mut detector = SessionStateDetector::new(false);
    
    // 様々なシェルプロンプトパターン
    let prompts = vec![
        ("user@hostname:~/project$ ", Some(SessionState::Idle)),
        ("claude> ", Some(SessionState::Idle)), 
        (">>> ", Some(SessionState::Idle)),
        ("(venv) user@host:~/app$ ", Some(SessionState::Idle)),
        ("PS C:\\Users\\user> ", Some(SessionState::Idle)),
    ];
    
    for (prompt, expected) in prompts {
        // 最初に別の状態にしてからプロンプトをテスト
        detector.process_output("🔧 Working...");
        let result = detector.process_output(prompt);
        assert_eq!(result, expected, "Failed for prompt: {}", prompt);
    }
}