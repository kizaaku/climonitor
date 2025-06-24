use tokio::process::Child;

/// プロセス監視システム
pub struct ProcessMonitor {
    process_id: Option<u32>,
    last_cpu_time: Option<std::time::Instant>,
    last_cpu_usage: f32,
}

/// プロセス監視データ
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub child_count: u32,
    pub network_active: bool,
    pub status: ProcessStatus,
}

/// プロセス状態
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Running,
    Idle,
    HighActivity,
    Unknown,
}

impl ProcessMonitor {
    /// 新しいProcessMonitorを作成
    pub fn new() -> Self {
        Self {
            process_id: None,
            last_cpu_time: None,
            last_cpu_usage: 0.0,
        }
    }

    /// プロセスIDを設定
    pub fn set_process(&mut self, child: &Child) {
        self.process_id = child.id();
    }

    /// プロセス情報を取得
    pub async fn get_process_info(&mut self) -> ProcessInfo {
        ProcessInfo {
            cpu_percent: 0.0,
            memory_mb: 0,
            child_count: 0,
            network_active: false,
            status: ProcessStatus::Unknown,
        }
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self::new()
    }
}