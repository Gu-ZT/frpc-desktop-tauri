//! System integration service (ported from electron/service/SystemService.ts).
//!
//! Handles opening URLs/paths, network connectivity checks, archive
//! decompression and system usage reporting (CPU/memory of this process).

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use flate2::read::GzDecoder;
use reqwest::Client;
use sysinfo::System;

use crate::core::constants::GlobalConstant;
use crate::core::logger::Logger;
use crate::core::paths::PathUtils;
use crate::model::frpc::{SystemUsage, SystemUsageMemory};

/// Open a file with the default associated application (helper used by
/// LogService).
pub fn open_path(app: &tauri::AppHandle, path: &str) -> Result<bool, String> {
    let _ = app;
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|e| format!("open path failed: {e}"))?;
    Ok(true)
}

#[derive(Clone)]
pub struct SystemService {
    client: Client,
}

impl Default for SystemService {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemService {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(
                    GlobalConstant::INTERNET_CHECK_TIMEOUT_SECS,
                ))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Open an external URL with the default browser.
    pub async fn open_url(&self, app: &tauri::AppHandle, url: &str) -> Result<(), String> {
        let _ = app;
        if url.is_empty() {
            return Ok(());
        }
        tauri_plugin_opener::open_url(url, None::<&str>)
            .map_err(|e| format!("open url failed: {e}"))
    }

    /// Reveal a file in the platform file manager.
    pub async fn open_local_path(
        &self,
        app: &tauri::AppHandle,
        path: &str,
    ) -> Result<bool, String> {
        let _ = app;
        let path_obj = Path::new(path);
        if path_obj.is_dir() {
            tauri_plugin_opener::reveal_item_in_dir(path)
                .map_err(|e| format!("reveal failed: {e}"))?;
            Ok(true)
        } else {
            tauri_plugin_opener::open_path(path, None::<&str>)
                .map_err(|e| format!("open path failed: {e}"))?;
            Ok(true)
        }
    }

    /// Open a file with the default associated application.
    pub async fn open_local_file(
        &self,
        app: &tauri::AppHandle,
        path: &str,
    ) -> Result<bool, String> {
        let _ = app;
        tauri_plugin_opener::open_path(path, None::<&str>)
            .map_err(|e| format!("open file failed: {e}"))?;
        Ok(true)
    }

    pub fn decompress_zip_file(
        &self,
        zip_file_path: &str,
        target_path: &str,
    ) -> Result<(), String> {
        if !zip_file_path.ends_with(GlobalConstant::ZIP_EXT) {
            return Err("The file is not a .zip file".to_string());
        }
        if !Path::new(zip_file_path).exists() {
            return Err("The file does not exist".to_string());
        }
        Logger::info(
            "SystemService.decompressZipFile",
            &format!("Extracting zip: {zip_file_path} -> {target_path}"),
        );
        let file = fs::File::open(zip_file_path).map_err(|e| format!("open zip failed: {e}"))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("parse zip failed: {e}"))?;
        archive
            .extract(target_path)
            .map_err(|e| format!("extract zip failed: {e}"))?;
        Logger::info(
            "SystemService.decompressZipFile",
            &format!("Extraction completed: {target_path}"),
        );
        Ok(())
    }

    /// Extract a `.tar.gz` archive, keeping only the `frpc` binary (strip 1).
    pub fn decompress_tar_gz_file(
        &self,
        tar_gz_path: &str,
        target_path: &str,
    ) -> Result<(), String> {
        PathUtils::ensure_dir(Path::new(target_path));
        Logger::info(
            "SystemService.decompressTarGzFile",
            &format!("Extracting tar.gz: {tar_gz_path} -> {target_path}"),
        );
        let file = fs::File::open(tar_gz_path).map_err(|e| format!("open tar.gz failed: {e}"))?;
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        // strip 1 and only keep entries whose basename is "frpc"
        let mut entries = Vec::new();
        for entry in archive
            .entries()
            .map_err(|e| format!("read tar entries failed: {e}"))?
        {
            let mut entry = entry.map_err(|e| format!("tar entry error: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("tar entry path error: {e}"))?
                .to_path_buf();
            if path.file_name().and_then(|n| n.to_str()) == Some("frpc") {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("read tar entry failed: {e}"))?;
                entries.push((path, buf));
            }
        }
        // write each matched file at target_path/<basename>
        for (path, buf) in entries {
            let basename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("frpc")
                .to_string();
            fs::write(Path::new(target_path).join(basename), buf)
                .map_err(|e| format!("write extracted file failed: {e}"))?;
        }
        Logger::info(
            "SystemService.decompressTarGzFile",
            &format!("Extraction completed: {target_path}"),
        );
        Ok(())
    }

    /// Check internet connectivity (GET the msft connecttest URL).
    pub async fn check_internet_connect(&self) -> bool {
        match self
            .client
            .get(GlobalConstant::INTERNET_CHECK_URL)
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get this process's CPU usage percentage and RSS memory (MB).
    pub fn get_system_usage(&self) -> Result<SystemUsage, String> {
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        sys.refresh_memory();
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let process = sys
            .process(pid)
            .ok_or_else(|| "cannot read own process info".to_string())?;
        let cpu = process.cpu_usage() as f64;
        let used_mb = (process.memory() / 1024) as i64; // memory() returns KB on most platforms
        let total_mb = (sys.total_memory() / 1024) as i64;
        let percentage = if total_mb > 0 {
            (used_mb as f64 / total_mb as f64) * 100.0
        } else {
            0.0
        };
        Ok(SystemUsage {
            cpu,
            memory: SystemUsageMemory {
                used: used_mb,
                percentage,
            },
        })
    }

    /// Restart the app (Tauri's built-in restart, returns `!`).
    pub fn relaunch(&self, app: &tauri::AppHandle) -> Result<(), String> {
        app.restart();
    }
    /// Get local listening ports (netstat parsing, same as the Electron version).
    pub fn get_local_ports(&self) -> Result<Vec<crate::model::github::LocalPort>, String> {
        let (program, args, is_win) = if cfg!(target_os = "windows") {
            ("netstat".to_string(), vec!["-a", "-n"], true)
        } else {
            (
                "sh".to_string(),
                vec!["-c", "netstat -an | grep LISTEN"],
                false,
            )
        };
        let mut cmd = Command::new(&program);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output = cmd
            .args(&args)
            .output()
            .map_err(|e| format!("netstat failed: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut ports = Vec::new();
        if is_win {
            for line in stdout.split("\r\n") {
                if !line.contains("TCP") && !line.contains("UDP") {
                    continue;
                }
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() < 2 {
                    continue;
                }
                let local = cols[1];
                let s = local.rfind(':').unwrap_or(0);
                if s == 0 {
                    continue;
                }
                let local_ip = local[..s].to_string();
                let local_port: i64 = local[s + 1..].parse().unwrap_or(0);
                ports.push(crate::model::github::LocalPort {
                    protocol: cols[0].to_string(),
                    ip: local_ip,
                    port: local_port,
                });
            }
        } else {
            // darwin/linux
            for line in stdout.lines() {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() < 4 {
                    continue;
                }
                let local = cols[3];
                let sep = if cfg!(target_os = "macos") { '.' } else { ':' };
                let s = local.rfind(sep).unwrap_or(0);
                if s == 0 {
                    continue;
                }
                let local_ip = local[..s].to_string();
                let local_port: i64 = local[s + 1..].parse().unwrap_or(0);
                ports.push(crate::model::github::LocalPort {
                    protocol: cols[0].to_string(),
                    ip: local_ip,
                    port: local_port,
                });
            }
        }
        ports.sort_by_key(|p| p.port);
        Ok(ports)
    }
}
