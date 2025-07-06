# PowerShell Notification Script for climonitor
# Windows Toast Notifications

param(
    [string]$EventType,
    [string]$ToolName, 
    [string]$Message,
    [string]$Duration
)

# ログファイルに記録
$LogFile = "$env:USERPROFILE\.climonitor\notifications.log"
$LogDir = Split-Path $LogFile -Parent
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

$LogEntry = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') - event_type=$EventType, tool=$ToolName, message=$Message, duration=$Duration"
Add-Content -Path $LogFile -Value $LogEntry

# Windows Toast通知関数
function Show-ToastNotification {
    param(
        [string]$Title,
        [string]$Body
    )
    
    try {
        # Windows Runtime API を読み込み
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
        
        # トーストテンプレートを取得
        $Template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
        $RawXml = [xml] $Template.GetXml()
        
        # テキストを設定
        ($RawXml.toast.visual.binding.text | Where-Object {$_.id -eq "1"}).AppendChild($RawXml.CreateTextNode($Title)) | Out-Null
        ($RawXml.toast.visual.binding.text | Where-Object {$_.id -eq "2"}).AppendChild($RawXml.CreateTextNode($Body)) | Out-Null
        
        # 音声を追加
        $AudioNode = $RawXml.CreateElement("audio")
        $AudioNode.SetAttribute("src", "ms-winsoundevent:Notification.Default")
        $RawXml.toast.AppendChild($AudioNode) | Out-Null
        
        # 通知を作成して表示
        $SerializedXml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $SerializedXml.LoadXml($RawXml.OuterXml)
        $Toast = [Windows.UI.Notifications.ToastNotification]::new($SerializedXml)
        $Toast.Tag = "climonitor"
        $Toast.Group = "climonitor"
        $Toast.SuppressPopup = $false
        
        $Notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("climonitor")
        $Notifier.Show($Toast)
    }
    catch {
        # Toast通知が失敗した場合はバルーン通知にフォールバック
        Write-Host "Toast notification failed, using fallback: $Title - $Body"
    }
}

# イベントタイプに応じた通知
switch ($EventType) {
    "waiting_for_input" {
        Show-ToastNotification -Title "$ToolName が入力待ち" -Body $Message
    }
    "error" {
        Show-ToastNotification -Title "$ToolName エラー" -Body "エラーが発生しました: $Message"
    }
    "completed" {
        Show-ToastNotification -Title "$ToolName 完了" -Body $Message
    }
    "status_change" {
        Show-ToastNotification -Title "$ToolName 状態変化" -Body $Message
    }
    default {
        Show-ToastNotification -Title "climonitor" -Body "$ToolName: $Message"
    }
}