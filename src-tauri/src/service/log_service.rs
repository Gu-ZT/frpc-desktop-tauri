//! Log file service (ported from electron/service/LogService.ts).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use notify::Watcher;

use crate::core::paths::PathUtils;

#[derive(Clone)]
pub struct LogService {
    frp_log_path: String,
    app_log_path: String,
    watcher: Arc<Mutex<Option<notify::RecommendedWatcher>>>,
    watching: Arc<AtomicBool>,
}

impl Default for LogService {
    fn default() -> Self {
        Self::new()
    }
}

impl LogService {
    pub fn new() -> Self {
        Self {
            frp_log_path: PathUtils::get_frpc_log_file_path()
                .to_string_lossy()
                .to_string(),
            app_log_path: PathUtils::get_app_log_file_path()
                .to_string_lossy()
                .to_string(),
            watcher: Arc::new(Mutex::new(None)),
            watching: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn get_frp_log_content(&self) -> Result<String, String> {
        Self::read_file(&self.frp_log_path)
    }

    pub fn get_app_log_content(&self) -> Result<String, String> {
        Self::read_file(&self.app_log_path)
    }

    fn read_file(path: &str) -> Result<String, String> {
        if !Path::new(path).exists() {
            return Ok(String::new());
        }
        fs::read_to_string(path).map_err(|e| format!("read log failed: {e}"))
    }

    /// Watch the frpc log file and invoke `callback` on every change.
    pub fn watch_frpc_log<F: Fn() + Send + 'static>(&self, callback: F) {
        let mut guard = self.watcher.lock().unwrap();
        if guard.is_some() {
            return;
        }
        if !Path::new(&self.frp_log_path).exists() {
            // retry after 1s (mirrors the Electron setTimeout retry)
            let watching = self.watching.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if watching.load(Ordering::Relaxed) {
                    return;
                }
                callback();
            });
            return;
        }
        self.watching.store(true, Ordering::Relaxed);
        let path = self.frp_log_path.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    callback();
                }
            }
        })
        .ok();
        if let Some(mut watcher) = watcher {
            let _ = watcher.watch(Path::new(&path), notify::RecursiveMode::NonRecursive);
            *guard = Some(watcher);
        }
    }

    pub fn open_frpc_log_file(&self, app: &tauri::AppHandle) -> Result<bool, String> {
        crate::service::system_service::open_path(app, &self.frp_log_path)
    }

    pub fn open_app_log_file(&self, app: &tauri::AppHandle) -> Result<bool, String> {
        crate::service::system_service::open_path(app, &self.app_log_path)
    }
}
