# 設定リファレンス

climonitor は TOML 形式の設定ファイルを使用して詳細な設定が可能です。

## 設定ファイルの場所

設定ファイルは以下の場所から自動的に検出されます（優先度順）：

1. `./climonitor/config.toml` - カレントディレクトリ（プロジェクト固有設定）
2. `~/.climonitor/config.toml` - ホームディレクトリ（ユーザー設定）
3. `~/.config/climonitor/config.toml` - XDG設定ディレクトリ（Linux標準）

明示的にファイルを指定する場合は `--config` オプションを使用します：

```bash
climonitor --config /path/to/custom-config.toml
```

## 設定ファイル形式

### 基本構造

```toml
[connection]
type = "unix"  # または "grpc"
grpc_bind_addr = "127.0.0.1:50051"
unix_socket_path = "/tmp/climonitor.sock"
grpc_allowed_ips = ["127.0.0.1", "192.168.1.0/24"]

[logging]
verbose = false
log_file = "~/.climonitor/climonitor.log"

[notification]
# 現在未実装（将来拡張用）

[ui]
# 現在未実装（将来拡張用）
```

## 接続設定 ([connection])

### type
- **説明**: 通信方式の指定
- **値**: `"unix"` または `"grpc"`
- **デフォルト**: `"unix"`

```toml
[connection]
type = "grpc"  # gRPC接続を使用
```

### grpc_bind_addr
- **説明**: gRPC接続時のバインドアドレス
- **形式**: `"host:port"`
- **デフォルト**: `"127.0.0.1:50051"`
- **用途**: 
  - monitor: このアドレスでサーバーを起動
  - launcher: このアドレスに接続

```toml
[connection]
grpc_bind_addr = "0.0.0.0:50051"  # 全インターフェースでリッスン
```

### unix_socket_path
- **説明**: Unix socket接続時のソケットファイルパス
- **デフォルト**: `/tmp/climonitor.sock`
- **注意**: gRPC接続時は無視される

```toml
[connection]
unix_socket_path = "/var/run/climonitor.sock"
```

### grpc_allowed_ips
- **説明**: gRPC接続時のIP許可リスト（セキュリティ機能）
- **形式**: 文字列配列
- **デフォルト**: `[]` （空の場合は全て許可）
- **対応形式**:
  - 個別IP: `"192.168.1.100"`
  - CIDR記法: `"192.168.1.0/24"`
  - 特別キーワード: `"localhost"`, `"any"`

```toml
[connection]
grpc_allowed_ips = [
    "127.0.0.1",           # ローカルホスト
    "192.168.1.0/24",      # ローカルネットワーク
    "10.0.0.100",          # 特定のIP
    "localhost"            # localhost（127.0.0.1と::1）
]
```

**セキュリティ重要事項**: 
- 空のリスト `[]` は **全てのIPからの接続を許可** します
- プロダクション環境では必ず制限を設定してください

## ログ設定 ([logging])

### verbose
- **説明**: 詳細ログの有効化
- **値**: `true` または `false`
- **デフォルト**: `false`

```toml
[logging]
verbose = true  # 詳細ログを有効化
```

### log_file
- **説明**: CLIツールの出力を保存するログファイルパス
- **デフォルト**: なし（ログファイル無効）
- **注意**: `~` はホームディレクトリに展開されます

```toml
[logging]
log_file = "~/.climonitor/sessions.log"
```

## 設定の優先順位

設定は以下の優先順位で適用されます（上位が優先）：

1. **CLIオプション** - `--grpc`, `--bind`, `--verbose` など
2. **環境変数** - `CLIMONITOR_*` 系の変数
3. **設定ファイル** - TOMLファイルの内容
4. **デフォルト値** - プログラム内蔵のデフォルト

### 例：設定上書きの流れ

```toml
# config.toml
[connection]
type = "unix"
grpc_bind_addr = "127.0.0.1:50051"

[logging]
verbose = false
```

```bash
# 環境変数で gRPC に変更
export CLIMONITOR_GRPC_ADDR="192.168.1.100:50051"

# CLI オプションで verbose 有効化
climonitor --verbose --live
```

結果：
- 接続: gRPC `192.168.1.100:50051` （環境変数が優先）
- ログ: 詳細モード（CLI引数が優先）

## 環境変数

以下の環境変数で設定を上書きできます：

| 環境変数 | 設定項目 | 例 |
|---------|---------|---|
| `CLIMONITOR_GRPC_ADDR` | gRPC接続アドレス | `192.168.1.100:50051` |
| `CLIMONITOR_SOCKET_PATH` | Unix socketパス | `/tmp/climonitor.sock` |
| `CLIMONITOR_VERBOSE` | 詳細ログ | `true` または `1` |
| `CLIMONITOR_LOG_FILE` | ログファイル | `/path/to/log.txt` |

## 設定例

### ローカル開発用（Unix Socket）

```toml
[connection]
type = "unix"
unix_socket_path = "/tmp/climonitor-dev.sock"

[logging]
verbose = true
log_file = "~/.climonitor/dev.log"
```

### リモート監視用（gRPC + セキュリティ）

```toml
[connection]
type = "grpc"
grpc_bind_addr = "0.0.0.0:50051"
grpc_allowed_ips = ["192.168.1.0/24", "10.0.0.0/8"]

[logging]
verbose = false
log_file = "~/.climonitor/remote.log"
```

### セキュア構成（制限的）

```toml
[connection]
type = "grpc"
grpc_bind_addr = "127.0.0.1:50051"
grpc_allowed_ips = ["127.0.0.1"]

[logging]
verbose = true
log_file = "~/.climonitor/secure.log"
```

## トラブルシューティング

### 設定ファイルが読み込まれない

```bash
# 設定ファイル検出状況を確認
climonitor --verbose

# 設定候補パスを確認
find ~ -name "config.toml" 2>/dev/null | grep climonitor
```

### IP許可リストでアクセス拒否

```bash
# 接続元IPを確認
climonitor --verbose --live

# ログで拒否理由を確認
tail -f ~/.climonitor/climonitor.log
```

### 設定の構文エラー

```bash
# TOML構文チェック（オンラインツール等を使用）
# または詳細ログで確認
climonitor --verbose --config your-config.toml
```

## セキュリティベストプラクティス

1. **IP制限の設定**: gRPC使用時は必ず `grpc_allowed_ips` を設定
2. **最小権限の原則**: 必要最小限のIPアドレス範囲のみ許可
3. **ローカル優先**: 可能な限りUnix socketを使用
4. **ログ監視**: `--verbose` で接続状況を定期的に確認
5. **ファイアウォール**: OSレベルでの追加保護を設定

### 悪い例

```toml
[connection]
type = "grpc"
grpc_bind_addr = "0.0.0.0:50051"
grpc_allowed_ips = []  # 危険: 全世界からアクセス可能
```

### 良い例

```toml
[connection]
type = "grpc"
grpc_bind_addr = "0.0.0.0:50051"
grpc_allowed_ips = ["192.168.1.0/24"]  # 安全: ローカルネットワークのみ
```

## 将来拡張予定

以下のセクションは将来のバージョンで実装予定です：

- `[notification]`: 通知システムの詳細設定
- `[ui]`: ユーザーインターフェース設定
- `[security]`: 追加のセキュリティオプション
- `[performance]`: パフォーマンスチューニング設定