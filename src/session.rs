use serde::Deserialize;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::unicode_utils::truncate_str;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,    // 🟢 作業中 (tool_use)
    Waiting,   // 🟡 入力待ち (end_turn)
    Error,     // 🔴 エラー/中断
    Idle,      // ⚪ アイドル (5分以上無活動)
}

impl SessionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Active => "🟢",
            Self::Waiting => "🟡", 
            Self::Error => "🔴",
            Self::Idle => "⚪",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "作業中",
            Self::Waiting => "入力待ち",
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
        #[serde(rename = "stop_reason")]
        stop_reason: Option<String>,
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

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    #[allow(dead_code)]
    pub project_path: String,
    pub project_name: String,
    pub status: SessionStatus,
    pub last_activity: DateTime<Utc>,
    pub last_message: String,
    pub current_task: Option<String>,
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
        }
    }

    pub fn update_from_message(&mut self, msg: &SessionMessage) {
        self.last_activity = msg.timestamp;
        
        // ステータス判定
        self.status = determine_status(msg);
        
        // 最新メッセージ更新
        if let Some(content) = extract_message_content(&msg.message) {
            self.last_message = content;
        }
        
        // 現在のタスク更新
        if msg.message_type == "assistant" {
            self.current_task = extract_current_task(&msg.message);
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

fn determine_status(msg: &SessionMessage) -> SessionStatus {
    let now = Utc::now();
    let time_diff = now.signed_duration_since(msg.timestamp);
    
    // 5分以上古い場合はアイドル
    if time_diff.num_minutes() > 5 {
        return SessionStatus::Idle;
    }
    
    // エラーチェック
    if let Some(result) = &msg.tool_use_result {
        if result.contains("Error") || result.contains("error") {
            return SessionStatus::Error;
        }
    }
    
    // メッセージタイプ別判定
    match msg.message_type.as_str() {
        "assistant" => {
            let stop_reason = match &msg.message {
                MessageContent::Assistant { stop_reason, .. } => stop_reason.as_deref(),
                _ => None,
            };
            
            match stop_reason {
                Some("tool_use") => SessionStatus::Active,
                Some("end_turn") => SessionStatus::Waiting,
                None => SessionStatus::Active, // まだ応答中
                _ => SessionStatus::Waiting,
            }
        },
        "user" => SessionStatus::Active, // ユーザー入力後、Claude応答待ち
        _ => SessionStatus::Waiting,
    }
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

    pub fn update_session(&mut self, msg: SessionMessage) {
        let session = self.sessions
            .entry(msg.session_id.clone())
            .or_insert_with(|| Session::new(msg.session_id.clone(), msg.cwd.clone()));
        
        session.update_from_message(&msg);
    }

    #[allow(dead_code)]
    pub fn get_sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<_> = self.sessions.values().collect();
        sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        sessions
    }

    pub fn get_sessions_by_project(&self) -> HashMap<String, Vec<&Session>> {
        let mut projects: HashMap<String, Vec<&Session>> = HashMap::new();
        
        for session in self.sessions.values() {
            projects
                .entry(session.project_name.clone())
                .or_insert_with(Vec::new)
                .push(session);
        }
        
        // 各プロジェクト内でも最新順にソート
        for sessions in projects.values_mut() {
            sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        }
        
        projects
    }
}