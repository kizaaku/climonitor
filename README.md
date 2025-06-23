# Claude Session Monitor

Claude セッションファイルを監視し、リアルタイムでセッション状態を表示する軽量CLIツール。

## 🚀 Phase 3: リアルタイム出力監視

**NEW!** Claude Codeの出力ストリームを直接監視して、正確な状態をリアルタイムで検出する革新的なアプローチ。

### クイックスタート

```bash
# ターミナル1: Claude Codeを監視付きで起動
ccmonitor-launcher claude

# ターミナル2: リアルタイム状態表示
ccmonitor --live
```

## 特徴

- 🚀 **超高速**: Rust製で起動時間 ~1ms、メモリ使用量 ~2MB
- ⚡ **Phase 3: 真のリアルタイム監視**: Claude Codeの内部出力を直接解析
- 🎯 **正確な状態検出**: tool_use許可待ち vs 実行中を明確に判別
- 📊 **ハイブリッド監視**: リアルタイム + 従来のJSONL監視
- 📁 **プロジェクト別表示**: 作業ディレクトリごとにグループ化
- ⌨️ **インタラクティブ**: キーボード操作対応
- 🖥️ **複数モード**: TUI、ライブ、非対話、ウォッチ、デモ

## セッション状態

### Phase 3: リアルタイム検出
- 🟢 **作業中**: Claude がAPIリクエスト中、またはツール実行中
- 🟡 **承認待ち**: ツール実行許可をユーザーに要求中
- 🔵 **完了**: テキスト応答またはツール実行完了
- 🔴 **エラー**: 実行エラー、接続エラー、例外発生
- ⚪ **アイドル**: 5分以上更新がないセッション

### 従来方式との比較
| 検出方式 | tool_use判別 | 応答速度 | 正確性 |
|---------|-------------|----------|--------|
| **Phase 3** | ✅ 許可待ち/実行中を区別 | ⚡ リアルタイム | 🎯 デバッグログベース |
| 従来JSONL | ❌ 推測のみ | 🐌 ファイル監視遅延 | 📊 パターン推測 |

## インストール

```bash
# ビルド（ccmonitor + ccmonitor-launcher）
cargo build --release

# インストール（オプション）
cargo install --path .
```

## 使用方法

### 🔥 Phase 3: リアルタイム監視（推奨）

```bash
# 基本: Claude Codeを監視付きで起動
ccmonitor-launcher claude

# 詳細: デバッグパターンも表示
ccmonitor-launcher --verbose claude

# 任意のClaude引数をサポート
ccmonitor-launcher claude --project myproject
ccmonitor-launcher claude --help
```

```bash
# 別ターミナルでリアルタイム状態表示
ccmonitor --live

# 詳細ログ付きライブモード
ccmonitor --live --verbose

# プロジェクトフィルター
ccmonitor --live --project myproject
```

### 📊 従来モード

```bash
# TUIモード（リアルタイムダッシュボード）
./target/release/ccmonitor

# 非対話モード（現在の状態を表示して終了）
./target/release/ccmonitor --no-tui

# ウォッチモード（変更を継続監視）
./target/release/ccmonitor --watch

# デモモード（1秒タイマーテスト）
./target/release/ccmonitor --demo

# 詳細出力
./target/release/ccmonitor --no-tui --verbose

# 特定プロジェクトのみ
./target/release/ccmonitor --project ccmonitor
```

## キーボード操作（TUIモード）

- `q` / `Esc`: 終了
- `r`: 手動更新

## アーキテクチャ

### Phase 3: 出力ストリーム監視
1. **プロセスラッパー**: `ccmonitor-launcher`がClaude Codeを子プロセスとして起動
2. **出力キャプチャ**: `ANTHROPIC_LOG=debug`でstdout/stderrを監視
3. **パターン解析**: 正規表現でAPI呼び出し、ツール実行、エラーを検出
4. **状態配信**: Unix Domain Socketでリアルタイム状態をブロードキャスト
5. **ライブ表示**: `ccmonitor --live`が状態更新を受信して表示

### 従来方式: JSONL監視
1. **ファイルウォッチ**: `~/.claude/projects/*.jsonl`の変更を監視
2. **メッセージ解析**: JSONLエントリから状態を推測
3. **状態判定**: メッセージ内容とタイムスタンプで状態分類

## Phase 3の利点

- ✅ **根本問題解決**: tool_use「許可待ち」vs「実行中」の正確な判別
- ✅ **即座の更新**: 出力発生と同時の状態検出（遅延なし）
- ✅ **高精度**: Claude Codeの実際のデバッグログに基づく判定
- ✅ **透明性**: ユーザーのClaude操作に一切影響なし
- ✅ **拡張性**: 将来のClaude Code変更に対応可能

## いつPhase 3を使うか

**Phase 3がおすすめ:**
- リアルタイム開発ワークフロー
- ツール実行フローのデバッグ
- 即座の状態更新が必要
- Claude Codeの動作分析

**従来モードがおすすめ:**
- 過去セッションの分析
- 軽量な監視
- プロセスラッパーが使えない環境
- バックグラウンド監視

## 注意事項

- **TUIモード**: 通常のターミナルで実行してください
- **非対話モード**: どの環境でも動作します（Claude Code含む）
- **Phase 3**: `ccmonitor-launcher`が実行されていない場合、`ccmonitor --live`は適切なエラーメッセージを表示
- Claude Code環境では `--no-tui` オプションを使用してください

## 環境設定

```bash
# カスタムログディレクトリ
echo "CLAUDE_LOG_DIR=/custom/path/to/claude/logs" > .env.local

# デバッグモード
echo "CCMONITOR_DEBUG=1" >> .env.local
```

## 依存関係

- `ratatui`: ターミナルUI
- `crossterm`: クロスプラットフォーム端末制御
- `tokio`: 非同期ランタイム
- `notify`: ファイルシステム監視
- `serde`: JSON解析
- `regex`: パターンマッチング（Phase 3）

## ライセンス

MIT License