use chrono::Utc;
use climonitor_shared::{
    LauncherInfo, LauncherStatus, LauncherToMonitor, SessionInfo, SessionStatus,
};
use std::collections::HashMap;

/// セッション管理システム
pub struct SessionManager {
    launchers: HashMap<String, LauncherInfo>,
    sessions: HashMap<String, SessionInfo>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            launchers: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    /// launcher接続処理
    pub fn add_launcher(&mut self, launcher: LauncherInfo) -> Result<(), String> {
        if self.launchers.contains_key(&launcher.id) {
            let launcher_id = &launcher.id;
            return Err(format!("Launcher {launcher_id} already exists"));
        }

        self.launchers.insert(launcher.id.clone(), launcher);
        Ok(())
    }

    /// launcher切断処理
    pub fn remove_launcher(&mut self, launcher_id: &str) -> Option<LauncherInfo> {
        // launcher削除
        let launcher = self.launchers.remove(launcher_id);

        // process_metrics フィールドは削除済み

        // 関連セッションを完全削除
        self.sessions
            .retain(|_, session| session.launcher_id != launcher_id);

        launcher
    }

    /// launcher情報更新
    pub fn update_launcher_activity(&mut self, launcher_id: &str) {
        if let Some(launcher) = self.launchers.get_mut(launcher_id) {
            launcher.last_activity = Utc::now();
            launcher.status = LauncherStatus::Active;
        }
    }

    /// セッション追加・更新
    pub fn update_session(&mut self, session: SessionInfo) {
        self.sessions.insert(session.id.clone(), session);
    }

    // update_process_metrics は削除済み

    /// メッセージ処理
    pub fn handle_message(&mut self, message: LauncherToMonitor) -> Result<(), String> {
        match message {
            LauncherToMonitor::Connect {
                launcher_id,
                project,
                tool_type,
                claude_args,
                working_dir,
                timestamp,
            } => {
                let launcher = LauncherInfo {
                    id: launcher_id.clone(),
                    project: project.clone(),
                    tool_type: tool_type.clone(),
                    claude_args,
                    working_dir,
                    connected_at: timestamp,
                    last_activity: timestamp,
                    status: LauncherStatus::Connected,
                };

                // launcher を登録
                self.add_launcher(launcher)
            }

            LauncherToMonitor::StateUpdate {
                launcher_id,
                session_id,
                status,
                ui_above_text,
                timestamp,
            } => {
                // launcher情報からプロジェクトとツールタイプを取得
                let (project, tool_type) = self
                    .launchers
                    .get(&launcher_id)
                    .map(|launcher| (launcher.project.clone(), Some(launcher.tool_type.clone())))
                    .unwrap_or((None, None));

                // 既存セッションから前回の状態変更時刻を取得
                let existing_session = self.sessions.get(&session_id);
                let (created_at, last_status_change) = existing_session
                    .map(|s| {
                        let last_change = if s.status != status {
                            // 状態が変化した場合は現在時刻
                            timestamp
                        } else {
                            // 状態が同じ場合は前回の変更時刻を保持
                            s.last_status_change
                        };
                        (s.created_at, last_change)
                    })
                    .unwrap_or((timestamp, timestamp));

                let session = SessionInfo {
                    id: session_id.clone(),
                    launcher_id: launcher_id.clone(),
                    project,
                    tool_type,
                    status,
                    previous_status: existing_session.as_ref().map(|s| s.status.clone()),
                    // confidence フィールドは削除済み
                    evidence: Vec::new(),            // 簡易実装では空
                    last_message: None,              // 簡易実装では空
                    launcher_context: None,          // 簡易実装では空
                    usage_reset_time: None,          // 簡易実装では空
                    is_waiting_for_execution: false, // 簡易実装では固定値
                    ui_above_text,
                    created_at,
                    last_activity: timestamp,
                    last_status_change,
                };

                self.update_session(session);
                Ok(())
            }

            LauncherToMonitor::ContextUpdate {
                launcher_id,
                session_id,
                ui_above_text,
                timestamp,
            } => {
                // 既存セッションのコンテキスト情報のみ更新
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.ui_above_text = ui_above_text;
                    session.last_activity = timestamp;
                } else {
                    // セッションが存在しない場合は何もしない（ContextUpdateのみなので）
                    if launcher_id.is_empty() {
                        // コンパイラの未使用変数警告を回避
                    }
                }
                Ok(())
            }

            // ProcessMetrics は削除済み

            // OutputCapture は削除済み
            LauncherToMonitor::Disconnect { launcher_id, .. } => {
                self.remove_launcher(&launcher_id);
                Ok(())
            }
        }
    }

    /// アクティブなlauncher一覧
    pub fn get_active_launchers(&self) -> Vec<&LauncherInfo> {
        self.launchers
            .values()
            .filter(|l| l.status != LauncherStatus::Disconnected)
            .collect()
    }

    /// 全launcher ID一覧を取得
    pub fn get_launcher_ids(&self) -> Vec<String> {
        self.launchers.keys().cloned().collect()
    }

    /// launcher情報を取得
    pub fn get_launcher(&self, launcher_id: &str) -> Option<&LauncherInfo> {
        self.launchers.get(launcher_id)
    }

    /// セッション情報を取得
    pub fn get_session(&self, session_id: &str) -> Option<&SessionInfo> {
        self.sessions.get(session_id)
    }

    /// アクティブなセッション一覧
    pub fn get_active_sessions(&self) -> Vec<&SessionInfo> {
        let cutoff = Utc::now() - chrono::Duration::minutes(5);
        self.sessions
            .values()
            .filter(|s| {
                // launcherが存在し、かつアクティブな条件を満たすセッションのみ
                self.launchers.contains_key(&s.launcher_id)
                    && (s.last_activity > cutoff || s.status != SessionStatus::Idle)
            })
            .collect()
    }

    /// プロジェクト別セッション取得
    pub fn get_sessions_by_project(&self) -> HashMap<String, Vec<&SessionInfo>> {
        let mut projects: HashMap<String, Vec<&SessionInfo>> = HashMap::new();

        for session in self.get_active_sessions() {
            let project_name = session
                .project
                .as_deref()
                .or_else(|| {
                    self.launchers
                        .get(&session.launcher_id)
                        .and_then(|l| l.project.as_deref())
                })
                .unwrap_or_default()
                .to_string();

            projects.entry(project_name).or_default().push(session);
        }

        // 各プロジェクト内で最新順にソート
        for sessions in projects.values_mut() {
            sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        }

        projects
    }

    /// プロジェクト別ランチャー取得（セッションと結合）
    pub fn get_launchers_by_project(
        &self,
    ) -> HashMap<String, Vec<(&LauncherInfo, Option<&SessionInfo>)>> {
        let mut projects: HashMap<String, Vec<(&LauncherInfo, Option<&SessionInfo>)>> =
            HashMap::new();

        for launcher in self.get_active_launchers() {
            let project_name = launcher.project.as_deref().unwrap_or_default().to_string();

            // このlauncherに対応するセッションを検索
            let session = self
                .sessions
                .values()
                .find(|s| s.launcher_id == launcher.id);

            projects
                .entry(project_name)
                .or_default()
                .push((launcher, session));
        }

        projects
    }

    /// 統計情報取得
    pub fn get_stats(&self) -> SessionStats {
        let active_sessions = self.sessions.len();
        let total_sessions = active_sessions;

        SessionStats {
            total_sessions,
            active_sessions,
        }
    }
}

/// 統計情報
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use climonitor_shared::{generate_connection_id, CliToolType};

    #[test]
    fn test_launcher_lifecycle() {
        let mut manager = SessionManager::new();

        let launcher = LauncherInfo {
            id: generate_connection_id(),
            project: Some("test".to_string()),
            tool_type: CliToolType::Claude,
            claude_args: vec!["--help".to_string()],
            working_dir: "/tmp".into(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            status: LauncherStatus::Connected,
        };

        let launcher_id = launcher.id.clone();

        // 追加
        assert!(manager.add_launcher(launcher).is_ok());
        assert_eq!(manager.get_active_launchers().len(), 1);

        // 削除
        assert!(manager.remove_launcher(&launcher_id).is_some());
        assert_eq!(manager.get_active_launchers().len(), 0);
    }

    #[test]
    fn test_session_stats() {
        let manager = SessionManager::new();
        let stats = manager.get_stats();

        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.active_sessions, 0);
    }
}
