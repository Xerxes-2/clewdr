use std::path::PathBuf;
use tracing::error;

use crate::{config::CONFIG_NAME, error::ClewdrError};

/// Get directory of the config file
pub fn config_dir() -> Result<PathBuf, ClewdrError> {
    let cwd = std::env::current_dir().map_err(|_| ClewdrError::PathNotFound("cwd".to_string()))?;
    let cwd_config = cwd.join(CONFIG_NAME);
    if cwd_config.exists() {
        return Ok(cwd);
    }
    let exec_path =
        std::env::current_exe().map_err(|_| ClewdrError::PathNotFound("exec".to_string()))?;
    let exec_dir = exec_path
        .parent()
        .ok_or_else(|| ClewdrError::PathNotFound("exec dir".to_string()))?
        .to_path_buf();
    Ok(exec_dir)
}

/// Helper function to print out json
pub fn print_out_json(json: &impl serde::ser::Serialize, file_name: &str) {
    let text = serde_json::to_string_pretty(json).unwrap_or_default();
    print_out_text(&text, file_name);
}

/// Helper function to print out text
pub fn print_out_text(text: &str, file_name: &str) {
    let Ok(dir) = config_dir() else {
        error!("No config found in cwd or exec dir");
        return;
    };
    let log_dir = dir.join("log");
    if !log_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            error!("Failed to create log dir: {}\n", e);
            return;
        }
    }
    let file_name = log_dir.join(file_name);
    let Ok(mut file) = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_name)
    else {
        error!("Failed to open file: {}", file_name.display());
        return;
    };
    if let Err(e) = std::io::Write::write_all(&mut file, text.as_bytes()) {
        error!("Failed to write to file: {}\n", e);
    }
}

/// Timezone for the API
pub const TIME_ZONE: &str = "America/New_York";
