# PowerShell Notification Script for climonitor
# Simple Windows balloon notification using System.Windows.Forms

param(
    [string]$EventType,
    [string]$ToolName, 
    [string]$Message,
    [string]$Duration
)

Write-Host "Notification: $ToolName - $EventType - $Message"

# ログファイルに記録
$LogFile = "$env:USERPROFILE\.climonitor\notifications.log"
if (-not (Test-Path (Split-Path $LogFile))) {
    New-Item -ItemType Directory -Path (Split-Path $LogFile) -Force | Out-Null
}
Add-Content -Path $LogFile -Value "$(Get-Date) - $EventType $ToolName $Message"

# シンプルなバルーン通知
Add-Type -AssemblyName System.Windows.Forms
$notification = New-Object System.Windows.Forms.NotifyIcon
$notification.Icon = [System.Drawing.SystemIcons]::Information
$notification.Visible = $true
$notification.ShowBalloonTip(3000, $ToolName, $Message, [System.Windows.Forms.ToolTipIcon]::Info)
Start-Sleep -Seconds 4
$notification.Dispose()