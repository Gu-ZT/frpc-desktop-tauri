//! Unit tests for core logic: path constants, TOML generation, NeDB parsing,
//! database migrations and repository round-trips.

use std::path::PathBuf;

use frpc_desktop_lib::core::paths::PathUtils;
use frpc_desktop_lib::db::app_config_repository::AppConfigRepository;
use frpc_desktop_lib::db::database_manager::{open_in_memory, INITIAL_SCHEMA_SQL};
use frpc_desktop_lib::db::proxy_repository::ProxyRepository;
use frpc_desktop_lib::db::server_repository::ServerRepository;
use frpc_desktop_lib::db::version_repository::VersionRepository;
use frpc_desktop_lib::model::frp::{
    FrpcProxy, FrpcProxyTransportConfig, FrpcVersion, OpenSourceFrpcDesktopServer,
};
use frpc_desktop_lib::service::server_service::ServerService;

/// Point the app data dir at a temp folder so tests never touch the real
/// user profile. Must be called before any PathUtils use.
fn isolate_user_data() {
    let dir = std::env::temp_dir().join(format!("frpc-desktop-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("FRPC_DESKTOP_USER_DATA", &dir);
}

// ------------------------------------------------------------------
// Path constants (must match the Electron version for data compatibility)
// ------------------------------------------------------------------

#[test]
fn path_constants_match_electron() {
    isolate_user_data();
    assert_eq!(PathUtils::md5("frpc"), "d9ecf567b6988bca88c46720024e12d0");
    assert_eq!(
        PathUtils::md5("frpc-log"),
        "71ae86cb0cda76922533992da4fc0fa8"
    );
    assert_eq!(
        PathUtils::get_frpc_filename(),
        "d9ecf567b6988bca88c46720024e12d0"
    );
    assert_eq!(
        PathUtils::get_win_frp_filename(),
        "d9ecf567b6988bca88c46720024e12d0.exe"
    );
    assert_eq!(
        PathUtils::get_toml_config_file_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "d9ecf567b6988bca88c46720024e12d0.toml"
    );
    assert_eq!(
        PathUtils::get_frpc_log_file_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "71ae86cb0cda76922533992da4fc0fa8.log"
    );
    // userData base dir name is the product name (like Electron) when not
    // overridden by the FRPC_DESKTOP_USER_DATA env var.
    assert_eq!(PathUtils::md5("frpc"), PathUtils::get_frpc_filename());
    // the version storage dir name is md5("frpc")
    assert_eq!(
        PathUtils::get_version_storage_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "d9ecf567b6988bca88c46720024e12d0"
    );
    // the download dir name
    assert_eq!(
        PathUtils::get_download_storage_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "download"
    );
    // the db dir name
    assert_eq!(
        PathUtils::get_data_base_storage_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "db"
    );
}

// ------------------------------------------------------------------
// Database migration + repository round-trip
// ------------------------------------------------------------------

#[test]
fn db_initial_schema_applies() {
    let db = open_in_memory().expect("open in-memory db");
    let conn = db.lock().unwrap();
    let table_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 't_frpcd_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        table_count >= 5,
        "expected business tables, got {table_count}"
    );
}

fn sample_proxy(name: &str, proxy_type: &str) -> FrpcProxy {
    FrpcProxy {
        id: String::new(),
        name: name.to_string(),
        proxy_type: proxy_type.to_string(),
        local_ip: "127.0.0.1".to_string(),
        local_port: "8080".to_string(),
        remote_port: "80".to_string(),
        custom_domains: vec!["app.example.com".to_string()],
        locations: vec!["/".to_string()],
        host_header_rewrite: String::new(),
        visitors_model: "".to_string(),
        server_user: String::new(),
        server_name: String::new(),
        secret_key: String::new(),
        bind_addr: String::new(),
        bind_port: None,
        subdomain: String::new(),
        basic_auth: false,
        http_user: String::new(),
        http_password: String::new(),
        fallback_to: String::new(),
        fallback_timeout_ms: 500,
        https2http: false,
        https2http_ca_file: String::new(),
        https2http_key_file: String::new(),
        keep_tunnel_open: false,
        status: 1,
        transport: FrpcProxyTransportConfig {
            use_encryption: false,
            use_compression: false,
            proxy_protocol_version: String::new(),
        },
    }
}

#[test]
fn server_config_round_trip() {
    let db = open_in_memory().expect("open in-memory db");
    let app_config = AppConfigRepository::new(db.clone());
    let server_repo = ServerRepository::new(db.clone(), app_config.clone());
    let proxy_repo = ProxyRepository::new(db.clone());

    let mut server = OpenSourceFrpcDesktopServer::default_server();
    server.server_addr = "example.com".to_string();
    server.server_port = 7000;
    server.auth.token = "secret".to_string();
    server.system.language = "zh-CN".to_string();

    server_repo.update_by_id("1", &server).unwrap();

    let loaded = server_repo.find_by_id("1").unwrap().unwrap();
    assert_eq!(loaded.server_addr, "example.com");
    assert_eq!(loaded.server_port, 7000);
    assert_eq!(loaded.auth.token, "secret");
    assert_eq!(loaded.system.language, "zh-CN");
    assert!(!loaded.system.launch_at_startup);

    // proxies belong to the same server
    let mut proxy = sample_proxy("web", "http");
    proxy_repo.insert(&mut proxy).unwrap();
    let proxies = proxy_repo.find_all().unwrap();
    assert_eq!(proxies.len(), 1);
    assert_eq!(proxies[0].name, "web");
    assert!(!proxies[0].id.is_empty());
}

#[test]
fn version_repo_round_trip() {
    let db = open_in_memory().expect("open in-memory db");
    let version_repo = VersionRepository::new(db.clone());
    let mut version = FrpcVersion {
        id: String::new(),
        github_release_id: 124395282,
        github_asset_id: 999,
        github_created_at: "2024-01-01T00:00:00Z".to_string(),
        name: "v0.60.0".to_string(),
        asset_name: "frp_0.60.0_linux_amd64.tar.gz".to_string(),
        version_download_count: 10,
        asset_download_count: 5,
        browser_download_url: "https://github.com/...".to_string(),
        downloaded: true,
        local_path: Some("/tmp/frpc".to_string()),
        size: "1.2 MB".to_string(),
    };
    version_repo.insert(&mut version).unwrap();
    let found = version_repo
        .find_by_github_release_id(124395282)
        .unwrap()
        .unwrap();
    assert_eq!(found.name, "v0.60.0");
    assert!(version_repo.exists(124395282).unwrap());
    assert!(!version_repo.exists(1).unwrap());
}

// ------------------------------------------------------------------
// TOML generation
// ------------------------------------------------------------------

#[test]
fn toml_generation_matches_expected_shape() {
    let db = open_in_memory().expect("open in-memory db");
    let app_config = AppConfigRepository::new(db.clone());
    let server_repo = ServerRepository::new(db.clone(), app_config.clone());
    let proxy_repo = ProxyRepository::new(db.clone());

    let mut server = OpenSourceFrpcDesktopServer::default_server();
    server.server_addr = "frp.example.com".to_string();
    server.server_port = 7000;
    server.auth.method = "token".to_string();
    server.auth.token = "abc123".to_string();
    server_repo.update_by_id("1", &server).unwrap();

    let mut proxy = sample_proxy("ssh", "tcp");
    proxy.local_port = "22".to_string();
    proxy.remote_port = "6000".to_string();
    proxy_repo.insert(&mut proxy).unwrap();

    let svc = ServerService::new(server_repo, proxy_repo);
    let out = std::env::temp_dir().join("frpc-test-config.toml");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(svc.gen_toml_config(&out.to_string_lossy()))
        .expect("gen toml");

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("serverAddr = \"frp.example.com\""));
    assert!(content.contains("serverPort = 7000"));
    assert!(content.contains("token = \"abc123\""));
    assert!(content.contains("[[proxies]]"));
    assert!(content.contains("name = \"ssh\""));
    assert!(content.contains("type = \"tcp\""));
    assert!(content.contains("localPort = 22"));
    assert!(content.contains("remotePort = 6000"));
    // log.to points at the frpc log file
    assert!(content.contains("71ae86cb0cda76922533992da4fc0fa8.log"));
    // webServer addr forced to loopback
    assert!(content.contains("addr = \"127.0.0.1\""));
}

#[test]
fn toml_generation_auth_none_omits_auth() {
    let db = open_in_memory().expect("open in-memory db");
    let app_config = AppConfigRepository::new(db.clone());
    let server_repo = ServerRepository::new(db.clone(), app_config.clone());
    let proxy_repo = ProxyRepository::new(db.clone());

    let mut server = OpenSourceFrpcDesktopServer::default_server();
    server.server_addr = "frp.example.com".to_string();
    server.auth.method = "none".to_string();
    server_repo.update_by_id("1", &server).unwrap();

    let svc = ServerService::new(server_repo, proxy_repo);
    let out = std::env::temp_dir().join("frpc-test-auth-none.toml");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(svc.gen_toml_config(&out.to_string_lossy()))
        .expect("gen toml");
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        !content.contains("[auth]"),
        "auth must be omitted when method == none"
    );
}

#[test]
fn range_port_proxy_appends_template() {
    let db = open_in_memory().expect("open in-memory db");
    let app_config = AppConfigRepository::new(db.clone());
    let server_repo = ServerRepository::new(db.clone(), app_config.clone());
    let proxy_repo = ProxyRepository::new(db.clone());

    let mut server = OpenSourceFrpcDesktopServer::default_server();
    server.server_addr = "frp.example.com".to_string();
    server_repo.update_by_id("1", &server).unwrap();

    let mut proxy = sample_proxy("ports", "tcp");
    proxy.local_port = "1000-1002".to_string();
    proxy.remote_port = "2000-2002".to_string();
    proxy_repo.insert(&mut proxy).unwrap();

    let svc = ServerService::new(server_repo, proxy_repo);
    let out = std::env::temp_dir().join("frpc-test-range.toml");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(svc.gen_toml_config(&out.to_string_lossy()))
        .expect("gen toml");
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("parseNumberRangePair \"1000-1002\" \"2000-2002\""));
    assert!(content.contains("name = \"ports-{{ $v.First }}\""));
}

// ------------------------------------------------------------------
// NeDB parsing (line-delimited JSON, skip `$$$` metadata lines)
// ------------------------------------------------------------------

#[test]
fn nedb_line_parser_skips_metadata() {
    let dir = std::env::temp_dir().join("frpc-nedb-test");
    std::fs::create_dir_all(&dir).unwrap();
    let server_file = dir.join("server-v2.db");
    let proxy_file = dir.join("proxy-v2.db");
    let version_file = dir.join("version-v2.db");

    // a realistic NeDB file: `$$$` metadata lines + JSON docs
    std::fs::write(
        &server_file,
        "$$$ 1710000000000\t1710000000000\n{\"serverAddr\":\"old.example.com\",\"serverPort\":7000,\"auth\":{\"method\":\"token\",\"token\":\"old-secret\"},\"log\":{\"to\":\"\",\"level\":\"info\",\"maxDays\":3,\"disablePrintColor\":false},\"transport\":{\"protocol\":\"tcp\"},\"system\":{\"launchAtStartup\":false,\"silentStartup\":false,\"autoConnectOnStartup\":false,\"language\":\"zh-CN\"}}\n",
    )
    .unwrap();
    std::fs::write(
        &proxy_file,
        "$$$ 1710000000000\t1710000000000\n{\"_id\":\"p1\",\"name\":\"web\",\"type\":\"http\",\"localIP\":\"127.0.0.1\",\"localPort\":\"8080\",\"remotePort\":\"80\",\"customDomains\":[\"a.com\"],\"locations\":[\"/\"],\"status\":1}\n",
    )
    .unwrap();
    std::fs::write(
        &version_file,
        "$$$ 1710000000000\t1710000000000\n{\"_id\":\"v1\",\"githubReleaseId\":123,\"githubAssetId\":456,\"githubCreatedAt\":\"2024-01-01\",\"name\":\"v0.60.0\",\"assetName\":\"frp_x.tar.gz\",\"browserDownloadUrl\":\"https://x\",\"downloaded\":true,\"localPath\":\"/tmp/frpc\"}\n",
    )
    .unwrap();

    // migrate into an in-memory DB
    let db = open_in_memory().unwrap();
    let app_config = AppConfigRepository::new(db.clone());
    let server_repo = ServerRepository::new(db.clone(), app_config.clone());
    let proxy_repo = ProxyRepository::new(db.clone());
    let version_repo = VersionRepository::new(db.clone());

    let svc = frpc_desktop_lib::db::nedb_migration::NedbMigrationService::new(
        &app_config,
        &server_repo,
        &proxy_repo,
        &version_repo,
        dir.clone(),
    );
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(svc.migrate(&db)).expect("migrate");

    let server = server_repo.find_by_id("1").unwrap().unwrap();
    assert_eq!(server.server_addr, "old.example.com");
    assert_eq!(server.auth.token, "old-secret");
    assert_eq!(server.system.language, "zh-CN");

    let proxies = proxy_repo.find_all().unwrap();
    assert_eq!(proxies.len(), 1);
    assert_eq!(proxies[0].name, "web");
    assert_eq!(proxies[0].proxy_type, "http");

    let versions = version_repo.find_all().unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].github_release_id, 123);

    // marker set + files renamed to .bak
    assert!(app_config.has_nedb_migration_marker().unwrap());
    assert!(!server_file.exists());
    let bak = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains(".migrated-"));
    assert!(bak, "NeDB files must be archived after migration");

    // idempotent: second migrate is a no-op
    rt.block_on(svc.migrate(&db)).expect("migrate again");
    assert_eq!(proxy_repo.find_all().unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------
// Constants
// ------------------------------------------------------------------

#[test]
fn platform_mapping_has_current_key() {
    let key = frpc_desktop_lib::core::constants::GlobalConstant::current_platform_key();
    let mapping = frpc_desktop_lib::core::constants::GlobalConstant::frp_arch_version_mapping();
    assert!(
        mapping.iter().any(|(k, _)| *k == key),
        "current platform {key} must be in the mapping"
    );
}

#[test]
fn initial_schema_embedded() {
    assert!(INITIAL_SCHEMA_SQL.contains("t_frpcd_servers"));
    assert!(INITIAL_SCHEMA_SQL.contains("t_frpcd_proxies"));
    assert!(INITIAL_SCHEMA_SQL.contains("t_frpcd_versions"));
}

#[allow(dead_code)]
fn _path_helper() -> PathBuf {
    PathBuf::new()
}
