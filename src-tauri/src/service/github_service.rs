//! GitHub API client (ported from electron/service/GitHubService.ts).
//!
//! Uses the `gh.jwinks.com` API mirror with the same user-agent as the
//! Electron version (`frpc-desktop/<version>`).

use reqwest::Client;

use crate::model::github::GithubRelease;

const MIRROR_API_PREFIX: &str = "https://gh.jwinks.com/api/repos";

#[derive(Clone)]
pub struct GitHubService {
    client: Client,
    user_agent: String,
}

impl GitHubService {
    pub fn new(app_version: &str) -> Self {
        let ua = format!("frpc-desktop/{app_version}");
        Self {
            client: Client::builder()
                .user_agent(ua.clone())
                .build()
                .unwrap_or_default(),
            user_agent: ua,
        }
    }

    pub async fn get_github_repo_all_releases(
        &self,
        github_repo: &str,
    ) -> Result<Vec<GithubRelease>, String> {
        let url = format!("{MIRROR_API_PREFIX}/{github_repo}/releases?page=1&per_page=1000");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("GitHub request failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "GitHub request failed with status {}",
                response.status()
            ));
        }
        let releases: Vec<GithubRelease> = response
            .json()
            .await
            .map_err(|e| format!("GitHub response parse failed: {e}"))?;
        Ok(releases)
    }

    pub async fn get_github_last_release(
        &self,
        github_repo: &str,
    ) -> Result<GithubRelease, String> {
        let url = format!("{MIRROR_API_PREFIX}/{github_repo}/releases/latest");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("GitHub request failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "GitHub request failed with status {}",
                response.status()
            ));
        }
        response
            .json()
            .await
            .map_err(|e| format!("GitHub response parse failed: {e}"))
    }

    #[allow(dead_code)]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}
