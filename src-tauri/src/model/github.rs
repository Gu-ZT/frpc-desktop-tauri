//! GitHub release models (ported from types/github.d.ts).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubAsset {
    pub id: i64,
    pub name: String,
    pub size: i64,
    pub download_count: i64,
    pub created_at: String,
    #[serde(rename = "browser_download_url")]
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRelease {
    pub id: i64,
    pub name: String,
    #[serde(rename = "tag_name")]
    pub tag_name: String,
    pub body: String,
    #[serde(rename = "html_url")]
    pub html_url: String,
    pub assets: Vec<GithubAsset>,
}

/// Local port listening record (used by the proxy page port picker).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPort {
    pub protocol: String,
    pub ip: String,
    pub port: i64,
}

/// Mirror option descriptor (renderer-only, kept for parity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubMirror {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}
