use self_update::cargo_crate_version;
use tracing::info;
use colored::Colorize;

use crate::config::Config;

pub async fn check_for_updates(config: &Config) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if !config.check_update {
        return Ok(false);
    }
    
    info!("Checking for updates...");
    let authors = option_env!("CARGO_PKG_AUTHORS").unwrap_or("");
    let repo_owner: &str = authors.split(':').next().unwrap_or("Xerxes-2");
    const REPO_NAME: &str = env!("CARGO_PKG_NAME");
    
    let update_available = tokio::task::spawn_blocking(|| -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let releases = self_update::backends::github::ReleaseList::configure()
            .repo_owner(repo_owner)
            .repo_name(REPO_NAME)
            .build()?
            .fetch()?;
        
        if releases.is_empty() {
            return Ok(false);
        }
        
        // Check if the current version is less than the latest release
        let current_version = cargo_crate_version!();
        
        for release in releases {
            if self_update::version::bump_is_greater(current_version, &release.version)
                .unwrap_or(false)
            {
                info!("New version {} available (current: {})", release.version, current_version);
                println!("{}", format!("New version {} available! (current: {})", 
                         release.version, current_version).yellow());
                return Ok(true);
            }
        }
        
        info!("Already at the latest version {}", current_version);
        Ok(false)
    }).await??;
    
    // update if available and auto_update is enabled
    if update_available && config.auto_update {
        perform_update().await?;
    }
    
    Ok(update_available)
}

pub async fn perform_update() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("{}", "Performing update...".cyan());
    let authors = option_env!("CARGO_PKG_AUTHORS").unwrap_or("");
    let repo_owner: &str = authors.split(':').next().unwrap_or("Xerxes-2");
    const REPO_NAME: &str = env!("CARGO_PKG_NAME");

    tokio::task::spawn_blocking(|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status = self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(REPO_NAME)
            .bin_name(REPO_NAME)
            .show_download_progress(true)
            .current_version(cargo_crate_version!())
            .build()?
            .update()?;
        
        if status.updated() {
            info!("Updated to version {}", status.version());
            println!("{}", format!("Successfully updated to version {}", status.version()).green());
        } else {
            info!("Update not needed, already at version {}", status.version());
            println!("{}", format!("Already at the latest version {}", status.version()).green());
        }
        
        Ok(())
    }).await??;
    
    println!("{}", "Update complete, closeing...".green());
    std::process::exit(0);
}