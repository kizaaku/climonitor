/// プロセス監視システム - 簡素化版
pub struct ProcessMonitor {
    // フィールドは必要に応じて追加
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub child_count: u32,
}

impl ProcessMonitor {
    /// 新しいProcessMonitorを作成
    pub fn new() -> Self {
        Self {
        }
    }
    
    /// プロセス情報を取得（最小実装）
    pub async fn get_process_info(&mut self) -> ProcessInfo {
        ProcessInfo {
            cpu_percent: 0.0,
            memory_mb: 0,
            child_count: 0,
        }
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self::new()
    }
}