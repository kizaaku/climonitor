use ccmonitor_shared::{
    LauncherInfo, LauncherStatus, LauncherToMonitor, ProcessMetrics, SessionInfo, SessionStatus,
};
use chrono::Utc;
use std::collections::HashMap;

/// セッション管理システム
pub struct SessionManager {
    launchers: HashMap<String, LauncherInfo>,
    sessions: HashMap<String, SessionInfo>,
    process_metrics: HashMap<String, ProcessMetrics>,
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
            process_metrics: HashMap::new(),
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

        // 関連プロセス情報削除
        self.process_metrics.remove(launcher_id);

        // 関連セッションの状態更新（切断状態に）
        for session in self.sessions.values_mut() {
            if session.launcher_id == launcher_id {
                session.status = SessionStatus::Idle;
                session.last_activity = Utc::now();
            }
        }

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

    /// プロセス情報更新
    pub fn update_process_metrics(&mut self, metrics: ProcessMetrics) {
        self.process_metrics
            .insert(metrics.launcher_id.clone(), metrics);
    }

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
                    id: launcher_id,
                    project,
                    tool_type,
                    claude_args,
                    working_dir,
                    connected_at: timestamp,
                    last_activity: timestamp,
                    status: LauncherStatus::Connected,
                };
                self.add_launcher(launcher)
            }

            LauncherToMonitor::StateUpdate {
                launcher_id,
                session_id,
                status,
                ui_execution_context,
                ui_above_text,
                timestamp,
            } => {
                // launcher情報からプロジェクトとツールタイプを取得
                let (project, tool_type) = self
                    .launchers
                    .get(&launcher_id)
                    .map(|launcher| (launcher.project.clone(), Some(launcher.tool_type.clone())))
                    .unwrap_or((None, None));

                let session = SessionInfo {
                    id: session_id.clone(),
                    launcher_id: launcher_id.clone(),
                    project,
                    tool_type,
                    status,
                    confidence: 1.0,                 // 簡易実装では固定値
                    evidence: Vec::new(),            // 簡易実装では空
                    last_message: None,              // 簡易実装では空
                    launcher_context: None,          // 簡易実装では空
                    usage_reset_time: None,          // 簡易実装では空
                    is_waiting_for_execution: false, // 簡易実装では固定値
                    ui_execution_context,
                    ui_above_text,
                    created_at: self
                        .sessions
                        .get(&session_id)
                        .map(|s| s.created_at)
                        .unwrap_or(timestamp),
                    last_activity: timestamp,
                };

                self.update_session(session);
                Ok(())
            }

            LauncherToMonitor::ProcessMetrics {
                launcher_id,
                cpu_percent,
                memory_mb,
                child_count,
                network_active,
                timestamp,
            } => {
                let metrics = ProcessMetrics {
                    launcher_id: launcher_id.clone(),
                    cpu_percent,
                    memory_mb,
                    child_count,
                    network_active,
                    timestamp,
                };

                self.update_process_metrics(metrics);
                self.update_launcher_activity(&launcher_id);
                Ok(())
            }

            LauncherToMonitor::OutputCapture { launcher_id, .. } => {
                self.update_launcher_activity(&launcher_id);
                Ok(())
            }

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

    /// 統計情報取得（unknownプロジェクトを除外）
    pub fn get_stats(&self) -> SessionStats {
        let sessions_by_project = self.get_sessions_by_project();

        // unknownプロジェクトを除外した統計
        let filtered_sessions: Vec<&SessionInfo> = sessions_by_project
            .iter()
            .filter(|(project_name, _)| *project_name != "unknown")
            .flat_map(|(_, sessions)| sessions.iter().copied())
            .collect();

        let active_sessions = filtered_sessions.len();
        let total_sessions = active_sessions; // フィルタリング後のセッション数

        SessionStats {
            total_sessions,
            active_sessions,
        }
    }

    /// 古いセッションのクリーンアップ
    pub fn cleanup_old_sessions(&mut self) {
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        self.sessions
            .retain(|_, session| session.last_activity > cutoff);
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
    use ccmonitor_shared::generate_connection_id;

    #[test]
    fn test_launcher_lifecycle() {
        let mut manager = SessionManager::new();

        let launcher = LauncherInfo {
            id: generate_connection_id(),
            project: Some("test".to_string()),
            tool_type: "Claude".to_string(),
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
