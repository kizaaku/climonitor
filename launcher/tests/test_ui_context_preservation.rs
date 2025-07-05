// test_ui_context_preservation.rs - UIコンテキスト保持機能のテスト

use climonitor_launcher::state_detector::create_state_detector;
use climonitor_shared::CliToolType;

#[test]
fn test_claude_ui_context_preservation() {
    let mut detector = create_state_detector(CliToolType::Claude, false);

    // 1. 初期状態ではコンテキストはNone
    assert_eq!(detector.get_ui_above_text(), None);

    // 2. コンテキスト付きの出力を処理
    let output_with_context = "● ファイルを読み込んでいます...\n";
    detector.process_output(output_with_context);
    
    // コンテキストが取得できることを確認
    assert_eq!(
        detector.get_ui_above_text(),
        Some("ファイルを読み込んでいます...".to_string())
    );

    // 3. コンテキストのない出力を処理（画面がクリアされた状態をシミュレート）
    let clear_output = "\x1b[2J\x1b[H"; // 画面クリア + カーソル移動
    detector.process_output(clear_output);

    // 4. コンテキストが保持されているかテスト
    let preserved_context = detector.get_ui_above_text();
    assert_eq!(
        preserved_context,
        Some("ファイルを読み込んでいます...".to_string()),
        "UIコンテキストが保持されていません"
    );

    // 5. 新しいコンテキストが出現したら更新される
    let new_context_output = "● 新しいタスクを実行中...\n";
    detector.process_output(new_context_output);
    
    assert_eq!(
        detector.get_ui_above_text(),
        Some("新しいタスクを実行中...".to_string())
    );
}

#[test]
fn test_gemini_ui_context_preservation() {
    let mut detector = create_state_detector(CliToolType::Gemini, false);

    // 1. 初期状態ではコンテキストはNone
    assert_eq!(detector.get_ui_above_text(), None);

    // 2. Gemini固有のコンテキスト付き出力を処理
    let output_with_context = "✦ コードを生成しています...\n";
    detector.process_output(output_with_context);
    
    // コンテキストが取得できることを確認
    assert_eq!(
        detector.get_ui_above_text(),
        Some("コードを生成しています...".to_string())
    );

    // 3. 画面スクロールをシミュレート（複数行の出力でコンテキストが画面外に）
    let scroll_output = "\n".repeat(50); // 50行の改行
    detector.process_output(&scroll_output);

    // 4. コンテキストが保持されているかテスト
    let preserved_context = detector.get_ui_above_text();
    assert_eq!(
        preserved_context,
        Some("コードを生成しています...".to_string()),
        "Gemini UIコンテキストが保持されていません"
    );
}

#[test]
fn test_context_preservation_with_multiple_updates() {
    let mut detector = create_state_detector(CliToolType::Claude, false);

    // 最初のコンテキスト
    detector.process_output("● 初期タスク実行中...\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("初期タスク実行中...".to_string())
    );

    // コンテキストのない出力（保持されるはず）
    detector.process_output("通常の出力テキスト\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("初期タスク実行中...".to_string())
    );

    // 新しいコンテキスト（更新されるはず）
    detector.process_output("● 更新されたタスク...\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("更新されたタスク...".to_string())
    );

    // 再びコンテキストのない出力（新しいコンテキストが保持されるはず）
    detector.process_output("別の通常出力\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("更新されたタスク...".to_string())
    );
}

#[test]
fn test_empty_context_handling() {
    let mut detector = create_state_detector(CliToolType::Claude, false);

    // 空のマーカー（テキストなし）
    detector.process_output("●\n");
    // 空のコンテキストは無視され、Noneが保持される
    assert_eq!(detector.get_ui_above_text(), None);

    // 有効なコンテキストを設定
    detector.process_output("● 有効なコンテキスト\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("有効なコンテキスト".to_string())
    );

    // 再び空のマーカー（既存のコンテキストが保持されるはず）
    detector.process_output("●\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("有効なコンテキスト".to_string())
    );
}

#[test]
fn test_context_preservation_during_state_transitions() {
    let mut detector = create_state_detector(CliToolType::Claude, false);

    // コンテキスト設定
    detector.process_output("● ファイル処理中...\n");
    assert_eq!(
        detector.get_ui_above_text(),
        Some("ファイル処理中...".to_string())
    );

    // 状態変化を伴う出力（例：実行中状態）
    detector.process_output("esc to interrupt\n");
    
    // 状態が変わってもコンテキストは保持される
    assert_eq!(
        detector.get_ui_above_text(),
        Some("ファイル処理中...".to_string())
    );

    // 状態が戻る
    detector.process_output("プロセス完了\n");
    
    // まだコンテキストは保持されている
    assert_eq!(
        detector.get_ui_above_text(),
        Some("ファイル処理中...".to_string())
    );
}