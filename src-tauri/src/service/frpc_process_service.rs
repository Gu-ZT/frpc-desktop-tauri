//! frpc process management service (ported from
//! electron/service/FrpcProcessService.ts).
//!
//! Handles starting/stopping/reloading the frpc binary, process guarding with
//! automatic recovery, disconnect notifications and connection-error
//! detection from the frpc log file.

use std::fs;
use std::path::Path;
use std::process::Command;
#[cfg(not(target_os = "macos"))]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::core::business_error::{BusinessError, ResponseCode};
use crate::core::constants::GlobalConstant;
use crate::core::logger::Logger;
use crate::core::paths::PathUtils;
use crate::db::version_repository::VersionRepository;
use crate::model::frpc::FrpcProcessStatus;
use crate::service::server_service::ServerService;
use crate::service::system_service::SystemService;

/// `CREATE_NO_WINDOW` flag: prevents a console window from flashing for
/// child processes on Windows (tasklist/taskkill/osascript etc).
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Build a `Command` that never flashes a console window on Windows.
fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Fixed paths with no spaces so sudoers matching is unambiguous.
#[cfg(target_os = "macos")]
const MAC_LAUNCHER_PATH: &str = "/usr/local/bin/frpc-desktop-launcher";
#[cfg(target_os = "macos")]
const MAC_SUDOERS_FILE: &str = "/etc/sudoers.d/frpc-desktop";

/// Error patterns that indicate frpc failed to connect to server.
const FRPC_ERROR_PATTERNS: [&str; 2] = ["connect to server error", "login to server failed"];
/// Success patterns that indicate frpc connected successfully.
const FRPC_SUCCESS_PATTERNS: [&str; 3] = [
    "login to server success",
    "start proxy success",
    "proxy added success",
];

const DISCONNECT_NOTIFICATION_COOLDOWN: Duration = Duration::from_secs(60);
const FRPC_RECOVERY_COOLDOWN: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct FrpcProcessService {
    server_service: ServerService,
    system_service: SystemService,
    version_repo: VersionRepository,
    process: Arc<Mutex<Option<FrpcChild>>>,
    last_start_time: Arc<AtomicI64>,
    last_notification: Arc<Mutex<Instant>>,
    recovery_checking: Arc<AtomicBool>,
    last_recovery_time: Arc<Mutex<Instant>>,
}

/// A running frpc child process (or a remote pid on macOS).
struct FrpcChild {
    pid: u32,
    /// Set on macOS when the process is a detached root process.
    #[allow(dead_code)]
    detached: bool,
}

impl FrpcProcessService {
    pub fn new(
        server_service: ServerService,
        system_service: SystemService,
        version_repo: VersionRepository,
    ) -> Self {
        Self {
            server_service,
            system_service,
            version_repo,
            process: Arc::new(Mutex::new(None)),
            last_start_time: Arc::new(AtomicI64::new(-1)),
            last_notification: Arc::new(Mutex::new(
                Instant::now() - DISCONNECT_NOTIFICATION_COOLDOWN,
            )),
            recovery_checking: Arc::new(AtomicBool::new(false)),
            last_recovery_time: Arc::new(Mutex::new(Instant::now() - FRPC_RECOVERY_COOLDOWN)),
        }
    }

    /// Whether the mac privileged helper is installed.
    #[cfg(target_os = "macos")]
    fn is_mac_helper_ready() -> bool {
        Path::new(MAC_LAUNCHER_PATH).exists() && Path::new(MAC_SUDOERS_FILE).exists()
    }

    /// Install the mac privileged helper (one-time, password prompt).
    #[cfg(target_os = "macos")]
    fn install_mac_helper() -> Result<(), BusinessError> {
        let launcher_content = [
            "#!/bin/bash",
            "ACTION=\"$1\"",
            "if [ \"$ACTION\" = \"start\" ]; then",
            "  \"$2\" -c \"$3\" &",
            "  echo $!",
            "elif [ \"$ACTION\" = \"stop\" ]; then",
            "  kill \"$2\"",
            "fi",
            "",
        ]
        .join("\n");
        let temp_launcher = "/tmp/frpc_desktop_launcher_setup.sh";
        let username = std::env::var("USER").unwrap_or_else(|_| "ALL".to_string());
        let temp_sudoers = "/tmp/frpc_desktop_sudoers_setup";

        fs::write(temp_launcher, launcher_content)
            .map_err(|e| BusinessError::internal(format!("write helper failed: {e}")))?;
        fs::write(
            temp_sudoers,
            format!("{username} ALL=(ALL) NOPASSWD: {MAC_LAUNCHER_PATH}\n"),
        )
        .map_err(|e| BusinessError::internal(format!("write sudoers failed: {e}")))?;

        let install_cmd = format!(
            "mkdir -p /usr/local/bin && cp {temp_launcher} {MAC_LAUNCHER_PATH} && chmod 755 {MAC_LAUNCHER_PATH} && chown root:wheel {MAC_LAUNCHER_PATH} && cp {temp_sudoers} {MAC_SUDOERS_FILE} && chmod 440 {MAC_SUDOERS_FILE} && chown root:wheel {MAC_SUDOERS_FILE}"
        );
        Logger::info(
            "FrpcProcessService.installMacHelper",
            "Installing privileged helper (one-time password prompt)",
        );
        let output = hidden_command("osascript")
            .arg("-e")
            .arg(format!(
                "do shell script \"{install_cmd}\" with administrator privileges"
            ))
            .output()
            .map_err(|e| BusinessError::internal(format!("osascript failed: {e}")))?;
        if !output.status.success() {
            return Err(BusinessError::internal(
                "macOS privileged helper installation failed".to_string(),
            ));
        }
        Logger::info(
            "FrpcProcessService.installMacHelper",
            "Privileged helper installed successfully",
        );
        Ok(())
    }

    /// Check whether a pid is alive.
    fn is_process_alive(pid: u32) -> bool {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        system.process(Pid::from_u32(pid)).is_some()
    }

    /// True when frpc is currently running (also probes for leftover processes
    /// from a previous app run).
    pub fn is_running(&self) -> bool {
        let mut guard = self.process.lock().unwrap();
        if guard.is_none() {
            let detected = Self::probe_external_frpc();
            if let Some(pid) = detected {
                *guard = Some(FrpcChild {
                    pid,
                    detached: false,
                });
                if self.last_start_time.load(Ordering::Relaxed) == -1 {
                    self.last_start_time.store(
                        Instant::now().elapsed().as_millis() as i64,
                        Ordering::Relaxed,
                    );
                }
                return true;
            }
            return false;
        }
        let child = guard.as_ref().unwrap();
        Self::is_process_alive(child.pid)
    }

    /// Probe for an external frpc process (leftover from a previous run).
    fn probe_external_frpc() -> Option<u32> {
        let process_name = if cfg!(target_os = "windows") {
            PathUtils::get_win_frp_filename()
        } else {
            PathUtils::get_frpc_filename()
        };
        #[cfg(target_os = "windows")]
        {
            let output = hidden_command("tasklist")
                .args(["/FI", &format!("IMAGENAME eq {process_name}"), "/FO", "CSV"])
                .output()
                .ok()?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.len() > 1 {
                let info: Vec<String> = lines[1]
                    .split("\",\"")
                    .map(|s| s.replace('"', ""))
                    .collect();
                if info.len() >= 2 {
                    if let Ok(pid) = info[1].parse::<u32>() {
                        return Some(pid);
                    }
                }
            }
            None
        }
        #[cfg(not(target_os = "windows"))]
        {
            let output = hidden_command("pgrep")
                .arg("-x")
                .arg(&process_name)
                .output()
                .ok()?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let first = stdout.lines().next()?.trim();
            first.parse::<u32>().ok()
        }
    }

    pub fn frpc_last_start_time(&self) -> i64 {
        self.last_start_time.load(Ordering::Relaxed)
    }

    /// Reset process state after the child exits.
    fn reset_process_state(&self, pid: u32) {
        let mut guard = self.process.lock().unwrap();
        if let Some(child) = guard.as_ref() {
            if child.pid == pid {
                *guard = None;
            }
        }
        if guard.is_none() {
            self.last_start_time.store(-1, Ordering::Relaxed);
            self.recovery_checking.store(false, Ordering::Relaxed);
        }
    }

    /// Kill a Windows process tree (only compiled on Windows).
    #[cfg(target_os = "windows")]
    fn terminate_process_tree(pid: u32) -> Result<(), BusinessError> {
        let output = hidden_command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| BusinessError::internal(format!("taskkill failed: {e}")))?;
        if output.status.success() || !Self::is_process_alive(pid) {
            Ok(())
        } else {
            Err(BusinessError::internal(format!(
                "taskkill failed for pid={pid}"
            )))
        }
    }

    /// Kill a process tree on Unix (SIGTERM); only compiled on Linux.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn terminate_process_tree(pid: u32) -> Result<(), BusinessError> {
        let output = hidden_command("kill")
            .args(["-TERM", &pid.to_string()])
            .output()
            .map_err(|e| BusinessError::internal(format!("kill failed: {e}")))?;
        if !output.status.success() && Self::is_process_alive(pid) {
            return Err(BusinessError::internal(format!(
                "kill failed for pid={pid}"
            )));
        }
        Ok(())
    }

    /// Read the last portion of the frpc log and detect a connection error.
    pub fn read_frpc_connection_error(&self) -> Option<String> {
        let log_path = PathUtils::get_frpc_log_file_path();
        if !log_path.exists() || self.last_start_time.load(Ordering::Relaxed) == -1 {
            return None;
        }
        let Ok(metadata) = fs::metadata(&log_path) else {
            return None;
        };
        if metadata.len() == 0 {
            return None;
        }
        let read_size = std::cmp::min(metadata.len(), 8192) as usize;
        let Ok(file) = fs::File::open(&log_path) else {
            return None;
        };
        use std::io::{Read, Seek, SeekFrom};
        let mut buf = vec![0u8; read_size];
        let mut reader = file;
        if reader.seek(SeekFrom::End(-(read_size as i64))).is_err() {
            return None;
        }
        if reader.read_exact(&mut buf).is_err() {
            return None;
        }
        let content = String::from_utf8_lossy(&buf);
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        for line in lines.iter().rev() {
            if FRPC_SUCCESS_PATTERNS.iter().any(|p| line.contains(p)) {
                return None;
            }
            if let Some(pattern) = FRPC_ERROR_PATTERNS.iter().find(|p| line.contains(*p)) {
                let idx = line.find(pattern).unwrap_or(0);
                let msg = line[idx..].trim().to_string();
                return Some(msg);
            }
        }
        None
    }

    /// Start the frpc process (mirrors the Electron flow).
    #[allow(unused_variables)]
    pub async fn start_frpc_process(&self, app: &tauri::AppHandle) -> Result<(), BusinessError> {
        if self.is_running() {
            Logger::info("FrpcProcessService.startFrpcProcess", "Already running");
            return Ok(());
        }
        if !self.server_service.has_server_config().await? {
            return Err(BusinessError::new(ResponseCode::NotConfig));
        }
        let config = self.server_service.get_server_config().await?;
        let version = self
            .version_repo
            .find_by_github_release_id(config.frpc_version.unwrap_or(-1))
            .map_err(|e| BusinessError::internal(format!("load version failed: {e}")))?
            .ok_or_else(|| BusinessError::new(ResponseCode::NotFoundVersion))?;

        let frpc_filename = if cfg!(target_os = "windows") {
            PathUtils::get_win_frp_filename()
        } else {
            PathUtils::get_frpc_filename()
        };
        let frpc_binary_path = version
            .local_path
            .as_ref()
            .map(|p| Path::new(p).join(&frpc_filename))
            .ok_or_else(|| BusinessError::new(ResponseCode::NotFoundVersion))?;
        if !frpc_binary_path.exists() {
            Logger::warn(
                "FrpcProcessService.startFrpcProcess",
                &format!(
                    "Binary not found at {}, removing stale DB record",
                    frpc_binary_path.display()
                ),
            );
            let _ = self.version_repo.delete_by_id(&version.id);
            return Err(BusinessError::new(ResponseCode::NotFoundVersion));
        }

        if config.web_server.port > 0 {
            let in_use = crate::util::net_utils::NetUtils::check_port_in_use(
                config.web_server.port,
                "127.0.0.1",
            );
            if in_use {
                Logger::warn(
                    "FrpcProcessService.startFrpcProcess",
                    &format!(
                        "Web Server Port {} is already in use",
                        config.web_server.port
                    ),
                );
                return Err(BusinessError::new(ResponseCode::WebServerPortInUse));
            }
        }

        let config_path = PathUtils::get_toml_config_file_path();
        self.server_service
            .gen_toml_config(&config_path.to_string_lossy())
            .await?;

        #[cfg(target_os = "macos")]
        {
            if !Self::is_mac_helper_ready() {
                Self::install_mac_helper()?;
            }
            let log_file_path = PathUtils::get_frpc_log_file_path();
            if !log_file_path.exists() {
                fs::write(&log_file_path, "").ok();
            }
            let frpc_binary = frpc_binary_path.to_string_lossy().to_string();
            let config_path_str = config_path.to_string_lossy().to_string();
            let output = hidden_command("sudo")
                .args([
                    "-n",
                    MAC_LAUNCHER_PATH,
                    "start",
                    &frpc_binary,
                    &config_path_str,
                ])
                .output()
                .map_err(|e| BusinessError::internal(format!("sudo failed: {e}")))?;
            let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(pid) = pid_str.parse::<u32>() {
                let mut guard = self.process.lock().unwrap();
                *guard = Some(FrpcChild {
                    pid,
                    detached: true,
                });
            }
            self.last_start_time.store(
                Instant::now().elapsed().as_millis() as i64,
                Ordering::Relaxed,
            );
            // macOS 分支是函数结尾（非 macOS 块被 cfg 移除），
            // return 在 macOS 编译下看似多余，但非 macOS 编译时需要它跳出。
            #[allow(clippy::needless_return)]
            return Ok(());
        }

        #[cfg(not(target_os = "macos"))]
        {
            let binary = frpc_binary_path.to_string_lossy().to_string();
            let config = config_path.to_string_lossy().to_string();
            let cwd = version.local_path.clone().unwrap_or_default();
            let mut child = hidden_command(&binary)
                .args(["-c", &config])
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .spawn()
                .map_err(|e| BusinessError::internal(format!("spawn frpc failed: {e}")))?;
            let pid = child.id();

            // drain stdout/stderr in background threads
            if let Some(stdout) = child.stdout.take() {
                std::thread::spawn(move || {
                    use std::io::Read;
                    let mut reader = stdout;
                    let mut chunk = [0u8; 4096];
                    loop {
                        match reader.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                Logger::debug(
                                    "FrpcProcessService.startFrpcProcess",
                                    &String::from_utf8_lossy(&chunk[..n]),
                                );
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                std::thread::spawn(move || {
                    use std::io::Read;
                    let mut reader = stderr;
                    let mut chunk = [0u8; 4096];
                    loop {
                        match reader.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                Logger::warn(
                                    "FrpcProcessService.startFrpcProcess",
                                    &String::from_utf8_lossy(&chunk[..n]),
                                );
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            let process_holder = self.process.clone();
            let last_start = self.last_start_time.clone();
            std::thread::spawn(move || {
                let status = child.wait();
                match status {
                    Ok(code) => {
                        Logger::warn(
                            "FrpcProcessService.startFrpcProcess",
                            &format!("frpc exited, code={code}"),
                        );
                    }
                    Err(e) => {
                        Logger::error(
                            "FrpcProcessService.startFrpcProcess",
                            &format!("frpc wait failed: {e}"),
                        );
                    }
                }
                let mut guard = process_holder.lock().unwrap();
                if let Some(c) = guard.as_ref() {
                    if c.pid == pid {
                        *guard = None;
                    }
                }
                if guard.is_none() {
                    last_start.store(-1, Ordering::Relaxed);
                }
            });
            {
                let mut guard = self.process.lock().unwrap();
                *guard = Some(FrpcChild {
                    pid,
                    detached: false,
                });
            }
            self.last_start_time.store(
                Instant::now().elapsed().as_millis() as i64,
                Ordering::Relaxed,
            );
            Logger::info(
                "FrpcProcessService.startFrpcProcess",
                &format!("frpc started successfully, pid={pid}"),
            );
            Ok(())
        }
    }

    /// Stop the frpc process.
    pub async fn stop_frpc_process(&self) -> Result<(), BusinessError> {
        let guard = self.process.lock().unwrap();
        let Some(child) = guard.as_ref() else {
            return Ok(());
        };
        let pid = child.pid;
        drop(guard);

        if !Self::is_process_alive(pid) {
            self.reset_process_state(pid);
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            let output = hidden_command("sudo")
                .args(["-n", MAC_LAUNCHER_PATH, "stop", &pid.to_string()])
                .output()
                .map_err(|e| BusinessError::internal(format!("sudo stop failed: {e}")))?;
            if !output.status.success() && Self::is_process_alive(pid) {
                return Err(BusinessError::internal(format!(
                    "sudo stop failed for pid={pid}"
                )));
            }
            self.reset_process_state(pid);
            // macOS 分支是函数结尾（非 macOS 块被 cfg 移除），
            // return 在 macOS 编译下看似多余，但非 macOS 编译时需要它跳出。
            #[allow(clippy::needless_return)]
            return Ok(());
        }

        #[cfg(not(target_os = "macos"))]
        {
            match Self::terminate_process_tree(pid) {
                Ok(()) => {
                    self.reset_process_state(pid);
                    Ok(())
                }
                Err(e) => {
                    if !Self::is_process_alive(pid) {
                        self.reset_process_state(pid);
                        return Ok(());
                    }
                    Err(e)
                }
            }
        }
    }

    /// Reload the frpc config by invoking `frpc reload -c config`.
    pub async fn reload_frpc_process(&self) -> Result<(), BusinessError> {
        if !self.is_running() {
            return Ok(());
        }
        let config = self.server_service.get_server_config().await?;
        let version = self
            .version_repo
            .find_by_github_release_id(config.frpc_version.unwrap_or(-1))
            .map_err(|e| BusinessError::internal(format!("load version failed: {e}")))?
            .ok_or_else(|| BusinessError::new(ResponseCode::NotFoundVersion))?;
        let config_path = PathUtils::get_toml_config_file_path();
        self.server_service
            .gen_toml_config(&config_path.to_string_lossy())
            .await?;
        let frpc_filename = if cfg!(target_os = "windows") {
            PathUtils::get_win_frp_filename()
        } else {
            PathUtils::get_frpc_filename()
        };
        let frpc_binary_path = version
            .local_path
            .as_ref()
            .map(|p| Path::new(p).join(&frpc_filename))
            .ok_or_else(|| BusinessError::new(ResponseCode::NotFoundVersion))?;
        let output = hidden_command(&frpc_binary_path.to_string_lossy())
            .args(["reload", "-c", &config_path.to_string_lossy()])
            .current_dir(
                version
                    .local_path
                    .as_ref()
                    .map(Path::new)
                    .unwrap_or(Path::new(".")),
            )
            .output()
            .map_err(|e| BusinessError::internal(format!("frpc reload failed: {e}")))?;
        if !output.status.success() {
            return Err(BusinessError::internal(format!(
                "frpc reload failed with code {:?}",
                output.status.code()
            )));
        }
        Logger::info(
            "FrpcProcessService.reloadFrpcProcess",
            "frpc config reloaded successfully",
        );
        Ok(())
    }

    /// Start the guardian loop: restart frpc when it dies and the network is
    /// reachable again.
    pub fn start_frpc_guardian(&self, app: tauri::AppHandle) {
        let this = self.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(
                        GlobalConstant::FRPC_PROCESS_STATUS_CHECK_INTERVAL,
                    ))
                    .await;
                    if this.recovery_checking.load(Ordering::Relaxed) {
                        continue;
                    }
                    let running = this.is_running();
                    if !running && this.last_start_time.load(Ordering::Relaxed) != -1 {
                        let now = Instant::now();
                        {
                            let last = this.last_recovery_time.lock().unwrap();
                            if now.duration_since(*last) < FRPC_RECOVERY_COOLDOWN {
                                continue;
                            }
                        }
                        this.recovery_checking.store(true, Ordering::Relaxed);
                        *this.last_recovery_time.lock().unwrap() = Instant::now();
                        let net_ok = this.system_service.check_internet_connect().await;
                        if net_ok {
                            match this.start_frpc_process(&app).await {
                                Ok(()) => Logger::info(
                                    "FrpcProcessService.frpcProcessGuardian",
                                    "Network restored, frpc process restarted.",
                                ),
                                Err(e) => Logger::error(
                                    "FrpcProcessService.frpcProcessGuardian",
                                    &format!("restart failed: {e}"),
                                ),
                            }
                        } else {
                            Logger::warn(
                                "FrpcProcessService.frpcProcessGuardian",
                                "frpc is not running and network is unreachable, waiting for recovery.",
                            );
                        }
                        this.recovery_checking.store(false, Ordering::Relaxed);
                    }
                }
            });
        });
    }

    /// Watch the frpc process status every second and push `FrpcProcessStatus`
    /// on the `frpcProcess:watchFrpcLog` event.
    pub fn watch_frpc_process(
        &self,
        app: tauri::AppHandle,
        emit: impl Fn(FrpcProcessStatus) + Send + 'static,
    ) {
        let this = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(
                GlobalConstant::FRPC_PROCESS_STATUS_CHECK_INTERVAL,
            ));
            let running = this.is_running();
            if !running {
                let now = Instant::now();
                let mut last = this.last_notification.lock().unwrap();
                let can_notify = now.duration_since(*last) >= DISCONNECT_NOTIFICATION_COOLDOWN;
                if this.last_start_time.load(Ordering::Relaxed) != -1 && can_notify {
                    Logger::warn(
                        "FrpcProcessService.watchFrpcProcess",
                        "frpc process exited unexpectedly",
                    );
                    use tauri_plugin_notification::NotificationExt;
                    let _ = app
                        .notification()
                        .builder()
                        .title(app.package_info().name.clone())
                        .body("Connection lost, please check the logs for details.".to_string())
                        .show();
                    *last = now;
                }
            } else {
                *this.last_notification.lock().unwrap() =
                    Instant::now() - DISCONNECT_NOTIFICATION_COOLDOWN;
            }
            let connection_error = if running {
                this.read_frpc_connection_error()
            } else {
                None
            };
            emit(FrpcProcessStatus {
                running,
                last_start_time: this.last_start_time.load(Ordering::Relaxed),
                connection_error,
            });
        });
    }
}
