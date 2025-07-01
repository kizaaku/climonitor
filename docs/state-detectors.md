# 状態検出器詳細仕様

## 概要

climonitorは各CLIツール専用の状態検出器を使用して、画面出力から正確な実行状態を判定します。Complete Independence アーキテクチャにより、各検出器は完全に独立して動作します。

## StateDetector Trait

すべての状態検出器が実装する共通インターフェース：

```rust
trait StateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState>;
    fn current_state(&self) -> &SessionState;
    fn to_session_status(&self) -> SessionStatus;
    fn debug_buffer(&self);
    fn get_ui_execution_context(&self) -> Option<String>;
    fn get_ui_above_text(&self) -> Option<String>;
    fn resize_screen_buffer(&mut self, rows: usize, cols: usize);
}
```

## SessionState 列挙型

```rust
enum SessionState {
    Connected,      // 🔗 PTYセッション開始
    Idle,          // 🔵 入力待ち・完了状態
    Busy,          // 🔵 処理実行中
    WaitingForInput, // ⏳ ユーザー確認待ち
    Error,         // 🔴 エラー状態
}
```

## Claude状態検出器 (ScreenClaudeStateDetector)

### 構造

```rust
struct ScreenClaudeStateDetector {
    screen_buffer: ScreenBuffer,              // VTEパーサー統合バッファ
    current_state: SessionState,              // 現在の状態
    previous_had_esc_interrupt: bool,         // 前回の"esc to interrupt"状態
    last_state_change: Option<Instant>,       // 状態変更時刻
    verbose: bool,                           // デバッグ出力フラグ
}
```

### 主要検出ロジック

#### 1. "esc to interrupt" パターン検出

**検出対象**: `"esc to interrupt"` 文字列の出現・消失

**ロジック**:
```rust
fn detect_claude_completion_state(&mut self) -> Option<SessionState> {
    let screen_lines = self.screen_buffer.get_screen_lines();
    let has_esc_interrupt = screen_lines.iter()
        .any(|line| line.contains("esc to interrupt"));
    
    // 出現: Idle → Busy
    if !self.previous_had_esc_interrupt && has_esc_interrupt {
        return Some(SessionState::Busy);
    }
    
    // 消失: Busy → Idle (完了)
    if self.previous_had_esc_interrupt && !has_esc_interrupt {
        return Some(SessionState::Idle);
    }
}
```

#### 2. UI Box内容での確認プロンプト検出

**検出パターン**:
- `"Do you want"`
- `"Would you like"`
- `"May I"`
- `"proceed?"`
- `"y/n"`

**ロジック**: UI box内のcontent_linesをスキャンして該当パターンを検索

#### 3. IDE接続状態検出

**検出パターン**: `"◯ IDE connected"`

**ロジック**: UI boxのbelow_linesで検出した場合、Idle状態に遷移

### Claude特有の特徴

- **高精度完了検出**: `"esc to interrupt"`の消失による確実な完了判定
- **実行コンテキスト**: `"⏺ 実行中"`表示の解析
- **状態遷移追跡**: 前回状態との比較による正確な変化検出

## Gemini状態検出器 (ScreenGeminiStateDetector)

### 構造

```rust
struct ScreenGeminiStateDetector {
    screen_buffer: ScreenBuffer,              // VTEパーサー統合バッファ
    current_state: SessionState,              // 現在の状態
    last_state_change: Option<Instant>,       // 状態変更時刻
    verbose: bool,                           // デバッグ出力フラグ
}
```

### 主要検出ロジック

#### 1. プロンプト準備完了検出

**検出対象**: UI box内で`>`から始まる行

**ロジック**:
```rust
if trimmed.starts_with('>') {
    return Some(SessionState::Idle);
}
```

**判定**: コマンドプロンプトが表示されている = アイドル状態

#### 2. 処理中状態検出

**検出パターン**: `"(esc to cancel"` (UI boxなし状態)

**ロジック**:
```rust
// UI boxがない場合の画面全体スキャン
for line in &screen_lines {
    if trimmed.contains("(esc to cancel") {
        return Some(SessionState::Busy);
    }
}
```

#### 3. 確認プロンプト検出

**UI box内パターン**:
- `"Allow execution?"`

**UI box下パターン**:
- `"waiting for user confirmation"`

**ロジック**: 両方の位置での検出をサポート

#### 4. 統計表示検出 (セッション終了)

**検出パターン**: 
- `"Cumulative Stats"`
- `"Input Tokens"`

**判定**: セッション完了後の統計情報表示 = アイドル状態

### Gemini特有の特徴

- **UI状態の多様性**: UI boxあり/なしの両方に対応
- **視覚的プロンプト**: `>`記号による明確なアイドル判定
- **統計情報活用**: セッション終了の確実な検出

## VTE Parser統合 (ScreenBuffer)

### PTY+1列バッファアーキテクチャ

**問題**: ink.jsライブラリのUI box重複描画

**解決策**:
```rust
let buffer_cols = cols + 1;  // PTY cols + 1
let grid = vec![vec![Cell::empty(); buffer_cols]; rows];

// 表示は元のPTYサイズに制限
let pty_cols = self.cols.saturating_sub(1);
```

### UI Box検出アルゴリズム

**検出対象**: Unicode罫線文字 `╭╮╰╯`

**アルゴリズム**:
```rust
fn find_ui_boxes(&self) -> Vec<UIBox> {
    // 1. 上辺 (╭ ╮) の検出
    // 2. 下辺 (╰ ╯) の検出  
    // 3. 矩形範囲の確定
    // 4. 内容・上下行の抽出
}
```

**抽出データ**:
- `content_lines`: UI box内のテキスト
- `above_lines`: UI box上部の行 (実行コンテキスト)
- `below_lines`: UI box下部の行 (ステータス情報)

## 共通エラー検出

両検出器で共通のエラーパターン：

**検出パターン**:
- `"✗"`
- `"failed"`  
- `"Error"`

**判定**: いずれかが検出された場合、Error状態に遷移

## デバッグ出力

### Claude検出器

```
🔍 [CLAUDE_STATE] esc_interrupt: false → true, current: Idle
🚀 [CLAUDE_START] 'esc to interrupt' appeared → Busy
✅ [CLAUDE_COMPLETION] 'esc to interrupt' disappeared → Completing
```

### Gemini検出器

```
✅ [GEMINI_READY] Command prompt ready: > 
⏳ [GEMINI_INPUT] Waiting for input: Allow execution?
⚡ [GEMINI_BUSY] Processing detected: ⠋ I'm Feeling Lucky (esc to cancel, 0s)
📊 [GEMINI_STATS] Stats displayed, session idle
```

## 状態遷移図

### Claude
```
Connected → Idle ⇄ Busy → Idle
    ↓         ↓
WaitingForInput ← → Error
```

### Gemini  
```
Connected → Idle ⇄ Busy → Idle
    ↓         ↓      ↓
WaitingForInput ← → Error
```

## パフォーマンス考慮事項

### 効率的なパターン検索

- 必要最小限の文字列検索
- 早期リターンによる無駄な処理の回避
- UI box検出の優先順位付け

### メモリ使用量

- 画面バッファサイズ: 80x24 + 1列 = 1,944文字
- UI box情報の一時保存のみ
- 不要なログ出力の制限

## 今後の拡張

### 新しいツール追加時の考慮事項

1. **専用検出器実装**: `ScreenXXXStateDetector`
2. **ツール固有パターン**: 独自のUI要素・メッセージ
3. **ファクトリー登録**: `state_detector.rs`への追加
4. **テストケース**: 実際の使用パターンでの検証