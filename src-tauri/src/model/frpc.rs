//! FRPC desktop process status models.

use serde::{Deserialize, Serialize};

/// Payload pushed on the `frpcProcess:watchFrpcLog` event channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcProcessStatus {
    pub running: bool,
    pub last_start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_error: Option<String>,
}

/// Payload pushed on the `system:watchSystemUsage` event channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemUsage {
    pub cpu: f64,
    pub memory: SystemUsageMemory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemUsageMemory {
    pub used: i64,
    pub percentage: f64,
}

/// Download progress payload pushed on the `version:downloadProgress` channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub percent: f64,
    pub github_release_id: i64,
    pub completed: bool,
}
