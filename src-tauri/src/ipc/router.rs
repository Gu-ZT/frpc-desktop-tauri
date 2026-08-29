//! Canonical IPC route table.
//!
//! Mirrors `electron/core/IpcRouter.ts`: the `path` strings are kept identical
//! so renderer code paths (`ipcRouters.*.path`) keep working. Each path maps
//! to a Tauri command name registered in `commands.rs`.

/// Map from the Electron IPC path to the Tauri command name.
pub fn command_for_path(path: &str) -> &'static str {
    match path {
        // SERVER
        "server/saveConfig" => "server_save_config",
        "server/getServerConfig" => "server_get_server_config",
        "server/resetAllConfig" => "server_reset_all_config",
        "server/exportConfig" => "server_export_config",
        "server/importTomlConfig" => "server_import_toml_config",
        "server/getLanguage" => "server_get_language",
        "server/saveLanguage" => "server_save_language",
        // LOG
        "log/getFrpLogContent" => "log_get_frp_log_content",
        "log/getAppLogContent" => "log_get_app_log_content",
        "log/openFrpcLogFile" => "log_open_frpc_log_file",
        "log/openAppLogFile" => "log_open_app_log_file",
        // VERSION
        "version/getVersions" => "version_get_versions",
        "version/downloadVersion" => "version_download_version",
        "version/getDownloadedVersions" => "version_get_downloaded_versions",
        "version/deleteDownloadedVersion" => "version_delete_downloaded_version",
        "version/importLocalFrpcVersion" => "version_import_local_frpc_version",
        // LAUNCH
        "launch/launch" => "launch_launch",
        "launch/terminate" => "launch_terminate",
        "launch/getStatus" => "launch_get_status",
        // PROXY
        "proxy/createProxy" => "proxy_create_proxy",
        "proxy/modifyProxy" => "proxy_modify_proxy",
        "proxy/deleteProxy" => "proxy_delete_proxy",
        "proxy/getAllProxies" => "proxy_get_all_proxies",
        "proxy/modifyProxyStatus" => "proxy_modify_proxy_status",
        "proxy/getLocalPorts" => "proxy_get_local_ports",
        // SYSTEM
        "system/openUrl" => "system_open_url",
        "system/relaunchApp" => "system_relaunch_app",
        "system/openAppData" => "system_open_app_data",
        "system/selectLocalFile" => "system_select_local_file",
        "system/getFrpcDesktopGithubLastRelease" => "system_get_frpc_desktop_github_last_release",
        _ => "",
    }
}

/// Listener channels (kept identical to the Electron `listeners` table).
pub const CHANNEL_WATCH_FRPC_PROCESS: &str = "frpcProcess:watchFrpcLog";
pub const CHANNEL_WATCH_SYSTEM_USAGE: &str = "system:watchSystemUsage";
pub const CHANNEL_DOWNLOAD_PROGRESS: &str = "version:downloadProgress";
