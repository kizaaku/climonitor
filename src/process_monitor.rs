use std::process::id as current_pid;
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
    pub fn new() -> Self {
        Self {
            process_id: None,
            last_cpu_time: None,
            last_cpu_usage: 0.0,
        }
    }

    /// 監視対象プロセスを設定
    pub fn set_process(&mut self, child: &Child) {
        self.process_id = child.id();
    }

    /// プロセス情報を取得
    pub async fn get_process_info(&mut self) -> ProcessInfo {
        let pid = match self.process_id {
            Some(pid) => pid,
            None => {
                return ProcessInfo {
                    cpu_percent: 0.0,
                    memory_mb: 0,
                    child_count: 0,
                    network_active: false,
                    status: ProcessStatus::Unknown,
                };
            }
        };

        let cpu_percent = self.get_cpu_usage(pid).await;
        let memory_mb = self.get_memory_usage(pid).await;
        let child_count = self.get_child_process_count(pid).await;
        let network_active = self.check_network_activity(pid).await;

        let status = self.determine_status(cpu_percent, child_count, network_active);

        ProcessInfo {
            cpu_percent,
            memory_mb,
            child_count,
            network_active,
            status,
        }
    }

    /// CPU使用率取得（簡易版）
    async fn get_cpu_usage(&mut self, pid: u32) -> f32 {
        #[cfg(target_os = "macos")]
        {
            self.get_cpu_usage_macos(pid).await
        }
        #[cfg(target_os = "linux")]
        {
            self.get_cpu_usage_linux(pid).await
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            0.0 // 他のOSでは未対応
        }
    }

    #[cfg(target_os = "macos")]
    async fn get_cpu_usage_macos(&mut self, pid: u32) -> f32 {
        use tokio::process::Command;
        
        match Command::new("ps")
            .args(&["-p", &pid.to_string(), "-o", "pcpu"])
            .output()
            .await
        {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = output_str.trim().split('\n').collect();
                
                if lines.len() >= 2 {
                    lines[1].trim().parse::<f32>().unwrap_or(0.0)
                } else {
                    0.0
                }
            }
            Err(_) => 0.0,
        }
    }

    #[cfg(target_os = "linux")]
    async fn get_cpu_usage_linux(&mut self, pid: u32) -> f32 {
        use tokio::fs;
        
        let stat_path = format!("/proc/{}/stat", pid);
        match fs::read_to_string(stat_path).await {
            Ok(content) => {
                let fields: Vec<&str> = content.split_whitespace().collect();
                if fields.len() >= 15 {
                    // 簡易CPU計算（実際はより複雑）
                    let utime: u64 = fields[13].parse().unwrap_or(0);
                    let stime: u64 = fields[14].parse().unwrap_or(0);
                    let total_time = utime + stime;
                    
                    // 前回からの差分でCPU使用率を計算（簡略化）
                    total_time as f32 / 1000.0 // 暫定値
                } else {
                    0.0
                }
            }
            Err(_) => 0.0,
        }
    }

    /// メモリ使用量取得
    async fn get_memory_usage(&self, pid: u32) -> u64 {
        #[cfg(target_os = "macos")]
        {
            self.get_memory_usage_macos(pid).await
        }
        #[cfg(target_os = "linux")]
        {
            self.get_memory_usage_linux(pid).await
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            0
        }
    }

    #[cfg(target_os = "macos")]
    async fn get_memory_usage_macos(&self, pid: u32) -> u64 {
        use tokio::process::Command;
        
        match Command::new("ps")
            .args(&["-p", &pid.to_string(), "-o", "rss"])
            .output()
            .await
        {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = output_str.trim().split('\n').collect();
                
                if lines.len() >= 2 {
                    let rss_kb: u64 = lines[1].trim().parse().unwrap_or(0);
                    rss_kb / 1024 // KB to MB
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    #[cfg(target_os = "linux")]
    async fn get_memory_usage_linux(&self, pid: u32) -> u64 {
        use tokio::fs;
        
        let status_path = format!("/proc/{}/status", pid);
        match fs::read_to_string(status_path).await {
            Ok(content) => {
                for line in content.lines() {
                    if line.starts_with("VmRSS:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            let kb: u64 = parts[1].parse().unwrap_or(0);
                            return kb / 1024; // KB to MB
                        }
                    }
                }
                0
            }
            Err(_) => 0,
        }
    }

    /// 子プロセス数取得
    async fn get_child_process_count(&self, pid: u32) -> u32 {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use tokio::process::Command;
            
            match Command::new("pgrep")
                .args(&["-P", &pid.to_string()])
                .output()
                .await
            {
                Ok(output) => {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    output_str.lines().count() as u32
                }
                Err(_) => 0,
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            0
        }
    }

    /// ネットワーク活動チェック（簡易版）
    async fn check_network_activity(&self, pid: u32) -> bool {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use tokio::process::Command;
            
            // lsof でネットワーク接続をチェック
            match Command::new("lsof")
                .args(&["-p", &pid.to_string(), "-i"])
                .output()
                .await
            {
                Ok(output) => !output.stdout.is_empty(),
                Err(_) => false,
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    /// プロセス状態判定
    fn determine_status(&self, cpu_percent: f32, child_count: u32, network_active: bool) -> ProcessStatus {
        if cpu_percent > 50.0 || child_count > 2 {
            ProcessStatus::HighActivity
        } else if cpu_percent > 5.0 || child_count > 0 || network_active {
            ProcessStatus::Running
        } else {
            ProcessStatus::Idle
        }
    }

    /// 現在のプロセスIDを取得
    pub fn current_process_id() -> u32 {
        current_pid()
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_monitor_creation() {
        let monitor = ProcessMonitor::new();
        assert!(monitor.process_id.is_none());
    }

    #[test]
    fn test_status_determination() {
        let monitor = ProcessMonitor::new();
        
        // 高活動
        let status = monitor.determine_status(60.0, 3, true);
        assert_eq!(status, ProcessStatus::HighActivity);
        
        // 通常動作
        let status = monitor.determine_status(10.0, 1, false);
        assert_eq!(status, ProcessStatus::Running);
        
        // アイドル
        let status = monitor.determine_status(1.0, 0, false);
        assert_eq!(status, ProcessStatus::Idle);
    }

    #[tokio::test]
    async fn test_process_info_unknown() {
        let mut monitor = ProcessMonitor::new();
        let info = monitor.get_process_info().await;
        
        assert_eq!(info.status, ProcessStatus::Unknown);
        assert_eq!(info.cpu_percent, 0.0);
        assert_eq!(info.memory_mb, 0);
    }
}