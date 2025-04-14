use anyhow::{anyhow, Context, Result};
use clap::Parser;
use dirs; // Import the dirs crate
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand, // Alias standard Command
};

// --- Structs ---

// Represents the data loaded from a command's TOML file
#[derive(Deserialize, Debug, Clone)]
pub struct CommandDef {
    pub description: String,
    pub command: String,
    // We'll add argument definitions here later
}

// Defines the command-line arguments your tool accepts
#[derive(Parser, Debug)]
#[command(name = "cmdy", author, version, about = "Runs predefined commands.", long_about = None)]
struct CliArgs {
    /// The name of the command command to execute
    command_name: Option<String>, // Make optional to allow listing commands

    /// Optional directory to load command definitions from.
    /// Defaults to $HOME/.config/cmdy/commands on Unix/macOS,
    /// standard config dir on Windows, or platform equivalent.
    #[arg(long, value_name = "DIRECTORY")] // Add the --dir flag
    dir: Option<PathBuf>,

    /// Arguments to pass to the command (captured but not used yet)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_args: Vec<String>,
}

// --- Main Logic ---

fn main() -> Result<()> {
    // Parse the command-line arguments provided by the user
    let cli_args = CliArgs::parse();

    // Determine the configuration directory to use
    let config_dir = determine_config_directory(&cli_args.dir)?;

    #[cfg(debug_assertions)]
    println!("Using configuration directory: {:?}", config_dir);

    // Load all command definitions from the determined directory
    let commands = load_commands(&config_dir)
        .with_context(|| format!("Failed to load command definitions from {:?}", config_dir))?;

    match cli_args.command_name {
        Some(name) => {
            // User provided a command name, find the definition
            let cmd_def = find_command_definition(&name, &commands)?;

            // Try to execute it (passing definition and user args)
            execute_command(&name, cmd_def, &cli_args.command_args)
                .with_context(|| format!("Failed to execute command '{}'", name))?;
        }
        None => {
            print_banner();
            list_available_commands(&commands, &config_dir);
        }
    }

    Ok(())
}

// --- Helper Functions ---

/// Prints the application banner and information.
fn print_banner() {
    // Use raw string literal for easier ASCII art handling
    println!(r#" ██████ ███    ███ ██████  ██    ██ "#);
    println!(r#"██      ████  ████ ██   ██  ██  ██  "#);
    println!(r#"██      ██ ████ ██ ██   ██   ████   "#);
    println!(r#"██      ██  ██  ██ ██   ██    ██    "#);
    println!(r#" ██████ ██      ██ ██████     ██    "#);
    println!();
    println!("Your friendly command manager");
    println!();
    println!("(it’s pronounced “commandy”)");
    println!();
    println!("(C) 2022-2025 Mark Jaquith"); // Using (C) for compatibility
}

/// Determines the directory to load command definitions from.
/// Priority:
/// 1. --dir flag
/// 2. macOS: $HOME/.config/cmdy/commands (forced)
/// 3. Other Unix: $XDG_CONFIG_HOME/cmdy/commands (or ~/.config/cmdy/commands)
/// 4. Windows: Standard config dir (%APPDATA%/cmdy/commands)
/// 5. Fallback: ./commands (if standard path cannot be determined)
fn determine_config_directory(cli_dir_flag: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = cli_dir_flag {
        // 1. Use the directory provided by the --dir flag
        Ok(dir.clone())
    } else {
        // Determine default based on OS
        let default_path = if cfg!(target_os = "macos") {
            // 2. Force $HOME/.config on macOS
            dirs::home_dir().map(|mut path| {
                path.push(".config"); // Use .config
                path.push("cmdy");
                path.push("commands");
                path
            })
        } else {
            // 3. & 4. Use standard config dir for other OS (Linux, Windows, etc.)
            dirs::config_dir().map(|mut path| {
                path.push("cmdy"); // Append our application's folder
                path.push("commands"); // Append the commands subfolder
                path
            })
        };

        match default_path {
            Some(path) => Ok(path),
            None => {
                // 5. Fallback if home or config dir is not available
                // Only print warning in debug builds
                #[cfg(debug_assertions)]
                eprintln!("Warning: Could not determine standard home or config directory. Falling back to './commands'.");
                Ok(PathBuf::from("./commands"))
            }
        }
    }
}

// --- Core Functions ---

/// Loads all `.toml` files from the specified directory into a HashMap.
pub fn load_commands(dir: &Path) -> Result<HashMap<String, CommandDef>> {
    let mut commands = HashMap::new();

    #[cfg(debug_assertions)]
    {
        let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        println!(
            "Attempting to load commands from: {}",
            canonical_dir.display()
        );
    }

    // Check if directory exists before trying to read
    if !dir.is_dir() {
        #[cfg(debug_assertions)]
        {
            let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            eprintln!(
                "Info: Configuration directory not found at {}. No commands loaded from this location.",
                canonical_dir.display()
            );
        }
        return Ok(commands);
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Process only if it's a file with a .toml extension
        if path.is_file() && path.extension().map_or(false, |ext| ext == "toml") {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|s| s.to_string())
                .context(format!(
                    "Could not get file stem or invalid UTF-8 for path: {}",
                    path.display()
                ))?;

            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read command file: {}", path.display()))?;

            match toml::from_str::<CommandDef>(&content) {
                Ok(cmd_def) => {
                    #[cfg(debug_assertions)]
                    println!(
                        "  Loaded definition for '{}' from {:?}",
                        name,
                        path.file_name().unwrap_or_default()
                    );
                    commands.insert(name, cmd_def);
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Warning: Failed to parse TOML from file: {}. Error: {}",
                        path.display(),
                        _e
                    );
                    // Continue loading other files even if one fails to parse
                }
            }
        }
    }
    Ok(commands)
}

/// Finds the command definition struct for a given command name.
pub fn find_command_definition<'a>(
    name: &str,
    commands: &'a HashMap<String, CommandDef>,
) -> Result<&'a CommandDef> {
    commands
        .get(name)
        .ok_or_else(|| anyhow!("Command '{}' not found.", name))
}

/// Executes the specified command using its definition.
pub fn execute_command(
    name: &str,
    cmd_def: &CommandDef,
    _cmd_args: &[String], // We receive user args but don't use them yet
) -> Result<()> {
    #[cfg(debug_assertions)]
    {
        println!("Executing '{}': {}", name, cmd_def.description);
        println!("  Running: {}", cmd_def.command);
    }

    // TODO: Implement argument parsing & substitution into cmd_def.command
    // TODO: Pass _cmd_args appropriately instead of just the raw template

    let mut cmd_process = if cfg!(target_os = "windows") {
        let mut cmd = ProcessCommand::new("cmd");
        cmd.args(["/C", &cmd_def.command]);
        cmd
    } else {
        let mut cmd = ProcessCommand::new("sh");
        cmd.arg("-c");
        cmd.arg(&cmd_def.command);
        cmd
    };

    let status = cmd_process
        .status()
        .with_context(|| format!("Failed to execute command '{}'", name))?;

    if !status.success() {
        anyhow::bail!("Command '{}' failed with status: {}", name, status);
    }

    #[cfg(debug_assertions)]
    println!("Command '{}' executed successfully.", name);
    Ok(())
}

/// Prints a list of available commands found in the configuration.
/// This output is essential and should appear in all builds.
pub fn list_available_commands(commands: &HashMap<String, CommandDef>, config_dir: &Path) {
    if commands.is_empty() {
        println!("No commands defined.");
        println!("Looked for *.toml files in: {}", config_dir.display());
        println!("Create .toml files in this directory to define commands.");
        return;
    }

    println!("Available commands (from {}):", config_dir.display());
    let mut names: Vec<_> = commands.keys().collect();
    names.sort();
    for name in names {
        if let Some(cmd_def) = commands.get(name) {
            println!("  {: <15} - {}", name, cmd_def.description);
        }
    }
    println!("\nRun 'cmdy <command_name> [args...]' to execute.");
    println!("Use 'cmdy --dir <directory>' to load commands from a different location.");
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    use super::*; // Import things from outer module
    use std::fs;
    use std::io::Write; // Import Write trait
    use tempfile::tempdir; // Use tempfile crate for easier test setup/cleanup

    // Helper function to create a temporary directory and files for testing
    fn setup_test_config(dir_path: &Path, files: &[(&str, &str)]) -> Result<()> {
        fs::create_dir_all(dir_path)?; // Ensure parent exists if needed
        for (name, content) in files {
            let file_path = dir_path.join(name);
            let mut file = fs::File::create(&file_path)
                .with_context(|| format!("Failed to create test file: {}", file_path.display()))?;
            writeln!(file, "{}", content)?;
        }
        Ok(())
    }

    #[test]
    fn test_determine_config_directory_flag_override() -> Result<()> {
        let flag_path = PathBuf::from("/tmp/custom_cmdy_dir");
        let cli_dir = Some(flag_path.clone());
        let result = determine_config_directory(&cli_dir)?;
        assert_eq!(result, flag_path);
        Ok(())
    }

    // Test the default path determination logic
    // This test's behavior depends on the OS it's run on
    #[test]
    fn test_determine_config_directory_default() -> Result<()> {
        let cli_dir = None;
        let result = determine_config_directory(&cli_dir)?;

        if cfg!(target_os = "macos") {
            // Expect $HOME/.config/cmdy/commands on macOS
            if let Some(mut expected_base) = dirs::home_dir() {
                expected_base.push(".config");
                expected_base.push("cmdy");
                expected_base.push("commands");
                assert_eq!(result, expected_base, "Test failed on macOS");
            } else {
                // If home_dir fails, expect fallback
                assert_eq!(
                    result,
                    PathBuf::from("./commands"),
                    "Test fallback failed on macOS"
                );
            }
        } else {
            // Expect standard config dir on other OS (Linux, Windows)
            if let Some(mut expected_base) = dirs::config_dir() {
                expected_base.push("cmdy");
                expected_base.push("commands");
                assert_eq!(result, expected_base, "Test failed on non-macOS");
            } else {
                // If config_dir fails, expect fallback
                assert_eq!(
                    result,
                    PathBuf::from("./commands"),
                    "Test fallback failed on non-macOS"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_load_commands_success() -> Result<()> {
        let temp_dir = tempdir()?; // Create a temp directory
        let dir_path = temp_dir.path();

        let files = [
            (
                "hello.toml",
                r#"
                description = "Says hello"
                command = "echo Hello World"
            "#,
            ),
            (
                "ls.toml",
                r#"
                description = "Lists directory contents"
                command = "ls -la"
            "#,
            ),
            ("extra.txt", "This should be ignored"),
        ];
        setup_test_config(dir_path, &files)?;

        let commands = load_commands(dir_path)?;

        assert_eq!(commands.len(), 2);
        assert!(commands.contains_key("hello"));
        assert!(commands.contains_key("ls"));
        assert_eq!(commands.get("hello").unwrap().description, "Says hello");
        assert_eq!(commands.get("ls").unwrap().command, "ls -la");

        // temp_dir is automatically cleaned up when it goes out of scope
        Ok(())
    }

    #[test]
    fn test_load_commands_empty_dir() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();
        // No need to create files, just the directory exists

        let commands = load_commands(dir_path)?;
        assert!(commands.is_empty());

        Ok(())
    }

    #[test]
    fn test_load_commands_nonexistent_dir() -> Result<()> {
        let dir_path = PathBuf::from("./target/test_data/load_nonexistent_unique_v6"); // Ensure unique path
        _ = fs::remove_dir_all(&dir_path); // Ensure it doesn't exist

        // Expect load_commands to print info (in debug) and return Ok(empty_map)
        let commands = load_commands(&dir_path)?;
        assert!(
            commands.is_empty(),
            "Expected empty map for non-existent dir, got {:?}",
            commands
        );

        Ok(())
    }

    #[test]
    fn test_load_commands_invalid_toml_continues() -> Result<()> {
        // Test that loading continues even if one file is invalid
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        let files = [
            (
                "bad.toml", // Invalid TOML
                r#"
                 description = "Bad TOML file
                 command = "echo This is broken"
             "#,
            ),
            (
                "good.toml", // Valid TOML
                r#"
                description = "This one is okay"
                command = "echo OK"
                "#,
            ),
        ];
        setup_test_config(dir_path, &files)?;

        // Expect load_commands to warn (in debug) but succeed and load the valid file
        let commands = load_commands(dir_path)?;
        assert_eq!(commands.len(), 1, "Should load only the valid command");
        assert!(commands.contains_key("good"));
        assert_eq!(
            commands.get("good").unwrap().description,
            "This one is okay"
        );

        Ok(())
    }

    #[test]
    fn test_find_command_definition_success() {
        let mut commands = HashMap::new();
        commands.insert(
            "test_cmd".to_string(),
            CommandDef {
                description: "A test command".to_string(),
                command: "echo test".to_string(),
            },
        );

        let result = find_command_definition("test_cmd", &commands);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().description, "A test command");
    }

    #[test]
    fn test_find_command_definition_not_found() {
        let commands: HashMap<String, CommandDef> = HashMap::new(); // Empty map

        let result = find_command_definition("non_existent", &commands);
        assert!(result.is_err());
        // Optional: check error message contains "not found"
        assert!(result.err().unwrap().to_string().contains("not found"));
    }
}
