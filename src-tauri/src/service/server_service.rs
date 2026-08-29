//! Server configuration service (ported from electron/service/ServerService.ts).
//!
//! Responsible for saving/loading the server config, generating the frpc TOML
//! config file and importing TOML configs.

use std::fs;

use toml::Value as TomlValue;

use crate::core::business_error::{BusinessError, ResponseCode};
use crate::core::constants::GlobalConstant;
use crate::core::logger::Logger;
use crate::core::paths::PathUtils;
use crate::db::proxy_repository::ProxyRepository;
use crate::db::server_repository::ServerRepository;
use crate::model::frp::{
    FrpcProxy, FrpcProxyTransportConfig, FrpcSystemConfiguration, OpenSourceFrpcDesktopServer,
    TransportConfig,
};

fn str_val(s: &str) -> TomlValue {
    TomlValue::String(s.to_string())
}

fn int_val(i: i64) -> TomlValue {
    TomlValue::Integer(i)
}

fn bool_val(b: bool) -> TomlValue {
    TomlValue::Boolean(b)
}

#[derive(Clone)]
pub struct ServerService {
    server_repo: ServerRepository,
    proxy_repo: ProxyRepository,
}

impl ServerService {
    pub fn new(server_repo: ServerRepository, proxy_repo: ProxyRepository) -> Self {
        Self {
            server_repo,
            proxy_repo,
        }
    }

    pub async fn save_server_config(
        &self,
        mut frpc_server: OpenSourceFrpcDesktopServer,
    ) -> Result<OpenSourceFrpcDesktopServer, BusinessError> {
        frpc_server.id = "1".to_string();
        let new_config = self
            .server_repo
            .update_by_id("1", &frpc_server)
            .map_err(|e| BusinessError::internal(format!("save config failed: {e}")))?;
        // autostart is handled by the app layer via tauri-plugin-autostart
        Logger::set_level(&new_config.log.level);
        Ok(new_config)
    }

    pub async fn get_server_config(&self) -> Result<OpenSourceFrpcDesktopServer, BusinessError> {
        self.server_repo
            .find_by_id("1")
            .map_err(|e| BusinessError::internal(format!("load config failed: {e}")))?
            .ok_or_else(|| BusinessError::new(ResponseCode::InternalError))
    }

    pub async fn has_server_config(&self) -> Result<bool, BusinessError> {
        if self
            .server_repo
            .exists("1")
            .map_err(|e| BusinessError::internal(format!("check config failed: {e}")))?
        {
            let config = self.get_server_config().await?;
            Ok(!config.server_addr.is_empty())
        } else {
            Ok(false)
        }
    }

    fn is_range_port(proxy: &FrpcProxy) -> bool {
        (proxy.proxy_type == "tcp" || proxy.proxy_type == "udp")
            && (proxy.local_port.contains('-') || proxy.local_port.contains(','))
    }

    fn is_visitors(proxy: &FrpcProxy) -> bool {
        (proxy.proxy_type == "stcp" || proxy.proxy_type == "sudp" || proxy.proxy_type == "xtcp")
            && proxy.visitors_model == "visitors"
    }

    fn is_enable_proxy(proxy: &FrpcProxy) -> bool {
        proxy.status == 1
    }

    /// Generate the frpc TOML config file at `output_path`.
    ///
    /// Semantics must match the Electron `genTomlConfig` exactly:
    /// - common config keys keep camelCase (frpc's TOML format),
    /// - `log.to` is overridden with the frpc log file path,
    /// - `loginFailExit` forced to `false`,
    /// - `webServer.addr` forced to `127.0.0.1`,
    /// - `auth` omitted when `auth.method == "none"`,
    /// - enabled proxies/visitors appended as `[[proxies]]` / `[[visitors]]`,
    /// - range-port proxies appended as a Go-template fragment (frpc renders it).
    pub async fn gen_toml_config(&self, output_path: &str) -> Result<(), BusinessError> {
        if output_path.is_empty() {
            return Ok(());
        }
        let server = self.get_server_config().await?;
        let proxies = self
            .proxy_repo
            .find_all()
            .map_err(|e| BusinessError::internal(format!("load proxies failed: {e}")))?;

        let mut common = toml::map::Map::new();
        common.insert("user".into(), str_val(&server.user));
        common.insert("serverAddr".into(), str_val(&server.server_addr));
        common.insert("serverPort".into(), int_val(server.server_port));
        common.insert(
            "loginFailExit".into(),
            bool_val(GlobalConstant::FRPC_LOGIN_FAIL_EXIT),
        );
        common.insert("udpPacketSize".into(), int_val(server.udp_packet_size));

        // log
        let mut log = toml::map::Map::new();
        log.insert(
            "to".into(),
            str_val(&PathUtils::get_frpc_log_file_path().to_string_lossy()),
        );
        log.insert("level".into(), str_val(&server.log.level));
        log.insert("maxDays".into(), int_val(server.log.max_days));
        log.insert(
            "disablePrintColor".into(),
            bool_val(server.log.disable_print_color),
        );
        common.insert("log".into(), TomlValue::Table(log));

        // auth (omit when method == "none")
        if server.auth.method != "none" {
            let mut auth = toml::map::Map::new();
            auth.insert("method".into(), str_val(&server.auth.method));
            auth.insert("token".into(), str_val(&server.auth.token));
            common.insert("auth".into(), TomlValue::Table(auth));
        }

        // webServer
        let mut web_server = toml::map::Map::new();
        web_server.insert("addr".into(), str_val(GlobalConstant::LOCAL_IP));
        web_server.insert("port".into(), int_val(server.web_server.port));
        web_server.insert("user".into(), str_val(&server.web_server.user));
        web_server.insert("password".into(), str_val(&server.web_server.password));
        web_server.insert(
            "pprofEnable".into(),
            bool_val(server.web_server.pprof_enable),
        );
        common.insert("webServer".into(), TomlValue::Table(web_server));

        // transport (with tls sub-table)
        common.insert("transport".into(), transport_to_toml(&server.transport));

        // metadatas
        common.insert(
            "metadatas".into(),
            serde_json_to_toml(&server.metadatas)
                .unwrap_or_else(|| TomlValue::Table(toml::map::Map::new())),
        );

        // proxies
        let mut enabled_proxies: Vec<TomlValue> = Vec::new();
        let mut enabled_range_port_proxies: Vec<String> = Vec::new();
        for proxy in proxies
            .iter()
            .filter(|p| Self::is_enable_proxy(p) && !Self::is_visitors(p))
        {
            if Self::is_range_port(proxy) {
                enabled_range_port_proxies.push(Self::range_port_template(proxy));
            } else {
                enabled_proxies.push(proxy_to_toml(proxy));
            }
        }
        if !enabled_proxies.is_empty() {
            common.insert("proxies".into(), TomlValue::Array(enabled_proxies));
        }

        // visitors
        let enabled_visitors: Vec<TomlValue> = proxies
            .iter()
            .filter(|p| Self::is_enable_proxy(p) && Self::is_visitors(p))
            .map(visitor_to_toml)
            .collect();
        if !enabled_visitors.is_empty() {
            common.insert("visitors".into(), TomlValue::Array(enabled_visitors));
        }

        let mut final_toml = toml::to_string(&TomlValue::Table(common))
            .map_err(|e| BusinessError::internal(format!("TOML serialize failed: {e}")))?;
        for block in &enabled_range_port_proxies {
            final_toml.push('\n');
            final_toml.push_str(block);
        }

        fs::write(output_path, final_toml)
            .map_err(|e| BusinessError::internal(format!("write config failed: {e}")))?;
        Ok(())
    }

    /// Build the Go-template fragment for a range-port proxy, byte-identical to
    /// the Electron template literal.
    fn range_port_template(proxy: &FrpcProxy) -> String {
        format!(
            r#"
{{{{- range $_, $v := parseNumberRangePair "{}" "{}" }}}}
[[proxies]]

type = "{}"
name = "{}-{{{{ $v.First }}}}"
localIP = "{}"
localPort = {{{{ $v.First }}}}
remotePort = {{{{ $v.Second }}}}
{{{{- end }}}}
"#,
            proxy.local_port, proxy.remote_port, proxy.proxy_type, proxy.name, proxy.local_ip
        )
    }

    /// Import a frpc TOML config file into the server config and proxies.
    /// Returns `{ canceled, path }` mirroring the Electron response.
    pub async fn import_toml_config(
        &self,
        file_path: &str,
    ) -> Result<serde_json::Value, BusinessError> {
        let toml_data = fs::read_to_string(file_path)
            .map_err(|e| BusinessError::internal(format!("read toml failed: {e}")))?;
        let source: toml::Table = toml::from_str(&toml_data)
            .map_err(|e| BusinessError::internal(format!("parse toml failed: {e}")))?;

        let mut config = OpenSourceFrpcDesktopServer::default_server();

        if let Some(v) = source.get("loginFailExit") {
            config.login_fail_exit = toml_bool(v);
        }
        if let Some(v) = source.get("udpPacketSize") {
            config.udp_packet_size = toml_i64(v);
        }
        if let Some(v) = source.get("serverAddr") {
            config.server_addr = toml_str(v);
        }
        if let Some(v) = source.get("serverPort") {
            config.server_port = toml_i64(v);
        }
        if let Some(v) = source.get("user") {
            config.user = toml_str(v);
        }

        if let Some(auth) = source.get("auth").and_then(|v| v.as_table()) {
            if let Some(v) = auth.get("method") {
                config.auth.method = toml_str(v);
            }
            if let Some(v) = auth.get("token") {
                config.auth.token = toml_str(v);
            }
        }

        if let Some(log) = source.get("log").and_then(|v| v.as_table()) {
            if let Some(v) = log.get("to") {
                config.log.to = toml_str(v);
            }
            if let Some(v) = log.get("level") {
                config.log.level = toml_str(v);
            }
            if let Some(v) = log.get("maxDays") {
                config.log.max_days = toml_i64(v);
            }
            if let Some(v) = log.get("disablePrintColor") {
                config.log.disable_print_color = toml_bool(v);
            }
        }

        if let Some(transport) = source.get("transport").and_then(|v| v.as_table()) {
            if let Some(v) = transport.get("dialServerTimeout") {
                config.transport.dial_server_timeout = toml_i64(v);
            }
            if let Some(v) = transport.get("dialServerKeepalive") {
                config.transport.dial_server_keepalive = toml_i64(v);
            }
            if let Some(v) = transport.get("poolCount") {
                config.transport.pool_count = toml_i64(v);
            }
            if let Some(v) = transport.get("tcpMux") {
                config.transport.tcp_mux = toml_bool(v);
            }
            if let Some(v) = transport.get("tcpMuxKeepaliveInterval") {
                config.transport.tcp_mux_keepalive_interval = toml_i64(v);
            }
            if let Some(v) = transport.get("protocol") {
                config.transport.protocol = toml_str(v);
            }
            if let Some(v) = transport.get("connectServerLocalIP") {
                config.transport.connect_server_local_ip = toml_str(v);
            }
            if let Some(v) = transport.get("proxyURL") {
                config.transport.proxy_url = toml_str(v);
            }
            if let Some(v) = transport.get("heartbeatInterval") {
                config.transport.heartbeat_interval = toml_i64(v);
            }
            if let Some(v) = transport.get("heartbeatTimeout") {
                config.transport.heartbeat_timeout = toml_i64(v);
            }
            if let Some(tls) = transport.get("tls").and_then(|v| v.as_table()) {
                if let Some(v) = tls.get("enable") {
                    config.transport.tls.enable = toml_bool(v);
                }
                if let Some(v) = tls.get("certFile") {
                    config.transport.tls.cert_file = toml_str(v);
                }
                if let Some(v) = tls.get("keyFile") {
                    config.transport.tls.key_file = toml_str(v);
                }
                if let Some(v) = tls.get("trustedCaFile") {
                    config.transport.tls.trusted_ca_file = toml_str(v);
                }
                if let Some(v) = tls.get("serverName") {
                    config.transport.tls.server_name = toml_str(v);
                }
                if let Some(v) = tls.get("disableCustomTLSFirstByte") {
                    config.transport.tls.disable_custom_tls_first_byte = toml_bool(v);
                }
            }
        }

        if let Some(metadatas) = source.get("metadatas").and_then(|v| v.as_table()) {
            if let Some(v) = metadatas.get("token") {
                if let Some(obj) = config.metadatas.as_object_mut() {
                    obj.insert("token".into(), serde_json::json!(toml_str(v)));
                }
            }
        }

        if let Some(web_server) = source.get("webServer").and_then(|v| v.as_table()) {
            if let Some(v) = web_server.get("addr") {
                config.web_server.addr = toml_str(v);
            }
            if let Some(v) = web_server.get("port") {
                config.web_server.port = toml_i64(v);
            }
            if let Some(v) = web_server.get("user") {
                config.web_server.user = toml_str(v);
            }
            if let Some(v) = web_server.get("password") {
                config.web_server.password = toml_str(v);
            }
            if let Some(v) = web_server.get("pprofEnable") {
                config.web_server.pprof_enable = toml_bool(v);
            }
        }

        self.save_server_config(config).await?;

        if let Some(proxies) = source.get("proxies").and_then(|v| v.as_array()) {
            let mut parsed: Vec<FrpcProxy> = Vec::new();
            for proxy in proxies {
                if let Some(table) = proxy.as_table() {
                    parsed.push(proxy_from_toml(table));
                }
            }
            let mut entities: Vec<FrpcProxy> = parsed;
            self.proxy_repo
                .insert_many(&mut entities)
                .map_err(|e| BusinessError::internal(format!("save proxies failed: {e}")))?;
        }

        if let Some(visitors) = source.get("visitors").and_then(|v| v.as_array()) {
            let mut parsed: Vec<FrpcProxy> = Vec::new();
            for visitor in visitors {
                if let Some(table) = visitor.as_table() {
                    parsed.push(visitor_from_toml(table));
                }
            }
            let mut entities: Vec<FrpcProxy> = parsed;
            self.proxy_repo
                .insert_many(&mut entities)
                .map_err(|e| BusinessError::internal(format!("save visitors failed: {e}")))?;
        }

        Ok(serde_json::json!({
            "canceled": false,
            "path": file_path
        }))
    }

    pub async fn is_silent_start(&self) -> Result<bool, BusinessError> {
        let config = self.get_server_config().await?;
        Ok(config.system.silent_startup)
    }

    pub async fn is_auto_connect_on_startup(&self) -> Result<bool, BusinessError> {
        let config = self.get_server_config().await?;
        Ok(config.system.auto_connect_on_startup)
    }

    pub async fn get_logger_level(&self) -> Result<String, BusinessError> {
        let config = self.get_server_config().await?;
        Ok(config.log.level)
    }

    pub async fn get_language(&self) -> Result<String, BusinessError> {
        match self.get_server_config().await {
            Ok(config) if !config.system.language.is_empty() => Ok(config.system.language),
            _ => Ok(GlobalConstant::DEFAULT_LANGUAGE.to_string()),
        }
    }

    pub async fn save_language(&self, language: &str) -> Result<(), BusinessError> {
        let mut config = match self.get_server_config().await {
            Ok(c) => c,
            Err(_) => OpenSourceFrpcDesktopServer::default_server(),
        };
        config.system.language = language.to_string();
        self.save_server_config(config).await?;
        Ok(())
    }

    pub async fn save_system_config(
        &self,
        system: FrpcSystemConfiguration,
    ) -> Result<(), BusinessError> {
        let mut config = self.get_server_config().await?;
        config.system = system;
        self.save_server_config(config).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TOML conversion helpers
// ---------------------------------------------------------------------------

fn transport_to_toml(transport: &TransportConfig) -> TomlValue {
    let mut t = toml::map::Map::new();
    t.insert(
        "dialServerTimeout".into(),
        int_val(transport.dial_server_timeout),
    );
    t.insert(
        "dialServerKeepalive".into(),
        int_val(transport.dial_server_keepalive),
    );
    t.insert("poolCount".into(), int_val(transport.pool_count));
    t.insert("tcpMux".into(), bool_val(transport.tcp_mux));
    t.insert(
        "tcpMuxKeepaliveInterval".into(),
        int_val(transport.tcp_mux_keepalive_interval),
    );
    t.insert("protocol".into(), str_val(&transport.protocol));
    t.insert(
        "connectServerLocalIP".into(),
        str_val(&transport.connect_server_local_ip),
    );
    t.insert("proxyURL".into(), str_val(&transport.proxy_url));

    let mut tls = toml::map::Map::new();
    tls.insert("enable".into(), bool_val(transport.tls.enable));
    tls.insert("certFile".into(), str_val(&transport.tls.cert_file));
    tls.insert("keyFile".into(), str_val(&transport.tls.key_file));
    tls.insert(
        "trustedCaFile".into(),
        str_val(&transport.tls.trusted_ca_file),
    );
    tls.insert("serverName".into(), str_val(&transport.tls.server_name));
    tls.insert(
        "disableCustomTLSFirstByte".into(),
        bool_val(transport.tls.disable_custom_tls_first_byte),
    );
    t.insert("tls".into(), TomlValue::Table(tls));

    t.insert(
        "heartbeatInterval".into(),
        int_val(transport.heartbeat_interval),
    );
    t.insert(
        "heartbeatTimeout".into(),
        int_val(transport.heartbeat_timeout),
    );
    TomlValue::Table(t)
}

fn proxy_to_toml(proxy: &FrpcProxy) -> TomlValue {
    let mut p = toml::map::Map::new();
    p.insert("name".into(), str_val(&proxy.name));
    p.insert("type".into(), str_val(&proxy.proxy_type));
    match proxy.proxy_type.as_str() {
        "tcp" | "udp" => {
            p.insert("localIP".into(), str_val(&proxy.local_ip));
            p.insert(
                "localPort".into(),
                int_val(proxy.local_port.parse().unwrap_or(0)),
            );
            p.insert(
                "remotePort".into(),
                int_val(proxy.remote_port.parse().unwrap_or(0)),
            );
            let mut transport = toml::map::Map::new();
            transport.insert(
                "useEncryption".into(),
                bool_val(proxy.transport.use_encryption),
            );
            transport.insert(
                "useCompression".into(),
                bool_val(proxy.transport.use_compression),
            );
            transport.insert(
                "proxyProtocolVersion".into(),
                str_val(&proxy.transport.proxy_protocol_version),
            );
            p.insert("transport".into(), TomlValue::Table(transport));
        }
        "http" | "https" => {
            let locations: Vec<String> = proxy
                .locations
                .iter()
                .filter(|l| !l.is_empty())
                .cloned()
                .collect();
            if proxy.https2http && proxy.proxy_type == "https" {
                p.insert(
                    "customDomains".into(),
                    TomlValue::Array(proxy.custom_domains.iter().map(|s| str_val(s)).collect()),
                );
                p.insert("subdomain".into(), str_val(&proxy.subdomain));
                if !locations.is_empty() {
                    p.insert(
                        "locations".into(),
                        TomlValue::Array(locations.iter().map(|s| str_val(s)).collect()),
                    );
                }
                if proxy.https2http {
                    let mut plugin = toml::map::Map::new();
                    plugin.insert("type".into(), str_val("https2http"));
                    plugin.insert(
                        "localAddr".into(),
                        str_val(&format!("{}:{}", proxy.local_ip, proxy.local_port)),
                    );
                    plugin.insert("crtPath".into(), str_val(&proxy.https2http_ca_file));
                    plugin.insert("keyPath".into(), str_val(&proxy.https2http_key_file));
                    p.insert("plugin".into(), TomlValue::Table(plugin));
                }
            } else {
                p.insert("localIP".into(), str_val(&proxy.local_ip));
                p.insert(
                    "localPort".into(),
                    int_val(proxy.local_port.parse().unwrap_or(0)),
                );
                p.insert(
                    "customDomains".into(),
                    TomlValue::Array(proxy.custom_domains.iter().map(|s| str_val(s)).collect()),
                );
                p.insert("subdomain".into(), str_val(&proxy.subdomain));
                if !locations.is_empty() {
                    p.insert(
                        "locations".into(),
                        TomlValue::Array(locations.iter().map(|s| str_val(s)).collect()),
                    );
                }
                if proxy.basic_auth {
                    p.insert("httpUser".into(), str_val(&proxy.http_user));
                    p.insert("httpPassword".into(), str_val(&proxy.http_password));
                }
            }
        }
        "stcp" | "xtcp" | "sudp" => {
            p.insert("localIP".into(), str_val(&proxy.local_ip));
            p.insert(
                "localPort".into(),
                int_val(proxy.local_port.parse().unwrap_or(0)),
            );
            p.insert("secretKey".into(), str_val(&proxy.secret_key));
        }
        _ => {}
    }
    TomlValue::Table(p)
}

fn visitor_to_toml(proxy: &FrpcProxy) -> TomlValue {
    let mut p = toml::map::Map::new();
    p.insert("name".into(), str_val(&proxy.name));
    p.insert("type".into(), str_val(&proxy.proxy_type));
    p.insert("serverName".into(), str_val(&proxy.server_name));
    p.insert("secretKey".into(), str_val(&proxy.secret_key));
    p.insert("bindAddr".into(), str_val(&proxy.bind_addr));
    if let Some(port) = proxy.bind_port {
        p.insert("bindPort".into(), int_val(port));
    }
    if proxy.proxy_type == "xtcp" {
        if !proxy.server_user.is_empty() {
            p.insert("serverUser".into(), str_val(&proxy.server_user));
        }
        p.insert("keepTunnelOpen".into(), bool_val(proxy.keep_tunnel_open));
        p.insert("fallbackTo".into(), str_val(&proxy.fallback_to));
        p.insert(
            "fallbackTimeoutMs".into(),
            int_val(proxy.fallback_timeout_ms),
        );
    } else if !proxy.server_user.is_empty() {
        p.insert("serverUser".into(), str_val(&proxy.server_user));
    }
    TomlValue::Table(p)
}

fn serde_json_to_toml(value: &serde_json::Value) -> Option<TomlValue> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(TomlValue::Boolean(*b)),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(TomlValue::Integer)
            .or_else(|| n.as_f64().map(TomlValue::Float)),
        serde_json::Value::String(s) => Some(TomlValue::String(s.clone())),
        serde_json::Value::Array(items) => {
            let arr: Vec<TomlValue> = items.iter().filter_map(serde_json_to_toml).collect();
            Some(TomlValue::Array(arr))
        }
        serde_json::Value::Object(map) => {
            let mut table = toml::map::Map::new();
            for (k, v) in map {
                if let Some(tv) = serde_json_to_toml(v) {
                    table.insert(k.clone(), tv);
                }
            }
            Some(TomlValue::Table(table))
        }
    }
}

fn toml_str(v: &TomlValue) -> String {
    match v {
        TomlValue::String(s) => s.clone(),
        TomlValue::Integer(i) => i.to_string(),
        TomlValue::Float(f) => f.to_string(),
        TomlValue::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

fn toml_i64(v: &TomlValue) -> i64 {
    match v {
        TomlValue::Integer(i) => *i,
        TomlValue::String(s) => s.parse().unwrap_or(0),
        TomlValue::Float(f) => *f as i64,
        _ => 0,
    }
}

fn toml_bool(v: &TomlValue) -> bool {
    match v {
        TomlValue::Boolean(b) => *b,
        TomlValue::Integer(i) => *i != 0,
        TomlValue::String(s) => s == "true",
        _ => false,
    }
}

/// Build a FrpcProxy from an imported TOML proxy table with defaults matching
/// the Electron `importTomlConfig` proxy defaults.
fn proxy_from_toml(table: &toml::Table) -> FrpcProxy {
    let mut proxy = default_imported_proxy();
    if let Some(v) = table.get("name") {
        proxy.name = toml_str(v);
    }
    if let Some(v) = table.get("type") {
        proxy.proxy_type = toml_str(v);
    }
    if let Some(v) = table.get("localIP") {
        proxy.local_ip = toml_str(v);
    }
    if let Some(v) = table.get("localPort") {
        proxy.local_port = toml_i64(v).to_string();
    }
    if let Some(v) = table.get("remotePort") {
        proxy.remote_port = toml_i64(v).to_string();
    }
    if let Some(v) = table.get("customDomains") {
        proxy.custom_domains = toml_string_array(v);
    }
    if let Some(v) = table.get("subdomain") {
        proxy.subdomain = toml_str(v);
    }
    if let Some(v) = table.get("locations") {
        proxy.locations = toml_string_array(v);
    }
    if let Some(v) = table.get("hostHeaderRewrite") {
        proxy.host_header_rewrite = toml_str(v);
    }
    if let Some(v) = table.get("httpUser") {
        proxy.http_user = toml_str(v);
    }
    if let Some(v) = table.get("httpPassword") {
        proxy.http_password = toml_str(v);
    }
    if let Some(v) = table.get("serverName") {
        proxy.server_name = toml_str(v);
    }
    if let Some(v) = table.get("serverUser") {
        proxy.server_user = toml_str(v);
    }
    if let Some(v) = table.get("secretKey") {
        proxy.secret_key = toml_str(v);
    }
    if let Some(v) = table.get("bindAddr") {
        proxy.bind_addr = toml_str(v);
    }
    if let Some(v) = table.get("bindPort") {
        proxy.bind_port = Some(toml_i64(v));
    }
    if let Some(v) = table.get("fallbackTo") {
        proxy.fallback_to = toml_str(v);
    }
    if let Some(v) = table.get("fallbackTimeoutMs") {
        proxy.fallback_timeout_ms = toml_i64(v);
    }
    if let Some(v) = table.get("keepTunnelOpen") {
        proxy.keep_tunnel_open = toml_bool(v);
    }
    if let Some(transport) = table.get("transport").and_then(|v| v.as_table()) {
        if let Some(v) = transport.get("useEncryption") {
            proxy.transport.use_encryption = toml_bool(v);
        }
        if let Some(v) = transport.get("useCompression") {
            proxy.transport.use_compression = toml_bool(v);
        }
        if let Some(v) = transport.get("proxyProtocolVersion") {
            proxy.transport.proxy_protocol_version = toml_str(v);
        }
    }
    proxy
}

/// Build a FrpcProxy (visitor) from an imported TOML visitor table.
fn visitor_from_toml(table: &toml::Table) -> FrpcProxy {
    let mut visitor = default_imported_proxy();
    visitor.visitors_model = "visitors".to_string();
    if let Some(v) = table.get("name") {
        visitor.name = toml_str(v);
    }
    if let Some(v) = table.get("type") {
        visitor.proxy_type = toml_str(v);
    }
    if let Some(v) = table.get("serverName") {
        visitor.server_name = toml_str(v);
    }
    if let Some(v) = table.get("serverUser") {
        visitor.server_user = toml_str(v);
    }
    if let Some(v) = table.get("secretKey") {
        visitor.secret_key = toml_str(v);
    }
    if let Some(v) = table.get("bindAddr") {
        visitor.bind_addr = toml_str(v);
    }
    if let Some(v) = table.get("bindPort") {
        visitor.bind_port = Some(toml_i64(v));
    }
    if let Some(transport) = table.get("transport").and_then(|v| v.as_table()) {
        if let Some(v) = transport.get("useEncryption") {
            visitor.transport.use_encryption = toml_bool(v);
        }
        if let Some(v) = transport.get("useCompression") {
            visitor.transport.use_compression = toml_bool(v);
        }
        if let Some(v) = transport.get("proxyProtocolVersion") {
            visitor.transport.proxy_protocol_version = toml_str(v);
        }
    }
    visitor
}

fn default_imported_proxy() -> FrpcProxy {
    FrpcProxy {
        id: String::new(),
        name: String::new(),
        proxy_type: "http".to_string(),
        local_ip: String::new(),
        local_port: "8080".to_string(),
        remote_port: "8080".to_string(),
        custom_domains: vec!["".to_string()],
        locations: vec!["".to_string()],
        host_header_rewrite: String::new(),
        visitors_model: "visitors".to_string(),
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

fn toml_string_array(v: &TomlValue) -> Vec<String> {
    match v {
        TomlValue::Array(items) => items.iter().map(toml_str).collect(),
        _ => vec!["".to_string()],
    }
}
