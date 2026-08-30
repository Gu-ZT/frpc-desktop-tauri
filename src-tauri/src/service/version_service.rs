//! Version service (ported from electron/service/VersionService.ts).
//!
//! Handles listing frp releases (GitHub + local JSON fallback), downloading
//! with progress, decompression, local import with checksum validation, and
//! deletion.

use std::fs;
use std::path::Path;

use crate::core::business_error::{BusinessError, ResponseCode};
use crate::core::constants::GlobalConstant;
use crate::core::logger::Logger;
use crate::core::paths::PathUtils;
use crate::db::version_repository::VersionRepository;
use crate::model::frp::FrpcVersion;
use crate::model::github::GithubRelease;
use crate::service::github_service::GitHubService;
use crate::service::system_service::SystemService;
use crate::util::file_utils::FileUtils;

/// Embedded local release JSON (mirrors `electron/json/frp-releases.json`).
const FRP_RELEASES_JSON: &str = include_str!("../json/frp-releases.json");
/// Embedded checksums JSON (mirrors `electron/json/frp_all_sha256_checksums.json`).
const FRP_CHECKSUMS_JSON: &str = include_str!("../json/frp_all_sha256_checksums.json");

const GITHUB_DOWNLOAD_MIRROR_PREFIX: &str = "https://gh.jwinks.com/file/";

#[derive(Clone)]
pub struct VersionService {
    version_repo: VersionRepository,
    system_service: SystemService,
    git_hub_service: GitHubService,
    versions: std::sync::Arc<std::sync::Mutex<Vec<FrpcVersion>>>,
}

impl VersionService {
    pub fn new(
        version_repo: VersionRepository,
        system_service: SystemService,
        git_hub_service: GitHubService,
    ) -> Self {
        Self {
            version_repo,
            system_service,
            git_hub_service,
            versions: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub async fn get_frp_versions_by_github(&self) -> Result<Vec<FrpcVersion>, BusinessError> {
        let releases = self
            .git_hub_service
            .get_github_repo_all_releases("fatedier/frp")
            .await
            .map_err(BusinessError::internal)?;
        let versions = self
            .github_release_to_frpc_version(releases)
            .await
            .map_err(BusinessError::internal)?;
        *self.versions.lock().unwrap() = versions.clone();
        Ok(versions)
    }

    pub async fn get_frp_version_by_local_json(&self) -> Result<Vec<FrpcVersion>, BusinessError> {
        let releases: Vec<GithubRelease> = serde_json::from_str(FRP_RELEASES_JSON)
            .map_err(|e| BusinessError::internal(format!("parse local json failed: {e}")))?;
        self.github_release_to_frpc_version(releases)
            .await
            .map_err(BusinessError::internal)
    }

    fn get_download_url(original_url: &str, mirror_id: Option<&str>) -> String {
        match mirror_id {
            None | Some("github") => original_url.to_string(),
            Some(_) => format!("{GITHUB_DOWNLOAD_MIRROR_PREFIX}{original_url}"),
        }
    }

    fn find_current_architecture_asset<'a>(
        &self,
        assets: &'a [crate::model::github::GithubAsset],
    ) -> Option<&'a crate::model::github::GithubAsset> {
        let fragments = GlobalConstant::current_arch_fragments();
        assets
            .iter()
            .find(|asset| fragments.iter().all(|f| asset.name.contains(f.as_str())))
    }

    async fn github_release_to_frpc_version(
        &self,
        releases: Vec<GithubRelease>,
    ) -> Result<Vec<FrpcVersion>, String> {
        let all_versions = self
            .version_repo
            .find_all()
            .map_err(|e| format!("load versions failed: {e}"))?;
        let mut out = Vec::new();
        for release in releases {
            // only support toml versions (release id > 124395282)
            if release.id <= 124395282 {
                continue;
            }
            let Some(asset) = self.find_current_architecture_asset(&release.assets) else {
                continue;
            };
            let download_count: i64 = release.assets.iter().map(|a| a.download_count).sum();

            let curr_version = all_versions
                .iter()
                .find(|v| v.github_release_id == release.id);
            let binary_exists = match curr_version {
                Some(v) => self.frpc_version_exists(v),
                None => false,
            };

            // clean up stale DB records
            if let Some(curr) = curr_version {
                if !binary_exists {
                    Logger::warn(
                        "VersionService.githubRelease2FrpcVersion",
                        &format!(
                            "Binary missing for version={}, removing stale DB record",
                            release.name
                        ),
                    );
                    let _ = self.version_repo.delete_by_id(&curr.id);
                }
            }

            out.push(FrpcVersion {
                id: String::new(),
                github_release_id: release.id,
                github_asset_id: asset.id,
                github_created_at: asset.created_at.clone(),
                name: release.name.clone(),
                asset_name: asset.name.clone(),
                version_download_count: download_count,
                asset_download_count: asset.download_count,
                browser_download_url: asset.browser_download_url.clone(),
                downloaded: binary_exists,
                local_path: if binary_exists {
                    curr_version.and_then(|v| v.local_path.clone())
                } else {
                    None
                },
                size: FileUtils::format_bytes(asset.size, 2),
            });
        }
        Ok(out)
    }

    fn frpc_version_exists(&self, version: &FrpcVersion) -> bool {
        if let Some(local_path) = &version.local_path {
            let filename = if cfg!(target_os = "windows") {
                PathUtils::get_win_frp_filename()
            } else {
                PathUtils::get_frpc_filename()
            };
            return Path::new(local_path).join(filename).exists();
        }
        false
    }

    pub async fn download_frp_version(
        &self,
        github_release_id: i64,
        mirror_id: Option<String>,
        on_progress: impl Fn(f64) + Send + 'static,
    ) -> Result<FrpcVersion, BusinessError> {
        let version = self
            .versions
            .lock()
            .unwrap()
            .iter()
            .find(|v| v.github_release_id == github_release_id)
            .cloned()
            .ok_or_else(|| BusinessError::internal("version not found".to_string()))?;

        let url = Self::get_download_url(&version.browser_download_url, mirror_id.as_deref());
        let downloaded_file_path = PathUtils::get_download_storage_path().join(&version.asset_name);
        let version_file_path = PathUtils::get_version_storage_path()
            .join(crate::core::paths::PathUtils::md5(&version.name));
        if version_file_path.exists() {
            fs::remove_dir_all(&version_file_path).ok();
        }
        Logger::info(
            "VersionService.downloadFrpVersion",
            &format!(
                "Downloading version={}, asset={}, url={}",
                version.name, version.asset_name, url
            ),
        );

        // stream download with progress
        let client = reqwest::Client::builder()
            .user_agent(format!(
                "frpc-desktop/{}",
                std::env::var("FRPC_DESKTOP_VERSION")
                    .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
            ))
            .build()
            .map_err(|e| BusinessError::internal(format!("client build failed: {e}")))?;
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| BusinessError::internal(format!("download failed: {e}")))?;
        let total = response.content_length().unwrap_or(0);
        let mut stream = response.bytes_stream();
        let mut file = fs::File::create(&downloaded_file_path)
            .map_err(|e| BusinessError::internal(format!("create file failed: {e}")))?;
        use futures_util::StreamExt;
        let mut received: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| BusinessError::internal(format!("download error: {e}")))?;
            use std::io::Write;
            file.write_all(&chunk)
                .map_err(|e| BusinessError::internal(format!("write file failed: {e}")))?;
            received += chunk.len() as u64;
            if total > 0 {
                on_progress(received as f64 / total as f64);
            }
        }
        drop(file);
        on_progress(1.0);
        Logger::info(
            "VersionService.downloadFrpVersion",
            &format!(
                "Download completed: {}, starting decompression",
                version.asset_name
            ),
        );

        let mut version = version;
        self.decompress_frp(&mut version, &downloaded_file_path.to_string_lossy())
            .await
    }

    pub async fn get_downloaded_versions(&self) -> Result<Vec<FrpcVersion>, BusinessError> {
        self.version_repo
            .find_all()
            .map_err(|e| BusinessError::internal(format!("load versions failed: {e}")))
    }

    pub async fn delete_frp_version(&self, github_release_id: i64) -> Result<(), BusinessError> {
        let Some(version) = self
            .version_repo
            .find_by_github_release_id(github_release_id)
            .map_err(|e| BusinessError::internal(format!("load version failed: {e}")))?
        else {
            return Ok(());
        };
        Logger::info(
            "VersionService.deleteFrpVersion",
            &format!(
                "Deleting version={}, path={:?}",
                version.name, version.local_path
            ),
        );
        if let Some(local_path) = &version.local_path {
            if Path::new(local_path).exists() {
                fs::remove_dir_all(local_path).ok();
            }
        }
        self.version_repo
            .delete_by_id(&version.id)
            .map_err(|e| BusinessError::internal(format!("delete version failed: {e}")))?;
        Logger::info(
            "VersionService.deleteFrpVersion",
            &format!("Version deleted: {}", version.name),
        );
        Ok(())
    }

    pub async fn import_local_frpc_version(
        &self,
        file_path: &str,
    ) -> Result<FrpcVersion, BusinessError> {
        Logger::info(
            "VersionService.importLocalFrpcVersion",
            &format!("Importing local file: {file_path}"),
        );
        let checksum = FileUtils::calculate_file_checksum(Path::new(file_path))
            .map_err(|e| BusinessError::internal(format!("checksum failed: {e}")))?;
        let checksums: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(FRP_CHECKSUMS_JSON)
                .map_err(|e| BusinessError::internal(format!("parse checksums failed: {e}")))?;
        let frp_name = checksums.get(&checksum).and_then(|v| v.as_str());
        let Some(frp_name) = frp_name else {
            Logger::warn(
                "VersionService.importLocalFrpcVersion",
                &format!("Unknown version, checksum not found: {checksum}"),
            );
            return Err(BusinessError::new(ResponseCode::UnknownVersion));
        };
        let fragments = GlobalConstant::current_arch_fragments();
        if !fragments.iter().all(|f| frp_name.contains(f.as_str())) {
            Logger::warn(
                "VersionService.importLocalFrpcVersion",
                &format!(
                    "Architecture mismatch: file={frp_name}, current={}",
                    fragments.join(",")
                ),
            );
            return Err(BusinessError::new(ResponseCode::VersionArgsError));
        }
        Logger::info(
            "VersionService.importLocalFrpcVersion",
            &format!("Checksum matched: {frp_name}"),
        );
        let version = self
            .get_frp_version_by_asset_name(frp_name)
            .ok_or_else(|| BusinessError::internal("asset not found in version list"))?;
        let exists = self
            .version_repo
            .exists(version.github_release_id)
            .map_err(|e| BusinessError::internal(format!("check version failed: {e}")))?;
        if exists {
            return Err(BusinessError::new(ResponseCode::VersionExists));
        }
        let mut version = version;
        self.decompress_frp(&mut version, file_path).await
    }

    fn get_frp_version_by_asset_name(&self, asset_name: &str) -> Option<FrpcVersion> {
        self.versions
            .lock()
            .unwrap()
            .iter()
            .find(|v| v.asset_name == asset_name)
            .cloned()
    }

    async fn decompress_frp(
        &self,
        version: &mut FrpcVersion,
        compressed_path: &str,
    ) -> Result<FrpcVersion, BusinessError> {
        let version_file_path = PathUtils::get_version_storage_path()
            .join(crate::core::paths::PathUtils::md5(&version.name));
        let ext = Path::new(&version.asset_name)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext_with_dot = format!(".{ext}");
        let file_name = Path::new(&version.asset_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Logger::info(
            "VersionService.decompressFrp",
            &format!(
                "Decompressing version={}, src={compressed_path}, dest={}",
                version.name,
                version_file_path.display()
            ),
        );

        if ext_with_dot == GlobalConstant::ZIP_EXT {
            self.system_service
                .decompress_zip_file(compressed_path, &version_file_path.to_string_lossy())
                .map_err(BusinessError::internal)?;
            let frp_temp_path = version_file_path.join(&file_name);
            let win_name = PathUtils::get_win_frp_filename();
            if frp_temp_path.join("frpc.exe").exists() {
                fs::rename(
                    frp_temp_path.join("frpc.exe"),
                    version_file_path.join(&win_name),
                )
                .map_err(|e| BusinessError::internal(format!("rename frpc failed: {e}")))?;
            }
            fs::remove_dir_all(&frp_temp_path).ok();
            Logger::info(
                "VersionService.decompressFrp",
                &format!("Decompression completed (zip): {}", version.name),
            );
        } else if ext_with_dot == GlobalConstant::GZ_EXT
            && version.asset_name.contains(GlobalConstant::TAR_GZ_EXT)
        {
            self.system_service
                .decompress_tar_gz_file(compressed_path, &version_file_path.to_string_lossy())
                .map_err(BusinessError::internal)?;
            let frpc_file_path = version_file_path.join("frpc");
            if frpc_file_path.exists() {
                let new_frpc_file_path = version_file_path.join(PathUtils::get_frpc_filename());
                fs::rename(&frpc_file_path, &new_frpc_file_path)
                    .map_err(|e| BusinessError::internal(format!("rename frpc failed: {e}")))?;
            }
            let downloaded_file = PathUtils::get_download_storage_path().join(&version.asset_name);
            if downloaded_file.exists() {
                fs::remove_file(&downloaded_file).ok();
            }
            Logger::info(
                "VersionService.decompressFrp",
                &format!("Decompression completed (tar.gz): {}", version.name),
            );
        }

        version.local_path = Some(version_file_path.to_string_lossy().to_string());
        version.downloaded = true;
        let mut version = version.clone();
        self.version_repo
            .insert(&mut version)
            .map_err(|e| BusinessError::internal(format!("save version failed: {e}")))
    }
}
