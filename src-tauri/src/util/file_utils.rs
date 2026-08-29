//! File helpers (ported from electron/utils/FileUtils.ts).

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

pub struct FileUtils;

impl FileUtils {
    /// Human readable byte size (same output as the Electron version).
    pub fn format_bytes(bytes: i64, decimals: usize) -> String {
        if bytes == 0 {
            return "0 Bytes".to_string();
        }
        let k: f64 = 1024.0;
        let dm = if decimals < 1 { 1 } else { decimals };
        let bytes_f = bytes as f64;
        let i = (bytes_f.ln() / k.ln()).floor() as usize;
        let sizes = ["Bytes", "KB", "MB", "GB", "TB"];
        let i = i.min(sizes.len() - 1);
        let value = bytes_f / k.powi(i as i32);
        let rounded = format!("{:.*}", dm, value);
        format!("{} {}", rounded, sizes[i])
    }

    /// SHA-256 checksum of a file (hex string).
    pub fn calculate_file_checksum(file_path: &Path) -> Result<String, std::io::Error> {
        let data = fs::read(file_path)?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn mkdir(path: &Path) {
        if !path.exists() {
            fs::create_dir_all(path).ok();
        }
    }
}
