# 状態検出器詳細仕様

## 概要

climonitorは各CLIツール専用の状態検出器を使用して、PTY画面出力から正確な実行状態を判定します。独立型アーキテクチャにより、各検出器は完全に独立して動作し、ツール固有のパターンに最適化されています。

## StateDetector Trait

すべての状態検出器が実装する共通インターフェース：

```rust
trait StateDetector {
    /// PTY出力を処理して状態変化を検出
    fn process_output(&mut self, output: &str) -> Option<SessionStatus>;
    
    /// 現在の状態を取得
    fn current_state(&self) -> &SessionStatus;
    
    /// デバッグ用画面バッファ表示
    fn debug_buffer(&self);
    
    /// 実行コンテキスト取得（● や ✦ マーカーの内容）
    fn get_ui_above_text(&self) -> Option<String>;
    
    /// 画面バッファサイズ変更
    fn resize_screen_buffer(&mut self, rows: usize, cols: usize);
}
```

## SessionStatus 列挙型

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SessionStatus {
    Connected,     // 🔗 PTYセッション開始・接続中
    Idle,          // 🔵 入力待ち・完了状態・アイドル
    Busy,          // 🔵 処理実行中
    WaitingInput,  // ⏳ ユーザー確認待ち
    Completed,     // ✅ セッション完了
    Error,         // 🔴 エラー状態
}
```

## Claude状態検出器 (ScreenClaudeStateDetector)

### 構造

```rust
pub struct ScreenClaudeStateDetector {
    screen_buffer: ScreenBuffer,              // VTEパーサー統合バッファ
    current_state: SessionStatus,             // 現在の状態
    previous_had_esc_interrupt: bool,         // 前回の"esc to interrupt"状態
    last_state_change: Option<Instant>,       // 状態変更時刻
    verbose: bool,                           // デバッグ出力フラグ
}
```

### 主要検出ロジック

#### 1. "esc to interrupt" パターン検出

Claude固有の実行状態指標：

```rust
fn detect_claude_completion_state(&mut self) -> Option<SessionStatus> {
    let has_esc_interrupt = screen_lines
        .iter()
        .any(|line| line.contains("esc to interrupt"));
    
    if self.previous_had_esc_interrupt && !has_esc_interrupt {
        // "esc to interrupt" が消えた = 実行完了
        return Some(SessionStatus::Idle);
    } else if !self.previous_had_esc_interrupt && has_esc_interrupt {
        // "esc to interrupt" が現れた = 実行開始
        return Some(SessionStatus::Busy);
    }
}
```

#### 2. UI Box パターン検出

UI boxからの状態判定：

```rust
// 承認プロンプト検出
if content_line.contains("Do you want")
    || content_line.contains("Would you like")
    || content_line.contains("May I")
    || content_line.contains("proceed?")
    || content_line.contains("y/n")
{
    return Some(SessionStatus::WaitingInput);
}

// IDE接続確認
if below_line.contains("◯ IDE connected") {
    return Some(SessionStatus::Idle);
}
```

#### 3. 実行コンテキスト抽出

```rust
fn get_ui_above_text(&self) -> Option<String> {
    let screen_lines = self.screen_buffer.get_screen_lines();
    
    // 画面全体から行頭●マーカーを探す（逆順で最新のものを取得）
    for line in screen_lines.iter().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('●') {
            let right_text = trimmed['●'.len_utf8()..].trim();
            if !right_text.is_empty() {
                return Some(right_text.to_string());
            }
        }
    }
    None
}
```

### 検出パターン一覧

| パターン | 状態 | 説明 |
|----------|------|------|
| `"esc to interrupt"` 出現 | Busy | Claude実行開始 |
| `"esc to interrupt"` 消失 | Idle | Claude実行完了 |
| `"Do you want"`, `"proceed?"` | WaitingInput | ユーザー確認待ち |
| `"◯ IDE connected"` | Idle | IDE接続完了 |
| `"●"` で始まる行 | - | 実行コンテキスト |

## Gemini状態検出器 (ScreenGeminiStateDetector)

### 構造

```rust
pub struct ScreenGeminiStateDetector {
    screen_buffer: ScreenBuffer,
    current_state: SessionStatus,
    verbose: bool,
}
```

### 主要検出ロジック

#### 1. 単一行パターンチェック

```rust
fn check_single_line_patterns(&self, line: &str) -> Option<SessionStatus> {
    let trimmed = line.trim();
    
    // 入力待ち状態（最優先）
    if line.contains("Waiting for user confirmation") {
        return Some(SessionStatus::WaitingInput);
    }
    
    // 実行中状態
    if line.contains("(esc to cancel") {
        return Some(SessionStatus::Busy);
    }
    
    None
}
```

#### 2. UI Box + 周辺行検出

```rust
fn detect_gemini_state(&mut self) -> Option<SessionStatus> {
    // 全ての画面内容から状態パターンをチェック
    if let Some(state) = self.check_screen_patterns(&screen_lines) {
        return Some(state);
    }
    
    // UI boxがある場合は、各UI boxとその上下の行をチェック
    for ui_box in &ui_boxes {
        for line in &ui_box.above_lines {
            if let Some(state) = self.check_single_line_patterns(line) {
                return Some(state);
            }
        }
    }
    
    // 特別な状態が検出されない場合はIdle
    Some(SessionStatus::Idle)
}
```

#### 3. 実行コンテキスト抽出

```rust
fn get_ui_above_text(&self) -> Option<String> {
    let screen_lines = self.screen_buffer.get_screen_lines();
    
    // 画面全体から行頭✦マーカーを探す（逆順で最新のものを取得）
    for line in screen_lines.iter().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('✦') {
            let right_text = trimmed['✦'.len_utf8()..].trim();
            if !right_text.is_empty() {
                return Some(right_text.to_string());
            }
        }
    }
    None
}
```

### 検出パターン一覧

| パターン | 状態 | 説明 |
|----------|------|------|
| `"(esc to cancel"` | Busy | Gemini処理中 |
| `"Waiting for user confirmation"` | WaitingInput | ユーザー確認待ち |
| `">"` で始まる行 | Idle | コマンドプロンプト |
| `"✦"` で始まる行 | - | 実行コンテキスト |

## ScreenBuffer統合

### VTEパーサー機能

両方の検出器は共通の`ScreenBuffer`を使用：

```rust
pub struct ScreenBuffer {
    grid: Vec<Vec<Cell>>,           // 画面グリッド
    cursor_row: usize,              // カーソル位置
    cursor_col: usize,
    rows: usize,                    // PTYサイズ
    cols: usize,
    verbose: bool,
}
```

### 主要機能

1. **ANSI Escape Sequence処理**
   - CSI sequences (色、カーソル移動、消去)
   - 文字出力とカーソル管理
   - スクロール処理

2. **UI Box検出**
   ```rust
   pub fn find_ui_boxes(&self) -> Vec<UiBox> {
       // ╭╮╰╯ パターンでUI boxを検出
       // 内容行と上下の行を抽出
   }
   ```

3. **PTY+1列バッファ**
   - 内部バッファ: PTY列数+1
   - 表示出力: 元のPTYサイズ
   - UIボックス重複問題を解決

## 状態遷移図

### Claude状態遷移

```
Connected → Idle ←→ Busy → Idle
    ↓         ↑      ↓
    └─────→ WaitingInput
              ↓
            Idle/Error
```

### Gemini状態遷移

```
Connected → Idle ←→ Busy → Idle
    ↓         ↑      ↓
    └─────→ WaitingInput
              ↓
            Idle/Error
```

## パフォーマンス考慮

### 最適化ポイント

1. **逆順検索**: 最新の実行コンテキストを効率的に取得
2. **パターンマッチング**: 正規表現より高速な文字列contains
3. **状態キャッシュ**: 不要な状態変化を防ぐ
4. **画面バッファ制限**: メモリ使用量を制限

### デバッグサポート

Verboseモード時の詳細ログ：

```rust
if self.verbose {
    eprintln!("🔍 [CLAUDE_STATE] esc_interrupt: {} → {}", 
              self.previous_had_esc_interrupt, has_esc_interrupt);
    eprintln!("🎯 [STATE_CHANGE] {:?} → {:?}", 
              old_state, new_state);
}
```

## 実装のベストプラクティス

### 1. Unicode安全性
```rust
// 文字境界を考慮した文字列スライス
let right_text = trimmed['●'.len_utf8()..].trim();
```

### 2. エラーハンドリング
```rust
// パターンマッチ失敗時の安全なフォールバック
if let Some(state) = detect_pattern() {
    return Some(state);
}
// デフォルト状態を返す
Some(SessionStatus::Idle)
```

### 3. 状態一貫性
```rust
// 状態変化時のみ更新
if new_state != self.current_state {
    self.current_state = new_state.clone();
    return Some(new_state);
}
```

---

この状態検出器アーキテクチャにより、climonitorは各CLIツールの細かな状態変化を正確に追跡し、リアルタイムで監視できます。