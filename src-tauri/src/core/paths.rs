//! Application data path utilities.
//!
//! CRITICAL: these paths must stay byte-for-byte identical to the Electron
//! version so that existing user data (SQLite database, downloaded frp
//! versions, logs, config) is inherited seamlessly after the migration.
//!
//! The Electron app used `app.getPath("userData")` which on Windows resolves
//! to `%APPDATA%\<productName>` = `%APPDATA%\Frpc-Desktop`. We reproduce the
//! same directory explicitly (instead of relying on Tauri's app_data_dir
//! which is derived from the bundle identifier) to guarantee compatibility.

use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

/// Directory names and hashed filenames must match the Electron version.
const APP_DATA_DIR_NAME: &str = "Frpc-Desktop";
const DB_DIR_NAME: &str = "db";
const DOWNLOAD_DIR_NAME: &str = "download";
const CONFIG_DIR_NAME: &str = "config";
const LOG_DIR_NAME: &str = "log";

/// md5("frpc") - used for the version storage dir, the binary name and the
/// toml config filename.
pub const FRPC_MD5: &str = "d9ecf567b6988bca88c46720024e12d0";
/// md5("frpc-log") - used for the frpc log filename.
pub const FRPC_LOG_MD5: &str = "71ae86cb0cda76922533992da4fc0fa8";

pub struct PathUtils;

impl PathUtils {
    /// md5 of an arbitrary string (same as Electron SecureUtils.calculateMD5).
    pub fn md5(input: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(input.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// The user data directory: `%APPDATA%/Frpc-Desktop` (Windows),
    /// `~/Library/Application Support/Frpc-Desktop` (macOS),
    /// `~/.config/Frpc-Desktop` (Linux).
    pub fn get_app_data() -> PathBuf {
        if let Ok(override_dir) = std::env::var("FRPC_DESKTOP_USER_DATA") {
            if !override_dir.is_empty() {
                return PathBuf::from(override_dir);
            }
        }
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join(APP_DATA_DIR_NAME)
    }

    pub fn get_download_storage_path() -> PathBuf {
        let dir = Self::get_app_data().join(DOWNLOAD_DIR_NAME);
        Self::ensure_dir(&dir);
        dir
    }

    pub fn get_version_storage_path() -> PathBuf {
        let dir = Self::get_app_data().join(Self::md5("frpc"));
        Self::ensure_dir(&dir);
        dir
    }

    pub fn get_config_storage_path() -> PathBuf {
        let dir = Self::get_app_data().join(CONFIG_DIR_NAME);
        Self::ensure_dir(&dir);
        dir
    }

    pub fn get_frpc_filename() -> String {
        Self::md5("frpc")
    }

    pub fn get_win_frp_filename() -> String {
        format!("{}.exe", Self::md5("frpc"))
    }

    pub fn get_data_base_storage_path() -> PathBuf {
        let dir = Self::get_app_data().join(DB_DIR_NAME);
        Self::ensure_dir(&dir);
        dir
    }

    pub fn get_database_file_path() -> PathBuf {
        Self::get_data_base_storage_path().join("frpc-desktop.sqlite3")
    }

    pub fn get_toml_config_file_path() -> PathBuf {
        Self::get_config_storage_path().join(format!("{}.toml", Self::md5("frpc")))
    }

    pub fn get_frpc_log_storage_path() -> PathBuf {
        let dir = Self::get_app_data().join(LOG_DIR_NAME);
        Self::ensure_dir(&dir);
        dir
    }

    pub fn get_frpc_log_file_path() -> PathBuf {
        Self::get_frpc_log_storage_path().join(format!("{}.log", Self::md5("frpc-log")))
    }

    /// The application log file. The Electron version used
    /// `app.getPath("logs")/main.log`; we keep the same file location
    /// (`<userData>/log/main.log`) so the log viewer keeps working.
    pub fn get_app_log_file_path() -> PathBuf {
        Self::get_frpc_log_storage_path().join("main.log")
    }

    /// Legacy NeDB files (migration source).
    pub fn nedb_server_file() -> PathBuf {
        Self::get_data_base_storage_path().join("server-v2.db")
    }

    pub fn nedb_proxy_file() -> PathBuf {
        Self::get_data_base_storage_path().join("proxy-v2.db")
    }

    pub fn nedb_version_file() -> PathBuf {
        Self::get_data_base_storage_path().join("version-v2.db")
    }

    pub fn ensure_dir(path: &Path) {
        if !path.exists() {
            std::fs::create_dir_all(path).ok();
        }
    }
}
