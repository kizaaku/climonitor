use ccmonitor::session_state::{SessionStateDetector, SessionState};

#[test]
fn test_empty_and_whitespace_input() {
    let mut detector = SessionStateDetector::new(false);
    
    // 空文字列やホワイトスペースのみの入力
    assert_eq!(detector.process_output(""), None);
    assert_eq!(detector.process_output("   "), None);
    assert_eq!(detector.process_output("\n\n\n"), None);
    assert_eq!(detector.process_output("\t\t\t"), None);
    
    // 状態は初期状態のまま
    assert_eq!(detector.current_state(), &SessionState::Connected);
}

#[test]
fn test_very_long_lines() {
    let mut detector = SessionStateDetector::new(false);
    
    // 非常に長い行の処理
    let long_line = "a".repeat(10000) + " ✅ completed";
    assert_eq!(detector.process_output(&long_line), Some(SessionState::Idle));
    
    // 長いエラーメッセージ
    let long_error = "Error: ".to_string() + &"x".repeat(5000);
    assert_eq!(detector.process_output(&long_error), Some(SessionState::Error));
}

#[test]
fn test_mixed_content_lines() {
    let mut detector = SessionStateDetector::new(false);
    
    // 複数のパターンが混在する行
    let mixed_lines = vec![
        ("🔧 Processing... but also ✅ some part completed", Some(SessionState::Busy)), // 最初にマッチしたパターンが優先
        ("Error occurred but continuing to process...", Some(SessionState::Error)),
        ("Ready to proceed? Also finished some tasks ✅", Some(SessionState::WaitingForInput)),
    ];
    
    for (line, expected) in mixed_lines {
        detector = SessionStateDetector::new(false); // リセット
        let result = detector.process_output(line);
        assert_eq!(result, expected, "Failed for mixed content: {}", line);
    }
}

#[test]
fn test_case_sensitivity() {
    let mut detector = SessionStateDetector::new(false);
    
    // 大文字小文字の違い
    let case_variations = vec![
        ("ERROR: Something went wrong", Some(SessionState::Error)),
        ("error: Something went wrong", Some(SessionState::Error)),
        ("Error: Something went wrong", Some(SessionState::Error)),
        ("PROCESSING data...", Some(SessionState::Busy)),
        ("Processing data...", Some(SessionState::Busy)),
        ("COMPLETED successfully", Some(SessionState::Idle)),
        ("completed successfully", Some(SessionState::Idle)),
    ];
    
    for (output, expected) in case_variations {
        detector = SessionStateDetector::new(false); // リセット
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for case variation: {}", output);
    }
}

#[test]
fn test_partial_pattern_matches() {
    let mut detector = SessionStateDetector::new(false);
    
    // 部分的なパターンマッチ（false positiveを避ける）
    let partial_matches = vec![
        ("This is not an error message", None),
        ("I'm not processing anything", None),
        ("The processor is running", None), // "process"を含むが"processing"ではない
        ("Error message: everything is ok", Some(SessionState::Error)), // "error"があるので検出
        ("Processing request now", Some(SessionState::Busy)),
    ];
    
    for (output, expected) in partial_matches {
        detector = SessionStateDetector::new(false); // リセット
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for partial match: {}", output);
    }
}

#[test]
fn test_concurrent_state_patterns() {
    let mut detector = SessionStateDetector::new(false);
    
    // 同じ行に複数の状態パターンが存在する場合
    let concurrent_patterns = vec![
        ("🔧 Processing... Error: failed ✅ completed", Some(SessionState::Busy)), // 最初の判定が優先
        ("✅ Success! But also error occurred", Some(SessionState::Idle)),
        ("Error: failed, now processing retry", Some(SessionState::Error)),
    ];
    
    for (output, expected) in concurrent_patterns {
        detector = SessionStateDetector::new(false); // リセット  
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for concurrent patterns: {}", output);
    }
}

#[test]
fn test_buffer_edge_cases() {
    let mut detector = SessionStateDetector::new(false);
    
    // ちょうど30行のバッファ境界をテスト
    for i in 0..29 {
        detector.process_output(&format!("Line {}", i));
    }
    
    // 30行目でパターンマッチ
    assert_eq!(detector.process_output("✅ completed at line 30"), Some(SessionState::Idle));
    
    // 31行目で古い行が削除される
    detector.process_output("Line 31");
    
    // まだ検出可能
    assert_eq!(detector.process_output("🔧 processing again"), Some(SessionState::Busy));
}

#[test]
fn test_special_characters() {
    let mut detector = SessionStateDetector::new(false);
    
    // 特殊文字を含むパターン
    let special_chars = vec![
        ("Error: file@path/name.txt not found", Some(SessionState::Error)),
        ("Processing $HOME/.config/app.conf", Some(SessionState::Busy)),
        ("✅ Success with special chars: !@#$%^&*()", Some(SessionState::Idle)),
        ("Proceed with [y/N]?", Some(SessionState::WaitingForInput)),
        ("Running command: curl -X POST https://api.example.com", Some(SessionState::Busy)),
    ];
    
    for (output, expected) in special_chars {
        detector = SessionStateDetector::new(false); // リセット
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for special chars: {}", output);
    }
}

#[test]
fn test_performance_with_large_buffer() {
    let mut detector = SessionStateDetector::new(false);
    
    let start = std::time::Instant::now();
    
    // 大量の出力を高速処理
    for i in 0..1000 {
        detector.process_output(&format!("Processing item {} of 1000", i));
    }
    
    // 最終的な状態検出
    detector.process_output("✅ All 1000 items processed successfully");
    
    let duration = start.elapsed();
    
    // パフォーマンステスト：1000行の処理が100ms以内
    assert!(duration.as_millis() < 100, "Performance test failed: took {}ms", duration.as_millis());
    assert_eq!(detector.current_state(), &SessionState::Idle);
}

#[test]
fn test_ansi_edge_cases() {
    let mut detector = SessionStateDetector::new(false);
    
    // 複雑なANSIエスケープシーケンス
    let ansi_cases = vec![
        ("\x1b[1;32m✅ Bold green success\x1b[0m", Some(SessionState::Idle)),
        ("\x1b[38;5;196m❌ 256-color error\x1b[0m", Some(SessionState::Error)), 
        ("\x1b[2J\x1b[H🔧 Clear screen and processing\x1b[0m", Some(SessionState::Busy)),
        ("\x1b[?25l\x1b[31mHidden cursor error\x1b[?25h\x1b[0m", Some(SessionState::Error)),
    ];
    
    for (output, expected) in ansi_cases {
        detector = SessionStateDetector::new(false); // リセット
        let result = detector.process_output(output);
        assert_eq!(result, expected, "Failed for ANSI case: {:?}", output);
    }
}

#[test]
fn test_state_transition_consistency() {
    let mut detector = SessionStateDetector::new(false);
    
    // 状態遷移の一貫性をテスト
    let state_sequence = vec![
        (SessionState::Connected, "Initial state"),
        (SessionState::Idle, "Ready >"),
        (SessionState::Busy, "🔧 Working..."),
        (SessionState::WaitingForInput, "Continue? (y/n)"),
        (SessionState::Busy, "🔧 Continuing..."),
        (SessionState::Error, "❌ Failed"),
        (SessionState::Busy, "🔧 Retrying..."),
        (SessionState::Idle, "✅ Success"),
    ];
    
    for (expected_state, output) in state_sequence {
        if let Some(_) = detector.process_output(output) {
            assert_eq!(detector.current_state(), &expected_state, "State transition failed for: {}", output);
        }
    }
}