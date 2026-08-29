//! Global constants (ported from electron/core/GlobalConstant.ts).

pub struct GlobalConstant;

impl GlobalConstant {
    pub const ZIP_EXT: &'static str = ".zip";
    pub const TOML_EXT: &'static str = ".toml";
    pub const GZ_EXT: &'static str = ".gz";
    pub const TAR_GZ_EXT: &'static str = ".tar.gz";
    pub const LOCAL_IP: &'static str = "127.0.0.1";
    pub const FRPC_LOGIN_FAIL_EXIT: bool = false;
    pub const INTERNET_CHECK_URL: &'static str = "http://www.msftconnecttest.com/connecttest.txt";
    pub const INTERNET_CHECK_TIMEOUT_SECS: u64 = 10;
    pub const DEFAULT_LANGUAGE: &'static str = "en-US";
    pub const FRPC_PROCESS_STATUS_CHECK_INTERVAL: u64 = 1;

    /// Mapping from (platform, arch) to frp release asset name fragments.
    pub fn frp_arch_version_mapping() -> &'static [(&'static str, &'static [&'static str])] {
        &[
            ("win32_x64", &["window", "amd64"]),
            ("win32_arm64", &["window", "arm64"]),
            ("win32_ia32", &["window", "386"]),
            ("darwin_arm64", &["darwin", "arm64"]),
            ("darwin_x64", &["darwin", "amd64"]),
            ("darwin_amd64", &["darwin", "amd64"]),
            ("linux_x64", &["linux", "amd64"]),
            ("linux_arm64", &["linux", "arm64"]),
        ]
    }

    /// Return the current platform key ("win32_x64", "darwin_arm64", ...).
    pub fn current_platform_key() -> String {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let os_key = match os {
            "windows" => "win32",
            "macos" => "darwin",
            _ => "linux",
        };
        let arch_key = match arch {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            "x86" => "ia32",
            "arm" => "arm64",
            _ => arch,
        };
        format!("{os_key}_{arch_key}")
    }

    /// Return the current platform arch fragments used to match frp assets.
    pub fn current_arch_fragments() -> Vec<String> {
        let key = Self::current_platform_key();
        Self::frp_arch_version_mapping()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }
}
