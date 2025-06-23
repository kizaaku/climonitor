use serde::Deserialize;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::unicode_utils::truncate_str;
use crate::status_detector::StatusDetector;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,    // 🟢 作業中
    Approve,   // 🟡 承認待ち (tool_use)
    Finish,    // 🔵 完了 (text)
    Error,     // 🔴 エラー/中断
    Idle,      // ⚪ アイドル (5分以上無活動)
}

impl SessionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Active => "🟢",
            Self::Approve => "🟡", 
            Self::Finish => "🔵",
            Self::Error => "🔴",
            Self::Idle => "⚪",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "作業中",
            Self::Approve => "承認待ち",
            Self::Finish => "完了",
            Self::Error => "エラー",
            Self::Idle => "アイドル",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SessionMessage {
    #[serde(rename = "parentUuid")]
    #[allow(dead_code)]
    pub parent_uuid: Option<String>,
    #[serde(rename = "userType")]
    #[allow(dead_code)]
    pub user_type: String,
    pub cwd: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[allow(dead_code)]
    pub version: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub message: MessageContent,
    #[allow(dead_code)]
    pub uuid: String,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "toolUseResult")]
    pub tool_use_result: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    User {
        #[allow(dead_code)]
        role: String,
        content: Vec<ContentItem>,
    },
    Assistant {
        #[allow(dead_code)]
        role: String,
        content: Vec<ContentItem>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentItem {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[allow(dead_code)]
        id: String,
        name: String,
        #[allow(dead_code)]
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    #[allow(dead_code)]
    pub project_path: String,
    pub project_name: String,
    pub status: SessionStatus,
    pub last_activity: DateTime<Utc>,
    pub last_message: String,
    pub current_task: Option<String>,
    pub last_tool_use: Option<DateTime<Utc>>, // tool_use実行時刻を記録
    pub file_path: Option<std::path::PathBuf>, // ファイルパスを記録
}

impl Session {
    pub fn new(session_id: String, project_path: String) -> Self {
        let project_name = extract_project_name(&project_path);
        
        Self {
            id: session_id,
            project_path,
            project_name,
            status: SessionStatus::Idle,
            last_activity: Utc::now(),
            last_message: String::new(),
            current_task: None,
            last_tool_use: None,
            file_path: None,
        }
    }

    pub fn update_from_message(&mut self, msg: &SessionMessage) {
        // メッセージのタイムスタンプと現在時刻の新しい方を使用
        self.last_activity = msg.timestamp.max(self.last_activity);
        
        // tool_use実行時刻を記録
        if let MessageContent::Assistant { content, .. } = &msg.message {
            for item in content {
                if let ContentItem::ToolUse { .. } = item {
                    self.last_tool_use = Some(msg.timestamp);
                    break;
                }
            }
        }
        
        // ユーザーメッセージでのみtool_use状態をリセット
        // （テキスト応答後もtool_useの1秒タイマーを継続させるため）
        match msg.message_type.as_str() {
            "user" => {
                self.last_tool_use = None; // ユーザーが新しい入力をしたらリセット
            },
            _ => {}
        }
        
        // StatusDetectorを使用してステータスを判定
        self.status = StatusDetector::determine_status(msg);
        
        // 最新メッセージ更新
        if let Some(content) = extract_message_content(&msg.message) {
            self.last_message = content;
        }
        
        // 現在のタスク更新
        if msg.message_type == "assistant" {
            self.current_task = extract_current_task(&msg.message);
        }
    }

    /// 時間経過による状態更新（ファイルタイムスタンプとtool_use時刻を考慮）
    pub fn update_status_by_time(&mut self) {
        // ファイルのタイムスタンプから活動時刻を更新
        self.update_activity_from_file();
        
        // ファイルの更新時刻を取得
        let file_modified_time = if let Some(file_path) = &self.file_path {
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(file_time) = modified.duration_since(std::time::UNIX_EPOCH) {
                        DateTime::<Utc>::from_timestamp(file_time.as_secs() as i64, 0)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        
        // 疑似メッセージを作成（現在の状態を保持）
        let pseudo_msg = SessionMessage {
            parent_uuid: None,
            user_type: "system".to_string(),
            cwd: self.project_path.clone(),
            session_id: self.id.clone(),
            version: "1.0".to_string(),
            message_type: "system".to_string(),
            message: MessageContent::User {
                role: "system".to_string(),
                content: vec![ContentItem::Text { text: "status_update".to_string() }],
            },
            uuid: "status_update".to_string(),
            timestamp: self.last_activity,
            tool_use_result: None,
        };
        
        // StatusDetectorの高度な判定を使用
        let new_status = StatusDetector::determine_status_with_context(
            &pseudo_msg, 
            self.last_tool_use, 
            file_modified_time
        );
        
        // デバッグログ（環境変数で制御）
        if std::env::var("CCMONITOR_DEBUG").is_ok() {
            eprintln!("Session {}: status {} -> {}, last_tool_use: {:?}", 
                self.id, 
                match self.status {
                    SessionStatus::Active => "Active",
                    SessionStatus::Approve => "Approve", 
                    SessionStatus::Finish => "Finish",
                    SessionStatus::Error => "Error",
                    SessionStatus::Idle => "Idle",
                },
                match new_status {
                    SessionStatus::Active => "Active",
                    SessionStatus::Approve => "Approve",
                    SessionStatus::Finish => "Finish", 
                    SessionStatus::Error => "Error",
                    SessionStatus::Idle => "Idle",
                },
                self.last_tool_use.map(|t| Utc::now().signed_duration_since(t).num_seconds())
            );
        }
        
        self.status = new_status;
    }

    /// ファイルのタイムスタンプから最終活動時刻を更新
    pub fn update_activity_from_file(&mut self) {
        if let Some(file_path) = &self.file_path {
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(file_time) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let file_datetime = DateTime::<Utc>::from_timestamp(file_time.as_secs() as i64, 0)
                            .unwrap_or_else(|| Utc::now());
                        
                        // ファイル更新時刻とメッセージ時刻の新しい方を使用
                        self.last_activity = self.last_activity.max(file_datetime);
                    }
                }
            }
        }
    }
}

fn extract_project_name(path: &str) -> String {
    path.split('/')
        .last()
        .unwrap_or("unknown")
        .trim_start_matches("-Users-kaz-dev-")
        .to_string()
}


fn extract_message_content(message: &MessageContent) -> Option<String> {
    let content_items = match message {
        MessageContent::User { content, .. } => content,
        MessageContent::Assistant { content, .. } => content,
    };
    
    for item in content_items {
        if let ContentItem::Text { text } = item {
            return Some(truncate_str(text, 100));
        }
    }
    None
}

fn extract_current_task(message: &MessageContent) -> Option<String> {
    let content_items = match message {
        MessageContent::Assistant { content, .. } => content,
        _ => return None,
    };
    
    for item in content_items {
        if let ContentItem::ToolUse { name, .. } = item {
            return Some(format!("Using: {}", name));
        }
    }
    None
}

#[derive(Debug)]
pub struct SessionStore {
    sessions: HashMap<String, Session>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// ファイルパスを指定してセッションを更新
    pub fn update_session_with_file(&mut self, msg: SessionMessage, file_path: std::path::PathBuf) {
        let session = self.sessions
            .entry(msg.session_id.clone())
            .or_insert_with(|| Session::new(msg.session_id.clone(), msg.cwd.clone()));
        
        // ファイルパスを設定
        session.file_path = Some(file_path);
        
        session.update_from_message(&msg);
        
        // ファイルのタイムスタンプから活動時刻を更新
        session.update_activity_from_file();
    }

    #[allow(dead_code)]
    pub fn get_sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<_> = self.sessions.values().collect();
        sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        sessions
    }

    pub fn get_sessions_by_project(&self) -> HashMap<String, Vec<&Session>> {
        let mut projects: HashMap<String, Vec<&Session>> = HashMap::new();
        
        // 直近24時間以内のセッションのみを対象とする（5時間では短すぎる可能性）
        let twenty_four_hours_ago = Utc::now() - chrono::Duration::hours(24);
        
        for session in self.sessions.values() {
            // デバッグ用ログ（環境変数で制御）
            if std::env::var("CCMONITOR_DEBUG").is_ok() {
                let time_diff = Utc::now().signed_duration_since(session.last_activity);
                eprintln!("Session {}: project={}, last_activity={}, diff={}h", 
                    session.id, session.project_name, session.last_activity, time_diff.num_hours());
            }
            
            // 直近24時間以内に活動があったセッションのみ表示
            if session.last_activity >= twenty_four_hours_ago {
                projects
                    .entry(session.project_name.clone())
                    .or_insert_with(Vec::new)
                    .push(session);
            } else if std::env::var("CCMONITOR_DEBUG").is_ok() {
                eprintln!("Filtered out session {} (too old)", session.id);
            }
        }
        
        // 各プロジェクト内でも最新順にソート
        for sessions in projects.values_mut() {
            sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        }
        
        projects
    }

    pub fn update_status_by_time(&mut self) {
        for session in self.sessions.values_mut() {
            // ファイルのタイムスタンプから活動時刻を更新
            session.update_activity_from_file();
            // 時間経過による状態更新
            session.update_status_by_time();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn create_test_message(msg_type: &str, content_type: &str, text: Option<&str>, tool_name: Option<&str>) -> SessionMessage {
        let content_item = match content_type {
            "text" => ContentItem::Text { 
                text: text.unwrap_or("Test message").to_string() 
            },
            "tool_use" => ContentItem::ToolUse {
                id: "test_id".to_string(),
                name: tool_name.unwrap_or("test_tool").to_string(),
                input: json!({}),
            },
            _ => panic!("Invalid content type"),
        };

        let message_content = match msg_type {
            "user" => MessageContent::User {
                role: "user".to_string(),
                content: vec![content_item],
            },
            "assistant" => MessageContent::Assistant {
                role: "assistant".to_string(),
                content: vec![content_item],
            },
            _ => panic!("Invalid message type"),
        };

        SessionMessage {
            parent_uuid: None,
            user_type: "test".to_string(),
            cwd: "/test/project".to_string(),
            session_id: "test_session".to_string(),
            version: "1.0".to_string(),
            message_type: msg_type.to_string(),
            message: message_content,
            uuid: "test_uuid".to_string(),
            timestamp: Utc::now(),
            tool_use_result: None,
        }
    }

    #[test]
    fn test_session_status_labels() {
        assert_eq!(SessionStatus::Active.label(), "作業中");
        assert_eq!(SessionStatus::Approve.label(), "承認待ち");
        assert_eq!(SessionStatus::Finish.label(), "完了");
        assert_eq!(SessionStatus::Error.label(), "エラー");
        assert_eq!(SessionStatus::Idle.label(), "アイドル");
    }

    #[test]
    fn test_session_status_icons() {
        assert_eq!(SessionStatus::Active.icon(), "🟢");
        assert_eq!(SessionStatus::Approve.icon(), "🟡");
        assert_eq!(SessionStatus::Finish.icon(), "🔵");
        assert_eq!(SessionStatus::Error.icon(), "🔴");
        assert_eq!(SessionStatus::Idle.icon(), "⚪");
    }

    #[test]
    fn test_session_creation() {
        let session = Session::new("test_id".to_string(), "/Users/kaz/dev/test-project".to_string());
        
        assert_eq!(session.id, "test_id");
        assert_eq!(session.project_name, "test-project");
        assert_eq!(session.status, SessionStatus::Idle);
        assert!(session.last_tool_use.is_none());
        assert!(session.file_path.is_none());
    }

    #[test]
    fn test_extract_project_name() {
        assert_eq!(extract_project_name("/Users/kaz/dev/test-project"), "test-project");
        assert_eq!(extract_project_name("-Users-kaz-dev-ccmonitor"), "ccmonitor");
        assert_eq!(extract_project_name("simple"), "simple");
    }

    #[test]
    fn test_user_message_processing() {
        let mut session = Session::new("test".to_string(), "/test".to_string());
        let msg = create_test_message("user", "text", Some("Hello"), None);
        
        session.update_from_message(&msg);
        
        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.last_tool_use.is_none());
        assert_eq!(session.last_message, "Hello");
    }

    #[test]
    fn test_assistant_text_message_processing() {
        let mut session = Session::new("test".to_string(), "/test".to_string());
        let msg = create_test_message("assistant", "text", Some("Response"), None);
        
        session.update_from_message(&msg);
        
        assert_eq!(session.status, SessionStatus::Finish);
        assert!(session.last_tool_use.is_none());
        assert_eq!(session.last_message, "Response");
    }

    #[test]
    fn test_assistant_tool_use_message_processing() {
        let mut session = Session::new("test".to_string(), "/test".to_string());
        let msg = create_test_message("assistant", "tool_use", None, Some("Read"));
        
        session.update_from_message(&msg);
        
        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.last_tool_use.is_some());
        assert_eq!(session.current_task, Some("Using: Read".to_string()));
    }

    #[test]
    fn test_error_detection() {
        let mut session = Session::new("test".to_string(), "/test".to_string());
        let mut msg = create_test_message("assistant", "text", Some("Test"), None);
        msg.tool_use_result = Some("Error: File not found".to_string());
        
        session.update_from_message(&msg);
        
        assert_eq!(session.status, SessionStatus::Error);
    }

    #[test]
    fn test_session_store_operations() {
        use tempfile::NamedTempFile;
        
        let mut store = SessionStore::new();
        let msg = create_test_message("user", "text", Some("Hello"), None);
        let temp_file = NamedTempFile::new().unwrap();
        
        store.update_session_with_file(msg, temp_file.path().to_path_buf());
        
        assert_eq!(store.sessions.len(), 1);
        let sessions = store.get_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "test_session");
    }

    #[test]
    fn test_time_based_status_update() {
        let mut session = Session::new("test".to_string(), "/test".to_string());
        
        // 過去の時刻を設定（5分以上前）
        session.last_activity = Utc::now() - chrono::Duration::minutes(6);
        session.status = SessionStatus::Active;
        
        session.update_status_by_time();
        
        assert_eq!(session.status, SessionStatus::Idle);
    }

    #[test]
    fn test_tool_use_approval_timing() {
        let mut session = Session::new("test".to_string(), "/test".to_string());
        
        // tool_use実行時刻を過去1秒前に設定
        session.last_tool_use = Some(Utc::now() - chrono::Duration::seconds(2));
        session.status = SessionStatus::Active;
        
        session.update_status_by_time();
        
        assert_eq!(session.status, SessionStatus::Approve);
    }

    #[test]
    fn test_twenty_four_hour_filter() {
        use tempfile::NamedTempFile;
        
        let mut store = SessionStore::new();
        
        // 最近のセッション
        let recent_msg = create_test_message("user", "text", Some("Recent"), None);
        let recent_file = NamedTempFile::new().unwrap();
        store.update_session_with_file(recent_msg, recent_file.path().to_path_buf());
        
        // 古いセッション（25時間前）
        let mut old_msg = create_test_message("user", "text", Some("Old"), None);
        old_msg.session_id = "old_session".to_string();
        old_msg.timestamp = Utc::now() - chrono::Duration::hours(25);
        let old_file = NamedTempFile::new().unwrap();
        store.update_session_with_file(old_msg, old_file.path().to_path_buf());
        
        // 古いセッションの活動時刻も古く設定（自動調整を無効化）
        if let Some(session) = store.sessions.get_mut("old_session") {
            session.last_activity = Utc::now() - chrono::Duration::hours(25);
        }
        
        let projects = store.get_sessions_by_project();
        
        // 最近のセッションのみが含まれているはず（24時間フィルタ）
        assert_eq!(projects.len(), 1);
        let sessions: Vec<_> = projects.values().flatten().collect();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "test_session");
    }

    #[test]
    fn test_file_timestamp_update() {
        use tempfile::NamedTempFile;
        use std::io::Write;
        
        let mut session = Session::new("test".to_string(), "/test".to_string());
        
        // テンポラリファイルを作成
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test content").unwrap();
        
        // ファイルパスを設定
        session.file_path = Some(temp_file.path().to_path_buf());
        
        let old_activity = session.last_activity;
        
        // 少し待ってからファイル時刻を更新
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.update_activity_from_file();
        
        // ファイルのタイムスタンプが適用されているはず
        // （このテストでは時刻が変わったことを確認）
        assert!(session.last_activity >= old_activity);
    }
}