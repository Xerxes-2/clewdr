use std::{
    env,
    fs::File,
    io::{BufReader, copy},
};

use colored::Colorize;
use http::header::USER_AGENT;
use serde::Deserialize;
use snafu::ResultExt;
use tracing::info;
use wreq::Client;
use zip::ZipArchive;

use crate::{
    Args,
    config::CLEWDR_CONFIG,
    error::{ClewdrError, WreqSnafu},
};

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Updater for the ClewdR application
/// Handles checking for updates and updating the application
pub struct ClewdrUpdater {
    client: Client,
    user_agent: String,
    repo_owner: &'static str,
    repo_name: &'static str,
}

impl ClewdrUpdater {
    /// Creates a new `ClewdrUpdater` instance
    ///
    /// # Returns
    /// * `Result<Self, ClewdrError>` - A new updater instance or an error
    ///
    /// # Errors
    /// If the HTTP client cannot be built for the configured proxy.
    pub fn new() -> Result<Self, ClewdrError> {
        let authors = option_env!("CARGO_PKG_AUTHORS").unwrap_or_default();
        let repo_owner = authors
            .split(':')
            .next()
            .and_then(|author| author.split('<').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Xerxes-2");
        let repo_name = env!("CARGO_PKG_NAME");
        let policy = wreq::redirect::Policy::default();
        let client = wreq::Client::builder()
            .redirect(policy)
            .build()
            .context(WreqSnafu {
                msg: "Failed to create HTTP client",
            })?;

        let user_agent = format!(
            "clewdr/{} (+https://github.com/{}/{})",
            env!("CARGO_PKG_VERSION"),
            repo_owner,
            repo_name
        );

        Ok(Self {
            client,
            user_agent,
            repo_owner,
            repo_name,
        })
    }

    /// Checks for updates by comparing the current version to the latest release on GitHub
    /// Performs automatic update if enabled in config or explicitly requested
    ///
    /// # Returns
    /// * `Result<bool, ClewdrError>` - True if update available, false otherwise
    ///
    /// # Errors
    /// If the release feed cannot be fetched or parsed, or the version string
    /// it reports is not valid semver.
    pub async fn check_for_updates(&self) -> Result<bool, ClewdrError> {
        if CLEWDR_CONFIG.load().no_fs {
            // If no_fs feature is enabled, skip update check
            info!("Update check skipped due to no_fs feature");
            return Ok(false);
        }

        let args: Args = clap::Parser::parse();
        if !args.update && !CLEWDR_CONFIG.load().check_update {
            return Ok(false);
        }

        info!("Checking for updates...");
        // info!("User-Agent: {}", self.user_agent);

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.repo_owner, self.repo_name
        );

        let response = self
            .client
            .get(&url)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to fetch latest release from GitHub",
            })?
            .error_for_status()
            .context(WreqSnafu {
                msg: "Fetch latest release from GitHub returned an error",
            })?;

        let release: GitHubRelease = response.json().await.context(WreqSnafu {
            msg: "Failed to parse GitHub release response",
        })?;
        let latest_version = release.tag_name.trim_start_matches('v');
        let current_version = env!("CARGO_PKG_VERSION");

        let update_available = Self::compare_versions(current_version, latest_version)?;

        if !update_available {
            info!("Already at the latest version {}", current_version.green());
            return Ok(false);
        }
        info!(
            "New version {} available (current: {})",
            latest_version.green().italic(),
            current_version.yellow()
        );
        // Auto update if enabled
        if args.update || CLEWDR_CONFIG.load().auto_update {
            self.perform_update(&release).await?;
        }

        Ok(true)
    }

    /// Performs the update process
    /// Downloads the appropriate release asset, extracts it, and replaces the current binary
    ///
    /// # Arguments
    /// * `release` - GitHub release information containing assets to download
    ///
    /// # Returns
    /// * `Result<(), ClewdrError>` - Success or error during update process
    async fn perform_update(&self, release: &GitHubRelease) -> Result<(), ClewdrError> {
        let latest_version = release.tag_name.trim_start_matches('v');

        // Find appropriate asset for this platform
        let asset = Self::find_appropriate_asset(release)?;

        info!("Downloading update from {}", asset.browser_download_url);

        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let zip_path = temp_dir.path().join("update.zip");

        // Download the asset
        let response = self
            .client
            .get(&asset.browser_download_url)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to download update asset",
            })?
            .error_for_status()
            .context(WreqSnafu {
                msg: "Download update asset returned an error",
            })?;

        // Save the downloaded file
        let content = response.bytes().await.context(WreqSnafu {
            msg: "Failed to read response bytes from update asset",
        })?;
        let mut file = File::create(&zip_path)?;
        copy(&mut content.as_ref(), &mut file)?;

        // Extract the zip
        let extract_dir = temp_dir.path().join("extracted");
        std::fs::create_dir_all(&extract_dir)?;

        let file = File::open(&zip_path)?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader)?;

        // Extract all files
        archive.extract(&extract_dir)?;

        let binary_name = if cfg!(windows) {
            "clewdr.exe"
        } else {
            "clewdr"
        };
        let binary_path = extract_dir.join(binary_name);

        if !binary_path.exists() {
            return Err(ClewdrError::AssetError {
                msg: format!("Binary not found in the update package: {binary_name}"),
            });
        }

        // Make the binary executable on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms)?;
        }

        #[cfg(target_os = "android")]
        {
            use tracing::warn;
            let so_path = extract_dir.join("libc++_shared.so");
            if so_path.exists() {
                let current_dir = env::current_exe()?
                    .parent()
                    .ok_or(ClewdrError::AssetError {
                        msg: "Failed to get current directory".to_string(),
                    })?
                    .to_path_buf();
                let target_so_path = current_dir.join("libc++_shared.so");
                std::fs::copy(&so_path, &target_so_path)?;
                info!("Copied libc++_shared.so to the application directory");
            } else {
                warn!("libc++_shared.so not found in the update package");
            }
        }

        // Replace the current binary
        self_replace::self_replace(&binary_path)?;

        println!("Successfully updated to version {}", latest_version.green());
        println!("{}", "Update complete, closing...".green());
        std::process::exit(0);
    }

    /// Finds the appropriate asset for the current platform and architecture
    ///
    /// # Arguments
    /// * `release` - GitHub release information containing available assets
    ///
    /// # Returns
    /// * `Result<&'a GitHubAsset, ClewdrError>` - Appropriate asset or error if none found
    fn find_appropriate_asset(release: &GitHubRelease) -> Result<&GitHubAsset, ClewdrError> {
        // Determine platform and architecture
        let os = env::consts::OS;
        let arch = env::consts::ARCH;

        let target = match (os, arch) {
            ("windows", "x86_64") => "windows-x86_64",
            ("linux", "x86_64") => {
                if cfg!(target_env = "musl") {
                    "musllinux-x86_64"
                } else {
                    "linux-x86_64"
                }
            }
            ("linux", "aarch64") => {
                if cfg!(target_env = "musl") {
                    "musllinux-aarch64"
                } else {
                    "linux-aarch64"
                }
            }
            ("macos", "x86_64") => "macos-x86_64",
            ("macos", "aarch64") => "macos-aarch64",
            ("android", "aarch64") => "android-aarch64",
            _ => {
                return Err(ClewdrError::AssetError {
                    msg: format!("Unsupported platform: {os}-{arch}"),
                });
            }
        };
        info!("Detected platform: {}", target);
        release
            .assets
            .iter()
            .find(|asset| {
                asset.name.contains(target)
                    && std::path::Path::new(&asset.name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
            })
            .ok_or(ClewdrError::AssetError {
                msg: format!("No suitable asset found for platform: {target}"),
            })
    }

    /// Compares two version strings to determine if an update is needed
    /// Parses versions in the format major.minor.patch
    ///
    /// # Arguments
    /// * `current` - Current version string
    /// * `latest` - Latest version string from GitHub
    ///
    /// # Returns
    /// * `Result<bool, ClewdrError>` - True if latest is newer than current, false otherwise
    fn compare_versions(current: &str, latest: &str) -> Result<bool, ClewdrError> {
        let parse_version = |v: &str| -> Result<(u32, u32, u32), ClewdrError> {
            let vec = v.split('.').collect::<Vec<_>>();
            let [major, minor, patch, ..] = vec.as_slice() else {
                return Err(ClewdrError::InvalidVersion {
                    version: v.to_string(),
                });
            };
            Ok((major.parse()?, minor.parse()?, patch.parse()?))
        };
        let current = parse_version(current)?;
        let latest = parse_version(latest)?;
        Ok(current < latest)
    }
}
