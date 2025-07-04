#!/bin/bash
# Windows Toast Notification for climonitor

event_type="$1"
tool_name="$2"
message="$3" 
duration="$4"

# ログファイルに記録
LOG_FILE="$HOME/.climonitor/notifications.log"
echo "$(date '+%Y-%m-%d %H:%M:%S') - event_type=$event_type, tool=$tool_name, message=$message, duration=$duration" >> "$LOG_FILE"

case "$event_type" in
    "waiting_for_input")
        /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -ExecutionPolicy Bypass -Command "
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > \$null;
        \$Template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);
        \$RawXml = [xml] \$Template.GetXml();
        (\$RawXml.toast.visual.binding.text|where {\$_.id -eq '1'}).AppendChild(\$RawXml.CreateTextNode('$tool_name が入力待ち')) > \$null;
        (\$RawXml.toast.visual.binding.text|where {\$_.id -eq '2'}).AppendChild(\$RawXml.CreateTextNode('$message')) > \$null;
        \$audioNode = \$RawXml.CreateElement('audio');
        \$audioNode.SetAttribute('src', 'ms-winsoundevent:Notification.Default');
        \$RawXml.toast.AppendChild(\$audioNode) > \$null;
        \$SerializedXml = New-Object Windows.Data.Xml.Dom.XmlDocument;
        \$SerializedXml.LoadXml(\$RawXml.OuterXml);
        \$Toast = [Windows.UI.Notifications.ToastNotification]::new(\$SerializedXml);
        \$Toast.Tag = 'climonitor';
        \$Toast.Group = 'climonitor';
        \$Toast.SuppressPopup = \$false;
        \$Notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('climonitor');
        \$Notifier.Show(\$Toast);
        "
        ;;
    "error")
        /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -ExecutionPolicy Bypass -Command "
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > \$null;
        \$Template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);
        \$RawXml = [xml] \$Template.GetXml();
        (\$RawXml.toast.visual.binding.text|where {\$_.id -eq '1'}).AppendChild(\$RawXml.CreateTextNode('$tool_name エラー')) > \$null;
        (\$RawXml.toast.visual.binding.text|where {\$_.id -eq '2'}).AppendChild(\$RawXml.CreateTextNode('エラーが発生しました')) > \$null;
        \$SerializedXml = New-Object Windows.Data.Xml.Dom.XmlDocument;
        \$SerializedXml.LoadXml(\$RawXml.OuterXml);
        \$Toast = [Windows.UI.Notifications.ToastNotification]::new(\$SerializedXml);
        \$Toast.Tag = 'climonitor';
        \$Toast.Group = 'climonitor';
        \$Toast.SuppressPopup = \$false;
        \$Notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('climonitor');
        \$Notifier.Show(\$Toast);
        "
        ;;
    "completed")
        /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -ExecutionPolicy Bypass -Command "
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > \$null;
        \$Template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);
        \$RawXml = [xml] \$Template.GetXml();
        (\$RawXml.toast.visual.binding.text|where {\$_.id -eq '1'}).AppendChild(\$RawXml.CreateTextNode('$tool_name 完了')) > \$null;
        (\$RawXml.toast.visual.binding.text|where {\$_.id -eq '2'}).AppendChild(\$RawXml.CreateTextNode('$message')) > \$null;
        \$audioNode = \$RawXml.CreateElement('audio');
        \$audioNode.SetAttribute('src', 'ms-winsoundevent:Notification.Default');
        \$RawXml.toast.AppendChild(\$audioNode) > \$null;
        \$SerializedXml = New-Object Windows.Data.Xml.Dom.XmlDocument;
        \$SerializedXml.LoadXml(\$RawXml.OuterXml);
        \$Toast = [Windows.UI.Notifications.ToastNotification]::new(\$SerializedXml);
        \$Toast.Tag = 'climonitor';
        \$Toast.Group = 'climonitor';
        \$Toast.SuppressPopup = \$false;
        \$Notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('climonitor');
        \$Notifier.Show(\$Toast);
        "
        ;;
esac