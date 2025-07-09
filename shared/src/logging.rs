use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// ログレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Trace => write!(f, "TRACE"),
        }
    }
}

impl From<&str> for LogLevel {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            _ => LogLevel::Info,
        }
    }
}

/// ログカテゴリ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogCategory {
    // Core categories
    System,
    Transport,
    Session,

    // Transport specific
    UnixSocket,
    Grpc,

    // Screen detection
    Screen,
    Claude,
    Gemini,

    // Protocol
    Protocol,
    Connection,

    // UI
    Display,
    Notification,
}

impl fmt::Display for LogCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogCategory::System => write!(f, "SYSTEM"),
            LogCategory::Transport => write!(f, "TRANSPORT"),
            LogCategory::Session => write!(f, "SESSION"),
            LogCategory::UnixSocket => write!(f, "UNIX"),
            LogCategory::Grpc => write!(f, "GRPC"),
            LogCategory::Screen => write!(f, "SCREEN"),
            LogCategory::Claude => write!(f, "CLAUDE"),
            LogCategory::Gemini => write!(f, "GEMINI"),
            LogCategory::Protocol => write!(f, "PROTOCOL"),
            LogCategory::Connection => write!(f, "CONNECTION"),
            LogCategory::Display => write!(f, "DISPLAY"),
            LogCategory::Notification => write!(f, "NOTIFICATION"),
        }
    }
}

/// グローバルログレベル
static GLOBAL_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

/// ログメッセージの出力先
static LOG_OUTPUT: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

/// ログレベルを設定
pub fn set_log_level(level: LogLevel) {
    GLOBAL_LOG_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// 現在のログレベルを取得
pub fn get_log_level() -> LogLevel {
    match GLOBAL_LOG_LEVEL.load(Ordering::Relaxed) {
        0 => LogLevel::Error,
        1 => LogLevel::Warn,
        2 => LogLevel::Info,
        3 => LogLevel::Debug,
        4 => LogLevel::Trace,
        _ => LogLevel::Info,
    }
}

/// ログ出力先を設定
pub fn set_log_output<F>(output: F)
where
    F: Fn(&str) + Send + Sync + 'static,
{
    let _ = LOG_OUTPUT.set(Box::new(output));
}

/// ログメッセージの出力
pub fn log_message(level: LogLevel, category: LogCategory, message: &str) {
    let current_level = get_log_level();

    // レベルチェック
    if level > current_level {
        return;
    }

    // タイムスタンプとフォーマット
    let timestamp = chrono::Utc::now().format("%H:%M:%S%.3f");
    let formatted = format!("[{timestamp}] [{level}] [{category}] {message}");

    // 出力
    if let Some(output) = LOG_OUTPUT.get() {
        output(&formatted);
    } else {
        // デフォルトはeprintln!
        eprintln!("{formatted}");
    }
}

/// ログマクロ
#[macro_export]
macro_rules! log_error {
    ($category:expr, $($arg:tt)*) => {
        $crate::logging::log_message(
            $crate::logging::LogLevel::Error,
            $category,
            &format!($($arg)*)
        );
    };
}

#[macro_export]
macro_rules! log_warn {
    ($category:expr, $($arg:tt)*) => {
        $crate::logging::log_message(
            $crate::logging::LogLevel::Warn,
            $category,
            &format!($($arg)*)
        );
    };
}

#[macro_export]
macro_rules! log_info {
    ($category:expr, $($arg:tt)*) => {
        $crate::logging::log_message(
            $crate::logging::LogLevel::Info,
            $category,
            &format!($($arg)*)
        );
    };
}

#[macro_export]
macro_rules! log_debug {
    ($category:expr, $($arg:tt)*) => {
        $crate::logging::log_message(
            $crate::logging::LogLevel::Debug,
            $category,
            &format!($($arg)*)
        );
    };
}

#[macro_export]
macro_rules! log_trace {
    ($category:expr, $($arg:tt)*) => {
        $crate::logging::log_message(
            $crate::logging::LogLevel::Trace,
            $category,
            &format!($($arg)*)
        );
    };
}

/// 便利なマクロ - よく使うカテゴリ別
#[macro_export]
macro_rules! log_system {
    ($level:ident, $($arg:tt)*) => {
        paste::paste! {
            [<log_ $level>]!($crate::logging::LogCategory::System, $($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_transport {
    ($level:ident, $($arg:tt)*) => {
        paste::paste! {
            [<log_ $level>]!($crate::logging::LogCategory::Transport, $($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_grpc {
    ($level:ident, $($arg:tt)*) => {
        paste::paste! {
            [<log_ $level>]!($crate::logging::LogCategory::Grpc, $($arg)*);
        }
    };
}

#[macro_export]
macro_rules! log_screen {
    ($level:ident, $($arg:tt)*) => {
        paste::paste! {
            [<log_ $level>]!($crate::logging::LogCategory::Screen, $($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn test_log_level_from_string() {
        assert_eq!(LogLevel::from("error"), LogLevel::Error);
        assert_eq!(LogLevel::from("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::from("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::from("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::from("invalid"), LogLevel::Info);
    }

    #[test]
    fn test_log_filtering() {
        // キャプチャ用のバッファ
        let output = Arc::new(Mutex::new(Vec::new()));
        let output_clone = output.clone();

        set_log_output(move |msg| {
            output_clone.lock().unwrap().push(msg.to_string());
        });

        // INFOレベルに設定
        set_log_level(LogLevel::Info);

        // 各レベルのメッセージをテスト
        log_error!(LogCategory::System, "Error message");
        log_warn!(LogCategory::System, "Warning message");
        log_info!(LogCategory::System, "Info message");
        log_debug!(LogCategory::System, "Debug message"); // これは出力されない
        log_trace!(LogCategory::System, "Trace message"); // これは出力されない

        let messages = output.lock().unwrap();
        assert_eq!(messages.len(), 3); // ERROR, WARN, INFO のみ
        assert!(messages[0].contains("ERROR"));
        assert!(messages[1].contains("WARN"));
        assert!(messages[2].contains("INFO"));
    }
}
