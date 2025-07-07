// Connected状態からBusy状態のみに遷移できることをテストする

use climonitor_launcher::screen_claude_detector::ScreenClaudeStateDetector;
use climonitor_launcher::state_detector::StateDetector;
use climonitor_shared::SessionStatus;

#[test]
fn test_connected_state_maintained_without_execution() {
    let mut detector = ScreenClaudeStateDetector::new(false);

    // 初期状態はConnected
    assert_eq!(*detector.current_state(), SessionStatus::Connected);

    // Claude起動後の通常のUI表示（"esc to interrupt"なし）
    let idle_ui_sequence = [
        // UIボックス表示
        "\x1b[2m\x1b[38;2;136;136;136m╭─────────────────────────────────────────╮\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m│\x1b[22m\x1b[38;2;153;153;153m > \x1b[39m\x1b[7m \x1b[27m                                        \x1b[2m\x1b[38;2;136;136;136m│\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m╰─────────────────────────────────────────╯\x1b[39m\x1b[22m\r\n",
    ];

    for line in idle_ui_sequence.iter() {
        let result = detector.process_output(line);
        // 状態変化が発生しないことを確認
        assert!(
            result.is_none(),
            "Should not change state from Connected without 'esc to interrupt'"
        );
    }

    // Connected状態が維持されることを確認
    assert_eq!(
        *detector.current_state(),
        SessionStatus::Connected,
        "Should maintain Connected state when no execution occurs"
    );
}

#[test]
fn test_connected_to_busy_transition() {
    let mut detector = ScreenClaudeStateDetector::new(false);

    // 初期状態はConnected
    assert_eq!(*detector.current_state(), SessionStatus::Connected);

    // "esc to interrupt"が表示される = 実行開始
    let busy_sequence = [
        // 実行中表示
        "\x1b[38;2;215;119;87m·\x1b[39m \x1b[38;2;215;119;87mThinking… \x1b[38;2;153;153;153m(1s · \x1b[1mesc \x1b[22mto interrupt)\x1b[39m\r\n",
        // UIボックス
        "\x1b[2m\x1b[38;2;136;136;136m╭─────────────────────────────────────────╮\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m│\x1b[22m\x1b[38;2;153;153;153m > \x1b[39m\x1b[7m \x1b[27m                                        \x1b[2m\x1b[38;2;136;136;136m│\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m╰─────────────────────────────────────────╯\x1b[39m\x1b[22m\r\n",
    ];

    let mut state_changed = false;
    for line in busy_sequence.iter() {
        if let Some(new_state) = detector.process_output(line) {
            assert_eq!(
                new_state,
                SessionStatus::Busy,
                "Should transition to Busy when 'esc to interrupt' appears"
            );
            state_changed = true;
        }
    }

    assert!(state_changed, "State should change from Connected to Busy");
    assert_eq!(*detector.current_state(), SessionStatus::Busy);
}

#[test]
fn test_busy_to_idle_transition() {
    let mut detector = ScreenClaudeStateDetector::new(true); // verboseモード

    // Connected→Busyの遷移を正しく行う
    let busy_sequence = [
        // "esc to interrupt"を含む行
        "\x1b[38;2;215;119;87m·\x1b[39m \x1b[38;2;215;119;87mThinking… \x1b[38;2;153;153;153m(1s · \x1b[1mesc \x1b[22mto interrupt)\x1b[39m\r\n",
        // UIボックス（above_lines扱い）
        "\x1b[2m\x1b[38;2;136;136;136m╭─────────────────────────────────────────╮\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m│\x1b[22m\x1b[38;2;153;153;153m > \x1b[39m\x1b[7m \x1b[27m                                        \x1b[2m\x1b[38;2;136;136;136m│\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m╰─────────────────────────────────────────╯\x1b[39m\x1b[22m\r\n",
    ];

    // Busy状態に遷移させる
    for line in busy_sequence.iter() {
        detector.process_output(line);
    }

    println!(
        "Current state after busy sequence: {:?}",
        detector.current_state()
    );

    // Busy状態になっていることを確認
    assert_eq!(*detector.current_state(), SessionStatus::Busy);

    // "esc to interrupt"が消える = 実行完了
    let idle_sequence = [
        // 実行完了後のUI（"esc to interrupt"なし）
        "\x1b[2m\x1b[38;2;136;136;136m╭─────────────────────────────────────────╮\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m│\x1b[22m\x1b[38;2;153;153;153m > \x1b[39m\x1b[7m \x1b[27m                                        \x1b[2m\x1b[38;2;136;136;136m│\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m╰─────────────────────────────────────────╯\x1b[39m\x1b[22m\r\n",
    ];

    let mut state_changed = false;
    for line in idle_sequence.iter() {
        if let Some(new_state) = detector.process_output(line) {
            assert_eq!(
                new_state,
                SessionStatus::Idle,
                "Should transition to Idle when 'esc to interrupt' disappears"
            );
            state_changed = true;
        }
    }

    assert!(state_changed, "State should change from Busy to Idle");
    assert_eq!(*detector.current_state(), SessionStatus::Idle);
}

#[test]
fn test_no_direct_connected_to_idle_transition() {
    let mut detector = ScreenClaudeStateDetector::new(false);

    // 初期状態はConnected
    assert_eq!(*detector.current_state(), SessionStatus::Connected);

    // Idleになりそうな状況を再現（UIボックスはあるが"esc to interrupt"なし）
    let sequence = [
        "\x1b[2m\x1b[38;2;136;136;136m╭─────────────────────────────────────────╮\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m│\x1b[22m\x1b[38;2;153;153;153m > Ready for input\x1b[39m                  \x1b[2m\x1b[38;2;136;136;136m│\x1b[39m\x1b[22m\r\n",
        "\x1b[2m\x1b[38;2;136;136;136m╰─────────────────────────────────────────╯\x1b[39m\x1b[22m\r\n",
    ];

    for line in sequence.iter() {
        let result = detector.process_output(line);
        // Connected→Idleの直接遷移は発生しない
        assert!(
            result.is_none() || result != Some(SessionStatus::Idle),
            "Should not transition directly from Connected to Idle"
        );
    }

    // Connected状態が維持される
    assert_eq!(
        *detector.current_state(),
        SessionStatus::Connected,
        "Should maintain Connected state, not transition to Idle"
    );
}
