use anyhow::{Context, Result};
use dirs;
use serde::Deserialize;
use std::{fs, path::PathBuf};
use toml;

/// Represents global application settings loaded from cmdy.toml.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    /// Command used for interactive filtering (e.g., fzf, gum choose, etc.).
    pub filter_command: String,
    /// Additional directories to scan (non-recursively) for TOML snippet files.
    pub directories: Vec<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            // Default fzf options: ANSI support, reverse layout, rounded border, 50% height
            filter_command: "fzf --ansi --layout=reverse --border=rounded --height=50%".to_string(),
            directories: Vec::new(),
        }
    }
}

/// Loads the application configuration from a TOML file.
/// Checks ~/.config/cmdy/cmdy.toml (macOS) or $XDG_CONFIG_HOME/cmdy/cmdy.toml, falling back to defaults.
pub fn load_app_config() -> Result<AppConfig> {
    let config_path = {
        #[cfg(target_os = "macos")]
        let base = dirs::home_dir()
            .map(|p| p.join(".config"))
            .unwrap_or_else(|| PathBuf::from("."));
        #[cfg(not(target_os = "macos"))]
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("cmdy").join("cmdy.toml")
    };
    if config_path.is_file() {
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
        match toml::from_str::<AppConfig>(&content) {
            Ok(cfg) => return Ok(cfg),
            Err(e) => eprintln!(
                "Warning: Failed to parse config file {}: {}. Using defaults.",
                config_path.display(),
                e
            ),
        }
    }
    Ok(AppConfig::default())
}

/// Determines the directory to load command definitions from.
/// Uses the `--dir` flag if provided, otherwise defaults to ~/.config/cmdy/commands or XDG config.
pub fn determine_config_directory(cli_dir_flag: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = cli_dir_flag {
        Ok(dir.clone())
    } else {
        let default_path = if cfg!(target_os = "macos") {
            dirs::home_dir().map(|mut path| {
                path.push(".config");
                path.push("cmdy");
                path.push("commands");
                path
            })
        } else {
            dirs::config_dir().map(|mut path| {
                path.push("cmdy");
                path.push("commands");
                path
            })
        };
        match default_path {
            Some(path) => Ok(path),
            None => Ok(PathBuf::from("./commands")),
        }
    }
}

// --- Tests for config ---
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    /// Tests that the `--dir` flag correctly overrides the default config directory.
    fn test_determine_config_directory_flag_override() -> Result<()> {
        let temp_dir = tempdir()?;
        let flag_path = temp_dir.path().join("custom_cmdy_dir_test");
        let cli_dir = Some(flag_path.clone());
        let result = determine_config_directory(&cli_dir)?;
        assert_eq!(result, flag_path);
        Ok(())
    }

    #[test]
    /// Tests that the default configuration directory logic works correctly.
    fn test_determine_config_directory_default() -> Result<()> {
        let cli_dir = None;
        let result = determine_config_directory(&cli_dir)?;
        let expected = if cfg!(target_os = "macos") {
            dirs::home_dir()
                .map(|mut path| {
                    path.push(".config");
                    path.push("cmdy");
                    path.push("commands");
                    path
                })
                .unwrap()
        } else {
            dirs::config_dir()
                .map(|mut path| {
                    path.push("cmdy");
                    path.push("commands");
                    path
                })
                .unwrap()
        };
        assert_eq!(result, expected);
        Ok(())
    }
}
