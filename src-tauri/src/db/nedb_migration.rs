//! NeDB → SQLite one-time migration (ported from
//! electron/database/NedbMigrationService.ts).
//!
//! NeDB data files are newline-delimited JSON documents. Lines beginning with
//! `$$$` are index/metadata lines and must be skipped; every other non-empty
//! line is a JSON document returned by `find({})`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::core::logger::Logger;
use crate::db::app_config_repository::AppConfigRepository;
use crate::db::database_manager::SharedDb;
use crate::db::proxy_repository::ProxyRepository;
use crate::db::server_repository::ServerRepository;
use crate::db::version_repository::VersionRepository;
use crate::model::frp::{
    FrpcProxy, FrpcProxyTransportConfig, FrpcSystemConfiguration, FrpcVersion,
    OpenSourceFrpcDesktopServer, TransportConfig, TransportTlsConfig,
};
use crate::util::id_utils::IdUtils;

pub struct NedbMigrationService<'a> {
    app_config: &'a AppConfigRepository,
    _server_repo: &'a ServerRepository,
    _proxy_repo: &'a ProxyRepository,
    _version_repo: &'a VersionRepository,
    database_directory: PathBuf,
}

impl<'a> NedbMigrationService<'a> {
    pub fn new(
        app_config: &'a AppConfigRepository,
        server_repo: &'a ServerRepository,
        proxy_repo: &'a ProxyRepository,
        version_repo: &'a VersionRepository,
        database_directory: PathBuf,
    ) -> Self {
        Self {
            app_config,
            _server_repo: server_repo,
            _proxy_repo: proxy_repo,
            _version_repo: version_repo,
            database_directory,
        }
    }

    pub async fn migrate(&self, db: &SharedDb) -> Result<(), String> {
        if self
            .app_config
            .has_nedb_migration_marker()
            .map_err(|e| format!("cannot read migration marker: {e}"))?
        {
            return Ok(());
        }

        let server_file = self.database_directory.join("server-v2.db");
        let proxy_file = self.database_directory.join("proxy-v2.db");
        let version_file = self.database_directory.join("version-v2.db");

        let existing_files: Vec<PathBuf> = [&server_file, &proxy_file, &version_file]
            .into_iter()
            .filter(|p| p.exists())
            .cloned()
            .collect();
        if existing_files.is_empty() {
            return Ok(());
        }

        let server_docs = Self::load_documents(&server_file)?;
        let proxy_docs = Self::load_documents(&proxy_file)?;
        let version_docs = Self::load_documents(&version_file)?;
        if server_docs.len() > 1 {
            return Err(
                "NeDB migration failed: multiple server configurations were found.".to_string(),
            );
        }

        let proxies: Vec<FrpcProxy> = proxy_docs
            .iter()
            .enumerate()
            .map(|(i, doc)| Self::normalize_proxy(doc, i))
            .collect::<Result<_, _>>()?;
        let versions: Vec<FrpcVersion> = version_docs
            .iter()
            .enumerate()
            .map(|(i, doc)| Self::normalize_version(doc, i))
            .collect::<Result<_, _>>()?;
        let server = match server_docs.first() {
            Some(doc) => Some(Self::normalize_server(doc)?),
            None if !proxies.is_empty() => Some(Self::create_default_server()),
            None => None,
        };

        Self::assert_unique(proxies.iter().map(|p| p.id.clone()), "proxy id")?;
        Self::assert_unique(versions.iter().map(|v| v.id.clone()), "version id")?;
        Self::assert_unique(
            versions.iter().map(|v| v.github_release_id.to_string()),
            "GitHub release id",
        )?;

        {
            let conn = db.lock().unwrap();
            let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
            if let Some(server) = &server {
                crate::db::server_repository::ServerRepository::upsert_with_conn(&conn, server)
                    .map_err(|e| e.to_string())?;
                crate::db::app_config_repository::AppConfigRepository::save_system_config_with_conn(
                    &conn,
                    &server.system,
                )
                .map_err(|e| e.to_string())?;
            } else {
                crate::db::app_config_repository::AppConfigRepository::save_system_config_with_conn(
                    &conn,
                    &FrpcSystemConfiguration {
                        launch_at_startup: false,
                        silent_startup: false,
                        auto_connect_on_startup: false,
                        language: "en-US".to_string(),
                    },
                )
                .map_err(|e| e.to_string())?;
            }
            for proxy in &proxies {
                crate::db::proxy_repository::ProxyRepository::upsert_with_conn(&conn, proxy)
                    .map_err(|e| e.to_string())?;
            }
            for version in &versions {
                crate::db::version_repository::VersionRepository::upsert_with_conn(&conn, version)
                    .map_err(|e| e.to_string())?;
            }
            self.verify_imported_ids(&conn, "t_frpcd_proxies", &proxies)
                .map_err(|e| e.to_string())?;
            self.verify_imported_ids(&conn, "t_frpcd_versions", &versions)
                .map_err(|e| e.to_string())?;
            crate::db::app_config_repository::AppConfigRepository::save_nedb_migration_marker_with_conn(
                &conn,
            )
            .map_err(|e| e.to_string())?;

            let fk_errors: i64 = conn
                .prepare("PRAGMA foreign_key_check")
                .map_err(|e| e.to_string())?
                .query([])
                .map_err(|e| e.to_string())?
                .next()
                .map_err(|e| e.to_string())?
                .map_or(0, |_| 1);
            if fk_errors != 0 {
                return Err(
                    "NeDB migration failed: SQLite foreign key validation failed.".to_string(),
                );
            }
            tx.commit().map_err(|e| e.to_string())?;
        }

        Self::backup_files(&existing_files);
        Logger::info(
            "NedbMigrationService",
            &format!(
                "NeDB migration completed: servers={}, proxies={}, versions={}.",
                server_docs.len(),
                proxies.len(),
                versions.len()
            ),
        );
        Ok(())
    }

    /// Read a NeDB file and return all documents (skipping `$$$` metadata lines).
    fn load_documents(filename: &Path) -> Result<Vec<serde_json::Value>, String> {
        if !filename.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read(filename).map_err(|e| {
            format!(
                "NeDB migration failed while reading {}: {e}",
                filename.display()
            )
        })?;
        // strip UTF-8 BOM if present (some editors/tools add one)
        let raw = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &raw[3..]
        } else {
            &raw[..]
        };
        let content = String::from_utf8_lossy(raw);
        let mut docs = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("$$$") {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(value) => docs.push(value),
                Err(e) => {
                    // The Electron `loadDatabase`/`find({})` also ignores lines
                    // that are not valid JSON documents; a single bad line must
                    // never abort the whole migration.
                    Logger::warn(
                        "NedbMigrationService",
                        &format!("Skipping unparsable line in {}: {e}", filename.display()),
                    );
                }
            }
        }
        Ok(docs)
    }

    fn normalize_server(source: &serde_json::Value) -> Result<OpenSourceFrpcDesktopServer, String> {
        let defaults = Self::create_default_server();
        let obj = source.as_object().ok_or("server must be an object")?;
        Ok(OpenSourceFrpcDesktopServer {
            id: "1".to_string(),
            frpc_version: match obj.get("frpcVersion") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(Self::integer(v, "server.frpcVersion", None, None)?),
            },
            multiuser: Self::boolean(obj.get("multiuser"), defaults.multiuser, "server.multiuser")?,
            user: Self::string(obj.get("user"), &defaults.user),
            server_addr: Self::string(obj.get("serverAddr"), &defaults.server_addr),
            server_port: Self::integer(
                obj.get("serverPort")
                    .unwrap_or(&serde_json::json!(defaults.server_port)),
                "server.serverPort",
                Some(1),
                Some(65535),
            )?,
            login_fail_exit: Self::boolean(
                obj.get("loginFailExit"),
                defaults.login_fail_exit,
                "server.loginFailExit",
            )?,
            udp_packet_size: Self::integer(
                obj.get("udpPacketSize")
                    .unwrap_or(&serde_json::json!(defaults.udp_packet_size)),
                "server.udpPacketSize",
                Some(1),
                None,
            )?,
            auth: Self::object(obj.get("auth"), &defaults.auth, "server.auth")?,
            log: Self::object(obj.get("log"), &defaults.log, "server.log")?,
            web_server: Self::object(
                obj.get("webServer"),
                &defaults.web_server,
                "server.webServer",
            )?,
            transport: Self::merge_transport(obj.get("transport"), &defaults.transport)?,
            metadatas: match obj.get("metadatas") {
                Some(v @ serde_json::Value::Object(_)) => v.clone(),
                _ => defaults.metadatas.clone(),
            },
            system: Self::normalize_system(obj.get("system"))?,
        })
    }

    fn normalize_proxy(source: &serde_json::Value, index: usize) -> Result<FrpcProxy, String> {
        let label = format!("proxy[{index}]");
        let obj = source.as_object().ok_or("proxy must be an object")?;
        let proxy_type = Self::string(obj.get("type"), "http");
        if !["http", "https", "tcp", "udp", "stcp", "xtcp", "sudp"].contains(&proxy_type.as_str()) {
            return Err(format!(
                "NeDB migration failed: unsupported type at {label}."
            ));
        }
        Ok(FrpcProxy {
            id: Self::identifier(obj.get("_id")),
            name: Self::string(obj.get("name"), ""),
            proxy_type,
            local_ip: Self::string(obj.get("localIP"), ""),
            local_port: Self::string(obj.get("localPort"), "8080"),
            remote_port: Self::string(obj.get("remotePort"), "8080"),
            custom_domains: Self::string_array(
                obj.get("customDomains"),
                &["".to_string()],
                &format!("{label}.customDomains"),
            )?,
            locations: Self::string_array(
                obj.get("locations"),
                &["".to_string()],
                &format!("{label}.locations"),
            )?,
            host_header_rewrite: Self::string(obj.get("hostHeaderRewrite"), ""),
            visitors_model: Self::string(obj.get("visitorsModel"), "visitors"),
            server_user: Self::string(obj.get("serverUser"), ""),
            server_name: Self::string(obj.get("serverName"), ""),
            secret_key: Self::string(obj.get("secretKey"), ""),
            bind_addr: Self::string(obj.get("bindAddr"), ""),
            bind_port: match obj.get("bindPort") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(Self::integer(
                    v,
                    &format!("{label}.bindPort"),
                    Some(1),
                    Some(65535),
                )?),
            },
            subdomain: Self::string(obj.get("subdomain"), ""),
            basic_auth: Self::boolean(obj.get("basicAuth"), false, &format!("{label}.basicAuth"))?,
            http_user: Self::string(obj.get("httpUser"), ""),
            http_password: Self::string(obj.get("httpPassword"), ""),
            fallback_to: Self::string(obj.get("fallbackTo"), ""),
            fallback_timeout_ms: Self::integer(
                obj.get("fallbackTimeoutMs")
                    .unwrap_or(&serde_json::json!(500)),
                &format!("{label}.fallbackTimeoutMs"),
                Some(0),
                None,
            )?,
            https2http: Self::boolean(
                obj.get("https2http"),
                false,
                &format!("{label}.https2http"),
            )?,
            https2http_ca_file: Self::string(obj.get("https2httpCaFile"), ""),
            https2http_key_file: Self::string(obj.get("https2httpKeyFile"), ""),
            keep_tunnel_open: Self::boolean(
                obj.get("keepTunnelOpen"),
                false,
                &format!("{label}.keepTunnelOpen"),
            )?,
            status: Self::integer(
                obj.get("status").unwrap_or(&serde_json::json!(1)),
                &format!("{label}.status"),
                Some(0),
                Some(1),
            )?,
            transport: Self::object(
                obj.get("transport"),
                &FrpcProxyTransportConfig {
                    use_encryption: false,
                    use_compression: false,
                    proxy_protocol_version: "".to_string(),
                },
                &format!("{label}.transport"),
            )?,
        })
    }

    fn normalize_version(source: &serde_json::Value, index: usize) -> Result<FrpcVersion, String> {
        let label = format!("version[{index}]");
        let obj = source.as_object().ok_or("version must be an object")?;
        Ok(FrpcVersion {
            id: Self::identifier(obj.get("_id")),
            github_release_id: Self::integer(
                obj.get("githubReleaseId")
                    .unwrap_or(&serde_json::Value::Null),
                &format!("{label}.githubReleaseId"),
                None,
                None,
            )?,
            github_asset_id: Self::integer(
                obj.get("githubAssetId").unwrap_or(&serde_json::Value::Null),
                &format!("{label}.githubAssetId"),
                None,
                None,
            )?,
            github_created_at: Self::string(obj.get("githubCreatedAt"), ""),
            name: Self::string(obj.get("name"), ""),
            asset_name: Self::string(obj.get("assetName"), ""),
            version_download_count: Self::integer(
                obj.get("versionDownloadCount")
                    .unwrap_or(&serde_json::json!(0)),
                &format!("{label}.versionDownloadCount"),
                Some(0),
                None,
            )?,
            asset_download_count: Self::integer(
                obj.get("assetDownloadCount")
                    .unwrap_or(&serde_json::json!(0)),
                &format!("{label}.assetDownloadCount"),
                Some(0),
                None,
            )?,
            browser_download_url: Self::string(obj.get("browserDownloadUrl"), ""),
            downloaded: Self::boolean(obj.get("downloaded"), true, &format!("{label}.downloaded"))?,
            local_path: match obj.get("localPath") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(Self::string(Some(v), "")),
            },
            size: Self::string(obj.get("size"), ""),
        })
    }

    fn create_default_server() -> OpenSourceFrpcDesktopServer {
        OpenSourceFrpcDesktopServer::default_server()
    }

    fn normalize_system(
        source: Option<&serde_json::Value>,
    ) -> Result<FrpcSystemConfiguration, String> {
        let system = match source {
            Some(serde_json::Value::Object(obj)) => obj,
            _ => {
                return Ok(FrpcSystemConfiguration {
                    launch_at_startup: false,
                    silent_startup: false,
                    auto_connect_on_startup: false,
                    language: "en-US".to_string(),
                })
            }
        };
        Ok(FrpcSystemConfiguration {
            launch_at_startup: Self::boolean(
                system.get("launchAtStartup"),
                false,
                "server.system.launchAtStartup",
            )?,
            silent_startup: Self::boolean(
                system.get("silentStartup"),
                false,
                "server.system.silentStartup",
            )?,
            auto_connect_on_startup: Self::boolean(
                system.get("autoConnectOnStartup"),
                false,
                "server.system.autoConnectOnStartup",
            )?,
            language: Self::string(system.get("language"), "en-US"),
        })
    }

    fn merge_transport(
        source: Option<&serde_json::Value>,
        fallback: &TransportConfig,
    ) -> Result<TransportConfig, String> {
        // `{ ...fallback, ...source }` semantics like the Electron version:
        // missing keys keep the default, present keys override.
        let merged = Self::merge_object(
            source,
            &serde_json::to_value(fallback).unwrap_or_default(),
            "server.transport",
        )?;
        let transport: TransportConfig = serde_json::from_value(merged).map_err(|_| {
            "NeDB migration failed: server.transport must be an object.".to_string()
        })?;
        let tls_merged = match source.and_then(|v| v.get("tls")) {
            Some(tls) => Self::merge_object(
                Some(tls),
                &serde_json::to_value(&fallback.tls).unwrap_or_default(),
                "server.transport.tls",
            )?,
            None => serde_json::to_value(&fallback.tls).unwrap_or_default(),
        };
        let tls: TransportTlsConfig = serde_json::from_value(tls_merged).map_err(|_| {
            "NeDB migration failed: server.transport.tls must be an object.".to_string()
        })?;
        Ok(TransportConfig { tls, ..transport })
    }

    /// `{ ...fallback, ...value }` shallow merge, preserving fallback defaults.
    fn merge_object(
        value: Option<&serde_json::Value>,
        fallback: &serde_json::Value,
        label: &str,
    ) -> Result<serde_json::Value, String> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(fallback.clone()),
            Some(serde_json::Value::Object(src)) => {
                let mut merged = fallback.as_object().cloned().unwrap_or_default();
                for (k, v) in src {
                    merged.insert(k.clone(), v.clone());
                }
                Ok(serde_json::Value::Object(merged))
            }
            Some(_) => Err(format!("NeDB migration failed: {label} must be an object.")),
        }
    }

    fn object<T: serde::de::DeserializeOwned + Clone + serde::Serialize>(
        value: Option<&serde_json::Value>,
        fallback: &T,
        label: &str,
    ) -> Result<T, String> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(fallback.clone()),
            Some(serde_json::Value::Object(_)) => {
                let merged = Self::merge_object(
                    value,
                    &serde_json::to_value(fallback).unwrap_or_default(),
                    label,
                )?;
                serde_json::from_value(merged)
                    .map_err(|_| format!("NeDB migration failed: {label} must be an object."))
            }
            Some(_) => Err(format!("NeDB migration failed: {label} must be an object.")),
        }
    }

    fn string_array(
        value: Option<&serde_json::Value>,
        fallback: &[String],
        label: &str,
    ) -> Result<Vec<String>, String> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(fallback.to_vec()),
            Some(serde_json::Value::Array(items)) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    if let serde_json::Value::String(s) = item {
                        out.push(s.clone());
                    } else {
                        return Err(format!(
                            "NeDB migration failed: {label} must be a string array."
                        ));
                    }
                }
                Ok(out)
            }
            Some(_) => Err(format!(
                "NeDB migration failed: {label} must be a string array."
            )),
        }
    }

    fn boolean(
        value: Option<&serde_json::Value>,
        fallback: bool,
        label: &str,
    ) -> Result<bool, String> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(fallback),
            Some(serde_json::Value::Bool(b)) => Ok(*b),
            Some(serde_json::Value::Number(n)) => {
                if let Some(1) = n.as_i64() {
                    Ok(true)
                } else if let Some(0) = n.as_i64() {
                    Ok(false)
                } else {
                    Err(format!("NeDB migration failed: {label} must be boolean."))
                }
            }
            Some(_) => Err(format!("NeDB migration failed: {label} must be boolean.")),
        }
    }

    fn integer(
        value: &serde_json::Value,
        label: &str,
        minimum: Option<i64>,
        maximum: Option<i64>,
    ) -> Result<i64, String> {
        let parsed = match value {
            serde_json::Value::Number(n) => n.as_i64(),
            serde_json::Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        };
        match parsed {
            Some(v)
                if minimum.map_or(true, |min| v >= min) && maximum.map_or(true, |max| v <= max) =>
            {
                Ok(v)
            }
            _ => Err(format!("NeDB migration failed: {label} is invalid.")),
        }
    }

    fn string(value: Option<&serde_json::Value>, fallback: &str) -> String {
        match value {
            None | Some(serde_json::Value::Null) => fallback.to_string(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        }
    }

    fn identifier(value: Option<&serde_json::Value>) -> String {
        let id = Self::string(value, "").trim().to_string();
        if id.is_empty() {
            IdUtils::gen_uuid()
        } else {
            id
        }
    }

    fn assert_unique(values: impl Iterator<Item = String>, label: &str) -> Result<(), String> {
        let mut seen = HashSet::new();
        for value in values {
            if !seen.insert(value) {
                return Err(format!("NeDB migration failed: duplicate {label}."));
            }
        }
        Ok(())
    }

    fn verify_imported_ids<T: serde::Serialize>(
        &self,
        db: &Connection,
        table_name: &str,
        entities: &[T],
    ) -> Result<(), String> {
        if entities.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = entities
            .iter()
            .map(|e| {
                serde_json::to_value(e)
                    .ok()
                    .and_then(|v| v.get("_id").and_then(|i| i.as_str()).map(|s| s.to_string()))
                    .unwrap_or_default()
            })
            .collect();
        let mut stmt = db
            .prepare(&format!("SELECT id FROM {table_name}"))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut actual: HashSet<String> = HashSet::new();
        for row in rows {
            actual.insert(row.map_err(|e| e.to_string())?);
        }
        if ids.iter().any(|id| !actual.contains(id)) {
            return Err(format!(
                "NeDB migration failed: {table_name} id validation failed."
            ));
        }
        Ok(())
    }

    fn backup_files(files: &[PathBuf]) {
        let timestamp = chrono::Utc::now()
            .format("%Y-%m-%dT%H-%M-%S%.3f")
            .to_string();
        for filename in files {
            let backup = filename.with_extension(format!(
                "{}.migrated-{}.bak",
                filename
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default(),
                timestamp
            ));
            match fs::rename(filename, &backup) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(0o444));
                    }
                }
                Err(e) => {
                    Logger::warn(
                        "NedbMigrationService",
                        &format!(
                            "Could not archive migrated NeDB file {} ({}).",
                            filename.display(),
                            e
                        ),
                    );
                }
            }
        }
    }
}
