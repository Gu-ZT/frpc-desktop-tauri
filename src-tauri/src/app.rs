//! Tauri application shell: plugins, state wiring, window/tray lifecycle.
//!
//! Port of `electron/main/index.ts`:
//! - startup: DB init → NeDB migration → services → listeners → window → tray
//! - window: minimize hides, close hides (unless quitting), show on activate
//! - tray: "显示主窗口" / "退出" menu, double-click shows window
//! - single instance, auto-connect on startup, silent start
//! - exit: stop frpc, checkpoint + close DB

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::core::logger::{init_logger, Logger};
use crate::core::paths::PathUtils;
use crate::db::app_config_repository::AppConfigRepository;
use crate::db::database_manager::DatabaseManager;
use crate::db::nedb_migration::NedbMigrationService;
use crate::db::proxy_repository::ProxyRepository;
use crate::db::server_repository::ServerRepository;
use crate::db::version_repository::VersionRepository;
use crate::ipc::commands::AppState;
use crate::ipc::router::{CHANNEL_WATCH_FRPC_PROCESS, CHANNEL_WATCH_SYSTEM_USAGE};
use crate::model::frpc::FrpcProcessStatus;
use crate::service::frpc_process_service::FrpcProcessService;
use crate::service::github_service::GitHubService;
use crate::service::log_service::LogService;
use crate::service::proxy_service::ProxyService;
use crate::service::server_service::ServerService;
use crate::service::system_service::SystemService;
use crate::service::version_service::VersionService;

/// Global quit flag (tray "退出" sets it; close then really quits).
static QUITTING: AtomicBool = AtomicBool::new(false);

pub struct FrpcDesktopApp;

impl FrpcDesktopApp {
    pub fn run() {
        init_logger();
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_notification::init())
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ))
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                // show the main window on second-instance
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }))
            .plugin(tauri_plugin_process::init())
            .invoke_handler(tauri::generate_handler![
                crate::ipc::commands::server_save_config,
                crate::ipc::commands::server_get_server_config,
                crate::ipc::commands::server_reset_all_config,
                crate::ipc::commands::server_export_config,
                crate::ipc::commands::server_import_toml_config,
                crate::ipc::commands::server_get_language,
                crate::ipc::commands::server_save_language,
                crate::ipc::commands::log_get_frp_log_content,
                crate::ipc::commands::log_get_app_log_content,
                crate::ipc::commands::log_open_frpc_log_file,
                crate::ipc::commands::log_open_app_log_file,
                crate::ipc::commands::version_get_versions,
                crate::ipc::commands::version_download_version,
                crate::ipc::commands::version_get_downloaded_versions,
                crate::ipc::commands::version_delete_downloaded_version,
                crate::ipc::commands::version_import_local_frpc_version,
                crate::ipc::commands::launch_launch,
                crate::ipc::commands::launch_terminate,
                crate::ipc::commands::launch_get_status,
                crate::ipc::commands::proxy_create_proxy,
                crate::ipc::commands::proxy_modify_proxy,
                crate::ipc::commands::proxy_delete_proxy,
                crate::ipc::commands::proxy_get_all_proxies,
                crate::ipc::commands::proxy_modify_proxy_status,
                crate::ipc::commands::proxy_get_local_ports,
                crate::ipc::commands::system_open_url,
                crate::ipc::commands::system_relaunch_app,
                crate::ipc::commands::system_open_app_data,
                crate::ipc::commands::system_select_local_file,
                crate::ipc::commands::system_get_frpc_desktop_github_last_release,
            ])
            .setup(|app| {
                let app_handle = app.handle();
                init_state(app_handle)?;
                setup_window(app_handle)?;
                setup_tray(app_handle)?;
                start_listeners(app_handle)?;
                Logger::info("FrpcDesktopApp", "Tauri app initialized.");
                Ok(())
            })
            .on_window_event(|window, event| {
                use tauri::WindowEvent;
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if !QUITTING.load(Ordering::Relaxed) {
                        api.prevent_close();
                        let _ = window.hide();
                        // macOS: hide the dock icon too (optional)
                    }
                }
            })
            .run(tauri::generate_context!())
            .expect("error while running tauri application");
    }
}

/// Initialize the database, repositories, services and Tauri state.
fn init_state(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let database_manager = DatabaseManager::new();
    let db = database_manager.initialize()?;

    let app_config_repo = AppConfigRepository::new(db.clone());
    let server_repo = ServerRepository::new(db.clone(), app_config_repo.clone());
    let version_repo = VersionRepository::new(db.clone());
    let proxy_repo = ProxyRepository::new(db.clone());

    let nedb_migration = NedbMigrationService::new(
        &app_config_repo,
        &server_repo,
        &proxy_repo,
        &version_repo,
        PathUtils::get_data_base_storage_path(),
    );
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(nedb_migration.migrate(&db))
        .map_err(std::io::Error::other)?;

    let system_service = SystemService::new();
    let server_service = ServerService::new(server_repo, proxy_repo.clone());
    let git_hub_service = GitHubService::new("1.2.6");
    let version_service = VersionService::new(
        version_repo,
        system_service.clone(),
        git_hub_service.clone(),
    );
    let log_service = LogService::new();
    let frpc_process_service = FrpcProcessService::new(
        server_service.clone(),
        system_service.clone(),
        VersionRepository::new(db.clone()),
    );
    let proxy_service = ProxyService::new(
        proxy_repo,
        frpc_process_service.clone(),
        system_service.clone(),
    );

    let state = AppState {
        server_service,
        version_service,
        log_service,
        frpc_process_service,
        proxy_service,
        system_service,
        github_service: git_hub_service,
        db,
    };
    app.manage(state);
    Ok(())
}

/// Window behavior: title, size limits, silent start.
fn setup_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<AppState>();
    let rt = tokio::runtime::Runtime::new()?;
    let silent = rt.block_on(state.server_service.is_silent_start());
    let silent = match silent {
        Ok(v) => v,
        Err(e) => {
            Logger::warn("FrpcDesktopApp", &format!("read silent start failed: {e}"));
            false
        }
    };
    let win = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    let arch = std::env::consts::ARCH;
    let title = format!("Frpc-Desktop v{} ({arch})", app.package_info().version);
    let _ = win.set_title(&title);
    if silent {
        let _ = win.hide();
    }
    Logger::info(
        "FrpcDesktopApp.initializeWindow",
        &format!(
            "=== Application Started ===\nApp       : {} v{}\nPlatform  : {} / {}\n",
            app.package_info().name,
            app.package_info().version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
    Ok(())
}

/// System tray with "显示主窗口" / "退出".
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let app_handle = app.clone();
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("default window icon missing")?;
    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip(app.package_info().name.clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                QUITTING.store(true, Ordering::Relaxed);
                let frpc = app.state::<AppState>();
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                rt.block_on(async {
                    let _ = frpc.frpc_process_service.stop_frpc_process().await;
                });
                DatabaseManager::close(&frpc.db);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::TrayIconEvent;
            if let TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    app_handle.manage(TrayState { _tray });
    Logger::info("FrpcDesktopApp.initializeTray", "Tray initialized.");
    Ok(())
}

/// Keep the tray alive (prevent drop).
struct TrayState {
    _tray: tauri::tray::TrayIcon,
}

/// Start the frpc process watcher, system usage watcher, and (when configured)
/// auto-connect on startup.
fn start_listeners(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<AppState>();

    // auto connect on startup
    let rt = tokio::runtime::Runtime::new()?;
    let auto = rt.block_on(state.server_service.is_auto_connect_on_startup());
    let auto = match auto {
        Ok(v) => v,
        Err(e) => {
            Logger::warn("FrpcDesktopApp", &format!("read auto connect failed: {e}"));
            false
        }
    };
    if auto {
        let app_handle = app.clone();
        let frpc = state.frpc_process_service.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(async {
                match frpc.start_frpc_process(&app_handle).await {
                    Ok(()) => Logger::info("FrpcDesktopApp", "AutoConnectOnStartup Completed."),
                    Err(e) => Logger::error(
                        "FrpcDesktopApp",
                        &format!("AutoConnectOnStartup failed: {e}"),
                    ),
                }
            });
        });
    }

    // watch frpc process status -> emit event
    {
        let app_handle = app.clone();
        let frpc = state.frpc_process_service.clone();
        frpc.watch_frpc_process(app_handle.clone(), move |status: FrpcProcessStatus| {
            let _ = app_handle.emit(
                CHANNEL_WATCH_FRPC_PROCESS,
                serde_json::to_value(&status).unwrap_or(serde_json::Value::Null),
            );
        });
    }

    // frpc guardian
    {
        let app_handle = app.clone();
        let frpc = state.frpc_process_service.clone();
        frpc.start_frpc_guardian(app_handle);
    }

    // system usage watcher (every second)
    {
        let app_handle = app.clone();
        let system_service = state.system_service.clone();
        std::thread::spawn(move || {
            let mut last_time = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let now = std::time::Instant::now();
                let _dt = now.duration_since(last_time).as_secs_f64();
                last_time = now;
                if let Ok(usage) = system_service.get_system_usage() {
                    let payload = serde_json::json!({
                        "cpu": usage.cpu,
                        "memory": {
                            "used": usage.memory.used,
                            "percentage": 0.0
                        }
                    });
                    let _ = app_handle.emit(CHANNEL_WATCH_SYSTEM_USAGE, payload);
                }
            }
        });
    }

    Logger::info("FrpcDesktopApp.startListeners", "Listeners initialized.");
    Ok(())
}
