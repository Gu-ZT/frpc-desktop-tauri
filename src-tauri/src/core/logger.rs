//! Application logger.
//!
//! Ported from electron/core/Logger.ts. Logs are written to both the console
//! and `<userData>/log/main.log` (same location as the Electron version).
//! The log level can be changed at runtime (from the server config).

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;

use super::paths::PathUtils;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl LogLevel {
    pub fn parse(level: &str) -> Self {
        match level.to_lowercase().as_str() {
            "debug" | "trace" => LogLevel::Debug,
            "warn" | "warning" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

struct FileSink {
    writer: Mutex<Option<std::fs::File>>,
}

impl FileSink {
    fn new() -> Self {
        Self {
            writer: Mutex::new(None),
        }
    }

    fn write_line(&self, line: &str) {
        let mut guard = self.writer.lock().unwrap();
        if guard.is_none() {
            let path = PathUtils::get_app_log_file_path();
            PathUtils::ensure_dir(path.parent().unwrap_or(std::path::Path::new(".")));
            *guard = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok();
        }
        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }
}

fn file_sink() -> &'static FileSink {
    static SINK: OnceLock<FileSink> = OnceLock::new();
    SINK.get_or_init(FileSink::new)
}

static LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

pub struct Logger;

impl Logger {
    pub fn set_level(level: &str) {
        if level.is_empty() {
            return;
        }
        let lvl = LogLevel::parse(level);
        LEVEL.store(lvl as u8, Ordering::Relaxed);
    }

    pub fn level() -> LogLevel {
        match LEVEL.load(Ordering::Relaxed) {
            0 => LogLevel::Debug,
            2 => LogLevel::Warn,
            3 => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }

    pub fn debug(module: &str, msg: &str) {
        if LogLevel::Debug >= Self::level() {
            Self::write(LogLevel::Debug, module, msg);
        }
    }

    pub fn info(module: &str, msg: &str) {
        if LogLevel::Info >= Self::level() {
            Self::write(LogLevel::Info, module, msg);
        }
    }

    pub fn warn(module: &str, msg: &str) {
        if LogLevel::Warn >= Self::level() {
            Self::write(LogLevel::Warn, module, msg);
        }
    }

    pub fn error(module: &str, msg: &str) {
        Self::write(LogLevel::Error, module, msg);
    }

    pub fn error_display(module: &str, err: &dyn std::fmt::Display) {
        Self::write(LogLevel::Error, module, &err.to_string());
    }

    fn write(level: LogLevel, module: &str, msg: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f");
        let line = format!("[{timestamp}][{}][{module}] {msg}", level.label());
        match level {
            LogLevel::Debug | LogLevel::Info => println!("{line}"),
            LogLevel::Warn | LogLevel::Error => eprintln!("{line}"),
        }
        file_sink().write_line(&line);
    }
}

/// Initialize the logger (console + file sink).
pub fn init_logger() {
    Logger::set_level("info");
}
