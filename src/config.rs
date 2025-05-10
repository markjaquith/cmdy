use anyhow::{Context, Result};
use std::{fs, path::PathBuf};
use serde::Deserialize;

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
    // Determine where to look for cmdy.toml
    let config_path = {
        #[cfg(target_os = "macos")]
        let base = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".config");
        #[cfg(not(target_os = "macos"))]
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
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
        return Ok(dir.clone());
    }
    // No CLI override: use XDG or HOME
    #[cfg(target_os = "macos")]
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".config");
    #[cfg(not(target_os = "macos"))]
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let path = base.join("cmdy").join("commands");
    Ok(path)
}

// --- Tests for config ---
#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::PathBuf, sync::Mutex};
    use tempfile::tempdir;

    // Serialize tests that modify the environment
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    /// Tests that the `--dir` flag correctly overrides the default config directory.
    fn test_determine_config_directory_flag_override() -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
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
        let _guard = ENV_LOCK.lock().unwrap();
        let cli_dir = None;
        let result = determine_config_directory(&cli_dir)?;
        let expected = if cfg!(target_os = "macos") {
            env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".config")
                .join("cmdy")
                .join("commands")
        } else {
            env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("cmdy")
                .join("commands")
        };
        assert_eq!(result, expected);
        Ok(())
    }

    #[test]
    /// load_app_config returns defaults when no config file is present
    fn test_load_app_config_default() -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        // Ensure no config environment variables
        unsafe {
            env::remove_var("XDG_CONFIG_HOME");
        }
        unsafe {
            env::remove_var("HOME");
        }
        let cfg = load_app_config()?;
        let default = AppConfig::default();
        assert_eq!(cfg.filter_command, default.filter_command);
        assert!(cfg.directories.is_empty());
        Ok(())
    }

    #[test]
    /// load_app_config loads valid TOML and parses fields
    fn test_load_app_config_file_parsed() -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempdir()?;
        // Determine config path based on OS
        let (_config_dir, config_file) = if cfg!(target_os = "macos") {
            unsafe {
                env::set_var("HOME", tmp.path());
            }
            let base = tmp.path().join(".config").join("cmdy");
            fs::create_dir_all(&base)?;
            let file = base.join("cmdy.toml");
            (base, file)
        } else {
            unsafe {
                env::set_var("XDG_CONFIG_HOME", tmp.path());
            }
            let base = tmp.path().join("cmdy");
            fs::create_dir_all(&base)?;
            let file = base.join("cmdy.toml");
            (base, file)
        };
        // Write a valid config
        let content = r#"
filter_command = "TESTCMD"
directories = ["one", "two"]
"#;
        fs::write(&config_file, content)?;
        let cfg = load_app_config()?;
        assert_eq!(cfg.filter_command, "TESTCMD");
        assert_eq!(
            cfg.directories,
            vec![PathBuf::from("one"), PathBuf::from("two")]
        );
        Ok(())
    }

    #[test]
    /// load_app_config falls back to defaults on parse error
    fn test_load_app_config_invalid_toml() -> Result<()> {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempdir()?;
        if cfg!(target_os = "macos") {
            unsafe {
                env::set_var("HOME", tmp.path());
            }
            let base = tmp.path().join(".config").join("cmdy");
            fs::create_dir_all(&base)?;
            fs::write(base.join("cmdy.toml"), "not toml")?;
        } else {
            unsafe {
                env::set_var("XDG_CONFIG_HOME", tmp.path());
            }
            let base = tmp.path().join("cmdy");
            fs::create_dir_all(&base)?;
            fs::write(base.join("cmdy.toml"), "not toml")?;
        }
        let cfg = load_app_config()?;
        let default = AppConfig::default();
        assert_eq!(cfg.filter_command, default.filter_command);
        assert!(cfg.directories.is_empty());
        Ok(())
    }
}
