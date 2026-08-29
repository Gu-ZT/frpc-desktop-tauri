//! Tauri command implementations (port of `electron/controller/*`).
//!
//! Each command mirrors the corresponding controller method and returns an
//! `ApiResponse` with the same JSON shape as the Electron `ResponseUtils`.

use tauri::{Emitter, State};

use crate::core::response::{wrap, wrap_unit, ApiResponse, CmdResult};
use crate::db::database_manager::DatabaseManager;
use crate::model::frp::OpenSourceFrpcDesktopServer;
use crate::service::frpc_process_service::FrpcProcessService;
use crate::service::github_service::GitHubService;
use crate::service::log_service::LogService;
use crate::service::proxy_service::ProxyService;
use crate::service::server_service::ServerService;
use crate::service::system_service::SystemService;
use crate::service::version_service::VersionService;

/// Application state exposed to Tauri commands.
pub struct AppState {
    pub server_service: ServerService,
    pub version_service: VersionService,
    pub log_service: LogService,
    pub frpc_process_service: FrpcProcessService,
    pub proxy_service: ProxyService,
    pub system_service: SystemService,
    pub github_service: GitHubService,
    pub db: crate::db::database_manager::SharedDb,
}

// ---------------------------------------------------------------------------
// SERVER
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn server_save_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    args: serde_json::Value,
) -> CmdResult {
    let server: OpenSourceFrpcDesktopServer = match serde_json::from_value(args) {
        Ok(s) => s,
        Err(e) => return Err(ApiResponse::fail_internal(format!("invalid config: {e}"))),
    };
    // apply autostart preference (replaces Electron app.setLoginItemSettings)
    {
        use tauri_plugin_autostart::ManagerExt;
        let autostart = app.autolaunch();
        let desired = server.system.launch_at_startup;
        match autostart.is_enabled() {
            Ok(enabled) if enabled != desired => {
                if desired {
                    let _ = autostart.enable();
                } else {
                    let _ = autostart.disable();
                }
            }
            _ => {}
        }
    }
    wrap(
        state
            .server_service
            .save_server_config(server)
            .await
            .map(|_| ()),
    )
}

#[tauri::command]
pub async fn server_get_server_config(state: State<'_, AppState>) -> CmdResult {
    wrap(state.server_service.get_server_config().await)
}

#[tauri::command]
pub async fn server_reset_all_config(state: State<'_, AppState>) -> CmdResult {
    let result: Result<(), crate::core::business_error::BusinessError> = async {
        state.frpc_process_service.stop_frpc_process().await?;
        DatabaseManager::reset_data(&state.db).map_err(|e| {
            crate::core::business_error::BusinessError::internal(format!("reset failed: {e}"))
        })?;
        let dirs = [
            crate::core::paths::PathUtils::get_download_storage_path(),
            crate::core::paths::PathUtils::get_version_storage_path(),
            crate::core::paths::PathUtils::get_frpc_log_storage_path(),
        ];
        for dir in dirs {
            let _ = std::fs::remove_dir_all(&dir);
            crate::core::paths::PathUtils::ensure_dir(&dir);
        }
        Ok(())
    }
    .await;
    wrap_unit(result)
}

#[tauri::command]
pub async fn server_export_config(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();
    match picked {
        Some(dir) => {
            let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S");
            let dir_path = match dir.into_path() {
                Ok(p) => p,
                Err(e) => return Err(ApiResponse::fail_internal(format!("invalid path: {e}"))),
            };
            let path = dir_path
                .join(format!("frpc-{timestamp}.toml"))
                .to_string_lossy()
                .to_string();
            match state.server_service.gen_toml_config(&path).await {
                Ok(()) => Ok(ApiResponse::success_data(serde_json::json!({
                    "canceled": false,
                    "path": path
                }))),
                Err(e) => Err(ApiResponse::fail_error(&e)),
            }
        }
        None => Ok(ApiResponse::success_data(serde_json::json!({
            "canceled": true,
            "path": ""
        }))),
    }
}

#[tauri::command]
pub async fn server_import_toml_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .add_filter("Frpc Toml ConfigFile", &["toml"])
        .blocking_pick_file();
    match picked {
        Some(path) => {
            let path = path.to_string();
            wrap(state.server_service.import_toml_config(&path).await)
        }
        None => Ok(ApiResponse::success_data(serde_json::json!({
            "canceled": true,
            "path": ""
        }))),
    }
}

#[tauri::command]
pub async fn server_get_language(state: State<'_, AppState>) -> CmdResult {
    wrap(state.server_service.get_language().await)
}

#[tauri::command]
pub async fn server_save_language(state: State<'_, AppState>, args: String) -> CmdResult {
    wrap_unit(state.server_service.save_language(&args).await)
}

// ---------------------------------------------------------------------------
// LOG
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn log_get_frp_log_content(state: State<'_, AppState>) -> CmdResult {
    match state.log_service.get_frp_log_content() {
        Ok(content) => Ok(ApiResponse::success_data(content)),
        Err(e) => Err(ApiResponse::fail_internal(e)),
    }
}

#[tauri::command]
pub async fn log_get_app_log_content(state: State<'_, AppState>) -> CmdResult {
    match state.log_service.get_app_log_content() {
        Ok(content) => Ok(ApiResponse::success_data(content)),
        Err(e) => Err(ApiResponse::fail_internal(e)),
    }
}

#[tauri::command]
pub async fn log_open_frpc_log_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult {
    let result = state
        .log_service
        .open_frpc_log_file(&app)
        .map_err(crate::core::business_error::BusinessError::internal);
    wrap_unit(result.map(|_| ()))
}

#[tauri::command]
pub async fn log_open_app_log_file(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult {
    let result = state
        .log_service
        .open_app_log_file(&app)
        .map_err(crate::core::business_error::BusinessError::internal);
    wrap_unit(result.map(|_| ()))
}

// ---------------------------------------------------------------------------
// VERSION
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn version_get_versions(state: State<'_, AppState>) -> CmdResult {
    match state.version_service.get_frp_versions_by_github().await {
        Ok(versions) => Ok(ApiResponse::success_data(versions)),
        Err(_) => match state.version_service.get_frp_version_by_local_json().await {
            Ok(local) => Ok(ApiResponse::success_data(local)),
            Err(e) => Err(ApiResponse::fail_error(&e)),
        },
    }
}

#[tauri::command]
pub async fn version_download_version(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    args: serde_json::Value,
) -> CmdResult {
    let github_release_id = args
        .get("githubReleaseId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let mirror_id = args
        .get("mirrorId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let app2 = app.clone();
    let result = state
        .version_service
        .download_frp_version(github_release_id, mirror_id, move |percent| {
            let payload = serde_json::json!({
                "percent": percent,
                "githubReleaseId": github_release_id,
                "completed": percent >= 1.0
            });
            let _ = app2.emit("version:downloadProgress", payload);
        })
        .await;
    match result {
        Ok(_) => Ok(ApiResponse::success_data(serde_json::json!({
            "percent": 1,
            "githubReleaseId": github_release_id,
            "completed": true
        }))),
        Err(e) => Err(ApiResponse::fail_error(&e)),
    }
}

#[tauri::command]
pub async fn version_get_downloaded_versions(state: State<'_, AppState>) -> CmdResult {
    match state.version_service.get_downloaded_versions().await {
        Ok(versions) => Ok(ApiResponse::success_data(versions)),
        Err(e) => Err(ApiResponse::fail_error(&e)),
    }
}

#[tauri::command]
pub async fn version_delete_downloaded_version(
    state: State<'_, AppState>,
    args: serde_json::Value,
) -> CmdResult {
    let github_release_id = args
        .get("githubReleaseId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    wrap_unit(
        state
            .version_service
            .delete_frp_version(github_release_id)
            .await,
    )
}

#[tauri::command]
pub async fn version_import_local_frpc_version(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .add_filter("Frpc", &["tar.gz", "zip"])
        .blocking_pick_file();
    match picked {
        Some(path) => {
            let path = path.to_string();
            match state.version_service.import_local_frpc_version(&path).await {
                Ok(_) => Ok(ApiResponse::success_data(serde_json::json!({
                    "canceled": false
                }))),
                Err(e) => Err(ApiResponse::fail_error(&e)),
            }
        }
        None => Ok(ApiResponse::success_data(serde_json::json!({
            "canceled": true
        }))),
    }
}

// ---------------------------------------------------------------------------
// LAUNCH
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn launch_launch(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult {
    wrap_unit(state.frpc_process_service.start_frpc_process(&app).await)
}

#[tauri::command]
pub async fn launch_terminate(state: State<'_, AppState>) -> CmdResult {
    wrap_unit(state.frpc_process_service.stop_frpc_process().await)
}

#[tauri::command]
pub async fn launch_get_status(state: State<'_, AppState>) -> CmdResult {
    let running = state.frpc_process_service.is_running();
    let connection_error = if running {
        state.frpc_process_service.read_frpc_connection_error()
    } else {
        None
    };
    Ok(ApiResponse::success_data(serde_json::json!({
        "running": running,
        "lastStartTime": state.frpc_process_service.frpc_last_start_time(),
        "connectionError": connection_error
    })))
}

// ---------------------------------------------------------------------------
// PROXY
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn proxy_create_proxy(state: State<'_, AppState>, args: serde_json::Value) -> CmdResult {
    let proxy: crate::model::frp::FrpcProxy = match serde_json::from_value(args) {
        Ok(p) => p,
        Err(e) => return Err(ApiResponse::fail_internal(format!("invalid proxy: {e}"))),
    };
    wrap(state.proxy_service.insert_proxy(proxy).await)
}

#[tauri::command]
pub async fn proxy_modify_proxy(state: State<'_, AppState>, args: serde_json::Value) -> CmdResult {
    let proxy: crate::model::frp::FrpcProxy = match serde_json::from_value(args) {
        Ok(p) => p,
        Err(e) => return Err(ApiResponse::fail_internal(format!("invalid proxy: {e}"))),
    };
    wrap(state.proxy_service.update_proxy(proxy).await)
}

#[tauri::command]
pub async fn proxy_delete_proxy(state: State<'_, AppState>, args: String) -> CmdResult {
    wrap_unit(state.proxy_service.delete_proxy(&args).await)
}

#[tauri::command]
pub async fn proxy_get_all_proxies(state: State<'_, AppState>) -> CmdResult {
    wrap(state.proxy_service.get_all_proxies().await)
}

#[tauri::command]
pub async fn proxy_modify_proxy_status(
    state: State<'_, AppState>,
    args: serde_json::Value,
) -> CmdResult {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let status = args.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    wrap_unit(state.proxy_service.update_proxy_status(&id, status).await)
}

#[tauri::command]
pub async fn proxy_get_local_ports(state: State<'_, AppState>) -> CmdResult {
    wrap(state.proxy_service.get_local_ports().await)
}

// ---------------------------------------------------------------------------
// SYSTEM
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn system_open_url(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    args: serde_json::Value,
) -> CmdResult {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let result = state
        .system_service
        .open_url(&app, &url)
        .await
        .map_err(crate::core::business_error::BusinessError::internal);
    wrap_unit(result)
}

#[tauri::command]
pub async fn system_relaunch_app(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult {
    let result = state
        .system_service
        .relaunch(&app)
        .map_err(crate::core::business_error::BusinessError::internal);
    wrap_unit(result)
}

#[tauri::command]
pub async fn system_open_app_data(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult {
    let path = crate::core::paths::PathUtils::get_app_data()
        .to_string_lossy()
        .to_string();
    let result = state
        .system_service
        .open_local_path(&app, &path)
        .await
        .map_err(crate::core::business_error::BusinessError::internal);
    wrap_unit(result.map(|_| ()))
}

#[tauri::command]
pub async fn system_select_local_file(app: tauri::AppHandle, args: serde_json::Value) -> CmdResult {
    let extensions: Vec<String> = args
        .get("extensions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if extensions.is_empty() {
        return Err(ApiResponse::fail_internal(
            "可选择扩展名不能为空".to_string(),
        ));
    }
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("File")
        .to_string();
    use tauri_plugin_dialog::DialogExt;
    let extension_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
    let picked = app
        .dialog()
        .file()
        .add_filter(&name, &extension_refs)
        .blocking_pick_file();
    match picked {
        Some(path) => Ok(ApiResponse::success_data(serde_json::json!({
            "canceled": false,
            "path": path.to_string()
        }))),
        None => Ok(ApiResponse::success_data(serde_json::json!({
            "canceled": true,
            "path": ""
        }))),
    }
}

#[tauri::command]
pub async fn system_get_frpc_desktop_github_last_release(
    state: State<'_, AppState>,
    args: serde_json::Value,
) -> CmdResult {
    let manual = args
        .get("manual")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    match state
        .github_service
        .get_github_last_release("luckjiawei/frpc-desktop")
        .await
    {
        Ok(data) => Ok(ApiResponse::success_data(serde_json::json!({
            "manual": manual,
            "version": data
        }))),
        Err(e) => Err(ApiResponse::fail_internal(e)),
    }
}
