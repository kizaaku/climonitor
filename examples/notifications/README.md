# 通知スクリプトの例

climonitor の通知システム用のスクリプト例です。

## 使い方

1. `windows-toast.sh` を `~/.climonitor/notify.sh` にコピー
2. 実行権限を付与: `chmod +x ~/.climonitor/notify.sh`
3. climonitor を起動

## スクリプト

### windows-toast.sh
- **対象**: Windows (WSL環境)
- **機能**: Windows トースト通知（音付き）
- **依存**: PowerShell

## 引数

全てのスクリプトは以下の引数を受け取ります：

1. `event_type` - イベント種別 (`waiting`, `error`, `completed`)
2. `tool_name` - ツール名 (`claude`, `gemini`)
3. `message` - メッセージ内容
4. `duration` - 実行時間

## カスタマイズ

スクリプトは自由に編集可能です：

- 通知の条件を変更
- 音の種類を変更（`ms-winsoundevent:Notification.Mail` など）
- 通知の内容をカスタマイズ
- ログ形式の変更