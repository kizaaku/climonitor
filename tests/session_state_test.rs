use ccmonitor::session_state::{SessionStateDetector, SessionState};

#[test]
fn test_claude_startup_sequence() {
    let mut detector = SessionStateDetector::new(false);
    
    // Claude 起動時の出力
    assert_eq!(detector.process_output("Welcome to Claude Code!"), None);
    assert_eq!(detector.current_state(), &SessionState::Connected);
    
    // プロンプト表示（アイドル状態）
    assert_eq!(
        detector.process_output("Ready for your next task >"), 
        Some(SessionState::Idle)
    );
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_tool_execution_states() {
    let mut detector = SessionStateDetector::new(false);
    
    // ツール実行開始（ビジー状態）
    assert_eq!(
        detector.process_output("🔧 Executing bash command..."), 
        Some(SessionState::Busy)
    );
    assert_eq!(detector.current_state(), &SessionState::Busy);
    
    // 実行完了（アイドル状態）
    assert_eq!(
        detector.process_output("✅ Command completed successfully"), 
        Some(SessionState::Idle)
    );
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_user_input_waiting() {
    let mut detector = SessionStateDetector::new(false);
    
    // ユーザー確認待ち
    assert_eq!(
        detector.process_output("Do you want to proceed? (y/n)"), 
        Some(SessionState::WaitingForInput)
    );
    assert_eq!(detector.current_state(), &SessionState::WaitingForInput);
    
    // 確認後処理継続
    assert_eq!(
        detector.process_output("Processing your request..."), 
        Some(SessionState::Busy)
    );
    assert_eq!(detector.current_state(), &SessionState::Busy);
}

#[test]
fn test_error_detection() {
    let mut detector = SessionStateDetector::new(false);
    
    // エラー発生
    assert_eq!(
        detector.process_output("❌ Error: Command failed with exit code 1"), 
        Some(SessionState::Error)
    );
    assert_eq!(detector.current_state(), &SessionState::Error);
    
    // エラーから回復
    assert_eq!(
        detector.process_output("Trying alternative approach..."), 
        Some(SessionState::Busy)
    );
    assert_eq!(detector.current_state(), &SessionState::Busy);
}

#[test]
fn test_ansi_escape_sequence_removal() {
    let mut detector = SessionStateDetector::new(false);
    
    // ANSI エスケープシーケンス付きの出力
    let ansi_output = "\x1b[32m✅ Task completed\x1b[0m successfully";
    assert_eq!(
        detector.process_output(ansi_output), 
        Some(SessionState::Idle)
    );
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_realistic_claude_session() {
    let mut detector = SessionStateDetector::new(false);
    
    // リアルなClaude Codeセッション
    let outputs = vec![
        ("Claude Code is starting...", None),
        ("Ready to help! What can I do for you?", Some(SessionState::Idle)),
        ("🔧 Reading file contents...", Some(SessionState::Busy)),
        ("📝 Analyzing code structure...", None), // 状態変化なし（既にBusy）
        ("✅ Analysis complete", Some(SessionState::Idle)),
        ("Would you like me to proceed with the changes? (y/n)", Some(SessionState::WaitingForInput)),
        ("🔧 Applying changes...", Some(SessionState::Busy)),
        ("Error: Permission denied", Some(SessionState::Error)),
        ("Retrying with sudo...", Some(SessionState::Busy)),
        ("✅ Changes applied successfully", Some(SessionState::Idle)),
        ("Ready for next task >", None), // 状態変化なし（既にIdle）
    ];
    
    for (output, expected_state_change) in outputs {
        let result = detector.process_output(output);
        assert_eq!(result, expected_state_change, "Failed for output: '{}'", output);
    }
    
    // 最終状態の確認
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_buffer_size_limit() {
    let mut detector = SessionStateDetector::new(false);
    
    // 30行以上の出力を送信してバッファサイズ制限をテスト
    for i in 0..50 {
        detector.process_output(&format!("Line {}: Some output", i));
    }
    
    // 最新の出力で状態検出が正常に動作することを確認
    assert_eq!(
        detector.process_output("✅ Final task completed"), 
        Some(SessionState::Idle)
    );
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_complex_multiline_output() {
    let mut detector = SessionStateDetector::new(false);
    
    let multiline_output = r#"
Running multiple commands:
  Command 1: ls -la
  Command 2: git status
🔧 Executing commands...
total 64
drwxr-xr-x  12 user  staff   384 Jan 15 10:30 .
drwxr-xr-x   4 user  staff   128 Jan 15 10:00 ..
-rw-r--r--   1 user  staff  1234 Jan 15 10:30 README.md

On branch main
Your branch is up to date with 'origin/main'.
✅ All commands completed successfully
"#;
    
    // 複数行出力の処理
    detector.process_output(multiline_output);
    
    // 最終的にアイドル状態になっていることを確認
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_state_persistence() {
    let mut detector = SessionStateDetector::new(false);
    
    // ビジー状態に移行
    detector.process_output("🔧 Processing...");
    assert_eq!(detector.current_state(), &SessionState::Busy);
    
    // 関係ない出力では状態変化しない
    assert_eq!(detector.process_output("Some random output"), None);
    assert_eq!(detector.process_output("Another line"), None);
    assert_eq!(detector.process_output("Still processing data"), None);
    
    // まだビジー状態を維持
    assert_eq!(detector.current_state(), &SessionState::Busy);
    
    // 明確な完了メッセージでアイドルに戻る
    assert_eq!(
        detector.process_output("✅ Processing finished"), 
        Some(SessionState::Idle)
    );
    assert_eq!(detector.current_state(), &SessionState::Idle);
}