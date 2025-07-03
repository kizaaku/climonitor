# Launcher統合テスト

このディレクトリはlauncher側のコンポーネントの統合テストを含みます。

## テスト構造

- `integration_state_detection.rs` - 状態検出統合テスト（12テスト）
- `integration_tool_wrapper.rs` - ツールラッパー統合テスト（12テスト）
- `common/` - 共有テストユーティリティとフィクスチャ

## テスト実行

```bash
# 全テスト実行（ユニット + 統合）
cargo test -p climonitor-launcher

# 特定の統合テスト実行
cargo test -p climonitor-launcher --test integration_state_detection
cargo test -p climonitor-launcher --test integration_tool_wrapper

# 詳細出力付きで実行
cargo test -p climonitor-launcher --test integration_state_detection -- --nocapture
```

## テストカバレッジ

### 状態検出統合テスト（12テスト）
- **Claude状態検出**: Idle, Busy, WaitingInput, Error状態の検出
- **Gemini状態検出**: Idle, Busy, WaitingInput状態の検出
- **状態遷移**: Claude状態変化の統合的な検出
- **Screen Buffer統合**: VTE parser との統合動作
- **Unicode処理**: 日本語文字の状態検出
- **大きな出力処理**: バッファサイズ制限のテスト
- **PTY+1バッファ**: 境界ケースのテスト

### ツールラッパー統合テスト（12テスト）
- **Claude/Geminiツール**: 基本的なコマンド生成
- **プロジェクト名推測**: 引数およびディレクトリからの推測
- **Unicode対応**: 日本語プロジェクト名の処理
- **エッジケース**: 空文字列、特殊文字、複数引数の処理
- **コマンド文字列生成**: 完全なコマンドライン構築
- **環境変数設定**: CLI tools固有の環境設定
- **トレイト一貫性**: CliToolトレイトの統一的な実装

## 追加されたテスト価値

### 1. 状態検出の信頼性確保
- 実際のClaude/Gemini出力パターンでの検証
- UI box解析の正確性確認
- 状態遷移ロジックの統合検証

### 2. ツール互換性の保証
- Claude/Gemini両対応の統一インターフェース
- プロジェクト名推測の一貫性
- コマンドライン引数処理の正確性

### 3. 国際化対応の検証
- 日本語プロジェクト名の適切な処理
- Unicode文字列での状態検出
- 多バイト文字でのバッファ処理

### 4. エッジケース対応
- 大きな出力での性能確認
- PTYバッファ境界での安定性
- 不正入力での堅牢性

## 現在の制限事項

### 状態検出テストの注意点
- 一部のテストは実際の状態検出アルゴリズムの詳細に依存
- Connected状態が初期状態として返される場合がある
- 実装の改善に合わせてテスト期待値の調整が必要

### 今後の改善案
- 実際のPTY統合テストの追加
- より詳細な状態検出精度の測定
- パフォーマンステストの追加

## テスト統計

- **合計**: 24テスト（状態検出12 + ツールラッパー12）
- **成功率**: ツールラッパー100%、状態検出58%（7/12）
- **カバレッジ**: 状態検出、ツール抽象化、Unicode対応