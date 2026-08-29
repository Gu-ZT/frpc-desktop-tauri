//! FRP configuration models (ported from types/frp.d.ts).
//!
//! Serde field names match the TypeScript types exactly (camelCase) so that:
//! 1. IPC JSON payloads stay identical to the Electron version, and
//! 2. TOML serialization produces the camelCase keys frpc expects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogConfig {
    pub to: String,
    pub level: String,
    #[serde(rename = "maxDays")]
    pub max_days: i64,
    #[serde(rename = "disablePrintColor")]
    pub disable_print_color: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    pub method: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebServerConfig {
    pub addr: String,
    pub port: i64,
    pub user: String,
    pub password: String,
    #[serde(rename = "pprofEnable")]
    pub pprof_enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransportTlsConfig {
    pub enable: bool,
    #[serde(rename = "certFile")]
    pub cert_file: String,
    #[serde(rename = "keyFile")]
    pub key_file: String,
    #[serde(rename = "trustedCaFile")]
    pub trusted_ca_file: String,
    #[serde(rename = "serverName")]
    pub server_name: String,
    #[serde(rename = "disableCustomTLSFirstByte")]
    pub disable_custom_tls_first_byte: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransportConfig {
    #[serde(rename = "dialServerTimeout")]
    pub dial_server_timeout: i64,
    #[serde(rename = "dialServerKeepalive")]
    pub dial_server_keepalive: i64,
    #[serde(rename = "poolCount")]
    pub pool_count: i64,
    #[serde(rename = "tcpMux")]
    pub tcp_mux: bool,
    #[serde(rename = "tcpMuxKeepaliveInterval")]
    pub tcp_mux_keepalive_interval: i64,
    pub protocol: String,
    #[serde(rename = "connectServerLocalIP")]
    pub connect_server_local_ip: String,
    #[serde(rename = "proxyURL")]
    pub proxy_url: String,
    pub tls: TransportTlsConfig,
    #[serde(rename = "heartbeatInterval")]
    pub heartbeat_interval: i64,
    #[serde(rename = "heartbeatTimeout")]
    pub heartbeat_timeout: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrpcSystemConfiguration {
    #[serde(rename = "launchAtStartup")]
    pub launch_at_startup: bool,
    #[serde(rename = "silentStartup")]
    pub silent_startup: bool,
    #[serde(rename = "autoConnectOnStartup")]
    pub auto_connect_on_startup: bool,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrpcProxyTransportConfig {
    #[serde(rename = "useEncryption")]
    pub use_encryption: bool,
    #[serde(rename = "useCompression")]
    pub use_compression: bool,
    #[serde(rename = "proxyProtocolVersion")]
    pub proxy_protocol_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcProxy {
    #[serde(rename = "_id")]
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    #[serde(rename = "localIP")]
    pub local_ip: String,
    #[serde(rename = "localPort")]
    pub local_port: String,
    #[serde(rename = "remotePort")]
    pub remote_port: String,
    #[serde(rename = "customDomains")]
    pub custom_domains: Vec<String>,
    pub locations: Vec<String>,
    #[serde(rename = "hostHeaderRewrite")]
    pub host_header_rewrite: String,
    #[serde(rename = "visitorsModel")]
    pub visitors_model: String,
    #[serde(rename = "serverUser")]
    pub server_user: String,
    #[serde(rename = "serverName")]
    pub server_name: String,
    #[serde(rename = "secretKey")]
    pub secret_key: String,
    #[serde(rename = "bindAddr")]
    pub bind_addr: String,
    #[serde(rename = "bindPort")]
    pub bind_port: Option<i64>,
    pub subdomain: String,
    #[serde(rename = "basicAuth")]
    pub basic_auth: bool,
    #[serde(rename = "httpUser")]
    pub http_user: String,
    #[serde(rename = "httpPassword")]
    pub http_password: String,
    #[serde(rename = "fallbackTo")]
    pub fallback_to: String,
    #[serde(rename = "fallbackTimeoutMs")]
    pub fallback_timeout_ms: i64,
    #[serde(rename = "https2http")]
    pub https2http: bool,
    #[serde(rename = "https2httpCaFile")]
    pub https2http_ca_file: String,
    #[serde(rename = "https2httpKeyFile")]
    pub https2http_key_file: String,
    #[serde(rename = "keepTunnelOpen")]
    pub keep_tunnel_open: bool,
    pub status: i64,
    pub transport: FrpcProxyTransportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcVersion {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "githubReleaseId")]
    pub github_release_id: i64,
    #[serde(rename = "githubAssetId")]
    pub github_asset_id: i64,
    #[serde(rename = "githubCreatedAt")]
    pub github_created_at: String,
    pub name: String,
    #[serde(rename = "assetName")]
    pub asset_name: String,
    #[serde(rename = "versionDownloadCount")]
    pub version_download_count: i64,
    #[serde(rename = "assetDownloadCount")]
    pub asset_download_count: i64,
    #[serde(rename = "browserDownloadUrl")]
    pub browser_download_url: String,
    pub downloaded: bool,
    #[serde(rename = "localPath")]
    pub local_path: Option<String>,
    pub size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSourceFrpcDesktopServer {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "frpcVersion")]
    pub frpc_version: Option<i64>,
    pub multiuser: bool,
    pub user: String,
    #[serde(rename = "serverAddr")]
    pub server_addr: String,
    #[serde(rename = "serverPort")]
    pub server_port: i64,
    #[serde(rename = "loginFailExit")]
    pub login_fail_exit: bool,
    #[serde(rename = "udpPacketSize")]
    pub udp_packet_size: i64,
    pub auth: AuthConfig,
    pub log: LogConfig,
    #[serde(rename = "webServer")]
    pub web_server: WebServerConfig,
    pub transport: TransportConfig,
    pub metadatas: serde_json::Value,
    pub system: FrpcSystemConfiguration,
}

impl OpenSourceFrpcDesktopServer {
    pub fn default_server() -> Self {
        Self {
            id: "".to_string(),
            frpc_version: None,
            multiuser: false,
            user: "".to_string(),
            server_addr: "".to_string(),
            server_port: 7000,
            login_fail_exit: false,
            udp_packet_size: 1500,
            auth: AuthConfig::default(),
            log: LogConfig {
                to: "".to_string(),
                level: "info".to_string(),
                max_days: 3,
                disable_print_color: false,
            },
            web_server: WebServerConfig {
                addr: "127.0.0.1".to_string(),
                port: 57400,
                user: "".to_string(),
                password: "".to_string(),
                pprof_enable: false,
            },
            transport: TransportConfig {
                dial_server_timeout: 10,
                dial_server_keepalive: 7200,
                pool_count: 0,
                tcp_mux: true,
                tcp_mux_keepalive_interval: 30,
                protocol: "tcp".to_string(),
                connect_server_local_ip: "".to_string(),
                proxy_url: "".to_string(),
                tls: TransportTlsConfig {
                    enable: true,
                    cert_file: "".to_string(),
                    key_file: "".to_string(),
                    trusted_ca_file: "".to_string(),
                    server_name: "".to_string(),
                    disable_custom_tls_first_byte: true,
                },
                heartbeat_interval: 30,
                heartbeat_timeout: 90,
            },
            metadatas: serde_json::json!({ "token": "" }),
            system: FrpcSystemConfiguration {
                launch_at_startup: false,
                silent_startup: false,
                auto_connect_on_startup: false,
                language: "en-US".to_string(),
            },
        }
    }
}
