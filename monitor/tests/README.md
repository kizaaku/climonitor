# 統合テスト

このディレクトリはlauncherとmonitorコンポーネント間の相互作用をテストする統合テストを含みます。

## テスト構造

- `integration_protocol_basic.rs` - プロトコル基本機能テスト（6テスト）
- `integration_session_management.rs` - セッション管理テスト（7テスト）
- `integration_regression_detection.rs` - リグレッション検出テスト（8テスト）
- `common/` - 共有テストユーティリティとフィクスチャ

## テスト実行

```bash
# 全テスト実行（ユニット + 統合）
cargo test

# 特定の統合テスト実行
cargo test --test integration_protocol_basic
cargo test --test integration_session_management
cargo test --test integration_regression_detection

# 詳細出力付きで実行
cargo test --test integration_protocol_basic -- --nocapture
```

## テストカバレッジ

### プロトコル基本機能テスト（6テスト）
- Connect/StateUpdate/ProcessMetrics/Disconnect メッセージシリアライゼーション
- SessionStatus表示機能
- CliToolType シリアライゼーション

### セッション管理テスト（7テスト）
- ランチャーライフサイクル（登録・削除）
- 重複ランチャー検出
- 複数ランチャー管理
- セッション操作
- 状態遷移

### リグレッション検出テスト（8テスト）
- プロトコル下位互換性
- Claude/Geminiツールサポート
- Enum安定性（SessionStatus/CliToolType）
- メッセージ構造安定性
- タイムスタンプフィールド存在確認
- エラーハンドリング堅牢性
- Unicode（日本語）サポート

## 追加されたテスト価値

### 1. リグレッション防止
- プロトコル変更時の既存機能破壊を検出
- 新機能追加時の既存テスト継続実行

### 2. 実世界シナリオ検証
- 実際のメッセージフォーマットでの動作確認
- マルチランチャー環境での動作確認

### 3. 品質保証
- プロトコル仕様の実装確認
- エラーケースの適切な処理確認
- 国際化（Unicode）サポート確認

## テスト統計

- **合計**: 26テスト（ユニット5 + 統合21）
- **成功率**: 100%
- **カバレッジ**: プロトコル、セッション管理、リグレッション検出