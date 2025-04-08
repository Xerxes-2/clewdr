use tracing::info;
use colored::Colorize;
use std::sync::Arc;
use rquest::Client;
use serde::Deserialize;
use std::fs::File;
use std::io::{copy, BufReader};
use std::env;
use zip::ZipArchive;

use crate::config::Config;

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
    // #[serde(default)]
    // name: String,
    // #[serde(default)]
    // body: String,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub struct Updater {
    config: Arc<Config>,
    client: Client,
    user_agent: String,
    repo_owner: String,
    repo_name: String,
}

impl Updater {
    pub fn new(config: Arc<Config>) -> Self {
        let authors = option_env!("CARGO_PKG_AUTHORS").unwrap_or("");
        let repo_owner = authors.split(':').next().unwrap_or("Xerxes-2").to_string();
        let repo_name = env!("CARGO_PKG_NAME").to_string();

        let user_agent = format!(
            "clewdr/{} (+https://github.com/{}/{})",
            env!("CARGO_PKG_VERSION"),
            repo_owner,
            repo_name
        );

        Self {
            config,
            client: Client::new(),
            user_agent,
            repo_owner,
            repo_name,
        }
    }

    pub async fn check_for_updates(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.check_update {
            return Ok(false);
        }

        info!("Checking for updates...");
        info!("User-Agent: {}", self.user_agent);

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.repo_owner, self.repo_name
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", &self.user_agent)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to check updates: HTTP {}", response.status()).into());
        }

        let release: GitHubRelease = response.json().await?;
        let latest_version = release.tag_name.trim_start_matches('v');
        let current_version = env!("CARGO_PKG_VERSION");

        let update_available = self.compare_versions(current_version, latest_version)?;

        if update_available {
            info!("New version {} available (current: {})", latest_version, current_version);
            println!(
                "{}",
                format!("New version {} available! (current: {})", latest_version, current_version)
                    .yellow()
            );

            // Auto update if enabled
            if self.config.auto_update {
                self.perform_update(&release).await?;
            }

            Ok(true)
        } else {
            info!("Already at the latest version {}", current_version);
            Ok(false)
        }
    }

    async fn perform_update(&self, release: &GitHubRelease) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let latest_version = release.tag_name.trim_start_matches('v');
        
        // Find appropriate asset for this platform
        let asset = self.find_appropriate_asset(release)?;

        println!("Downloading update from {}", asset.browser_download_url);

        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let zip_path = temp_dir.path().join("update.zip");

        // Download the asset
        let response = self
            .client
            .get(&asset.browser_download_url)
            .header("User-Agent", &self.user_agent)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to download update: HTTP {}", response.status()).into());
        }

        // Save the downloaded file
        let content = response.bytes().await?;
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

        let binary_name = if cfg!(windows) { "clewdr.exe" } else { "clewdr" };
        let binary_path = extract_dir.join(binary_name);

        if !binary_path.exists() {
            return Err(format!("Binary not found in update package: {}", binary_name).into());
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
            let so_path = extract_dir.join("libc++_shared.so");
            if so_path.exists() {
                let current_dir = env::current_exe()?
                    .parent()
                    .ok_or("Failed to get parent directory")?
                    .to_path_buf();
                let target_so_path = current_dir.join("libc++_shared.so");
                std::fs::copy(&so_path, &target_so_path)?;
                info!("Copied libc++_shared.so to the application directory");
            } else {
                info!("Warning: libc++_shared.so not found in the update package");
            }
        }

        // Replace the current binary
        self_replace::self_replace(&binary_path)?;

        println!("{}", format!("Successfully updated to version {}", latest_version).green());
        println!("{}", "Update complete, closing...".green());
        std::process::exit(0);
    }

    fn find_appropriate_asset<'a>(&self, release: &'a GitHubRelease) -> Result<&'a GitHubAsset, Box<dyn std::error::Error + Send + Sync>> {
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
            },
            ("linux", "aarch64") => {
                if cfg!(target_env = "musl") {
                    "musllinux-aarch64"
                } else {
                    "linux-aarch64"
                }
            },
            ("macos", "x86_64") => "macos-x86_64",
            ("macos", "aarch64") => "macos-aarch64",
            ("android", "aarch64") => "android-aarch64",
            _ => return Err(format!("Unsupported platform: {}-{}", os, arch).into()),
        };

        for asset in &release.assets {
            if asset.name.contains(target) && asset.name.ends_with(".zip") {
                return Ok(asset);
            }
        }

        Err(format!("No suitable release asset found for {}-{}", os, arch).into())
    }

    fn compare_versions(&self, current: &str, latest: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let parse_version = |v: &str| -> Result<(u32, u32, u32), Box<dyn std::error::Error + Send + Sync>> {
            let parts: Vec<&str> = v.split('.').collect();
            if parts.len() < 3 {
                return Err(format!("Invalid version format: {}", v).into());
            }
            let major = parts[0].parse::<u32>()?;
            let minor = parts[1].parse::<u32>()?;
            let patch = parts[2].parse::<u32>()?;
            Ok((major, minor, patch))
        };

        let (current_major, current_minor, current_patch) = parse_version(current)?;
        let (latest_major, latest_minor, latest_patch) = parse_version(latest)?;

        if latest_major > current_major {
            return Ok(true);
        }
        if latest_major == current_major && latest_minor > current_minor {
            return Ok(true);
        }
        if latest_major == current_major && latest_minor == current_minor && latest_patch > current_patch {
            return Ok(true);
        }

        Ok(false)
    }
}