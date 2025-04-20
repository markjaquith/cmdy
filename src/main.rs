use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use dirs;
use serde::Deserialize;
use shell_escape::escape;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio}, // Alias Command, Added Stdio
};

// --- Structs ---

// Represents the data loaded from a command's TOML file (Simplified)
#[derive(Deserialize, Debug, Clone)]
pub struct CommandDef {
    pub description: String,
    pub command: String,
    // No 'params' field anymore
}

// Defines the command-line arguments your tool accepts (Unchanged)
#[derive(Parser, Debug)]
#[command(name = "cmdy", author, version, about = "Runs predefined commands.", long_about = None)]
struct CliArgs {
    /// The name of the command command to execute
    command_name: Option<String>, // Make optional to allow listing commands

    /// Optional directory to load command definitions from.
    /// Defaults to standard config locations based on OS.
    #[arg(long, value_name = "DIRECTORY")]
    dir: Option<PathBuf>,

    /// Arguments to append to the command
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_args: Vec<String>,
}

// --- Main Logic ---

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();
    let config_dir = determine_config_directory(&cli_args.dir)?;

    #[cfg(debug_assertions)]
    println!("Using configuration directory: {:?}", config_dir);

    let commands = load_commands(&config_dir)
        .with_context(|| format!("Failed to load command definitions from {:?}", config_dir))?;

    match cli_args.command_name {
        Some(name) => {
            let cmd_def = find_command_definition(&name, &commands)?;
            // Directly execute with appended args
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

/// Prints the application banner and information. (Unchanged)
fn print_banner() {
    println!(r#" ██████ ███    ███ ██████  ██    ██ "#);
    println!(r#"██      ████  ████ ██   ██ ██  ██  "#);
    println!(r#"██      ██ ████ ██ ██   ██  ████   "#);
    println!(r#"██      ██  ██  ██ ██   ██   ██    "#);
    println!(r#" ██████ ██      ██ ██████    ██    "#);
    println!();
    println!("Your friendly command manager");
    println!();
    println!("(it’s pronounced “commandy”)");
    println!();
    println!("(C) 2022-2025 Mark Jaquith"); // Assuming current date is 2025 based on context prompt
}

/// Determines the directory to load command definitions from. (Unchanged)
fn determine_config_directory(cli_dir_flag: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = cli_dir_flag {
        Ok(dir.clone())
    } else {
        let default_path = if cfg!(target_os = "macos") {
            dirs::home_dir().map(|mut path| {
                path.push(".config"); // Use .config consistently
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
            None => {
                #[cfg(debug_assertions)]
                eprintln!("Warning: Could not determine standard home or config directory. Falling back to './commands'.");
                Ok(PathBuf::from("./commands"))
            }
        }
    }
}

// --- Core Functions ---

/// Loads all `.toml` files from the specified directory into a HashMap. (Logic Unchanged, uses simpler CommandDef)
pub fn load_commands(dir: &Path) -> Result<HashMap<String, CommandDef>> {
    let mut commands = HashMap::new();

    #[cfg(debug_assertions)]
    {
        // Attempt to get canonical path for clearer debug messages
        let canonical_dir_display = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        println!(
            "Attempting to load commands from: {}",
            canonical_dir_display.display()
        );
    }

    if !dir.is_dir() {
        #[cfg(debug_assertions)]
        {
            let canonical_dir_display = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            eprintln!(
                "Info: Configuration directory not found at {}. No commands loaded from this location.",
                canonical_dir_display.display()
            );
        }
        // It's not an error if the default dir doesn't exist, just return empty.
        return Ok(commands);
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

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
                // Parses the simplified CommandDef
                Ok(cmd_def) => {
                    #[cfg(debug_assertions)]
                    println!(
                        "  Loaded definition for '{}' from {:?}",
                        name,
                        path.file_name().unwrap_or_default() // Use default OsStr if no filename
                    );
                    commands.insert(name, cmd_def);
                }
                Err(e) => {
                    // Warning remains useful if TOML is malformed or has unexpected fields (like old 'params')
                    eprintln!(
                        "Warning: Failed to parse TOML from file: {}. Error: {}",
                        path.display(),
                        e
                    );
                    // Continue loading other files even if one fails to parse
                }
            }
        }
    }
    Ok(commands)
}

/// Finds the command definition struct for a given command name. (Unchanged)
pub fn find_command_definition<'a>(
    name: &str,
    commands: &'a HashMap<String, CommandDef>,
) -> Result<&'a CommandDef> {
    commands
        .get(name)
        .ok_or_else(|| anyhow!("Command '{}' not found.", name))
}

/// Executes the specified command, appending any provided arguments safely quoted.
pub fn execute_command(
    name: &str,
    cmd_def: &CommandDef,
    cmd_args: &[String], // Raw arguments from the user
) -> Result<()> {
    #[cfg(debug_assertions)]
    println!("Executing '{}': {}", name, cmd_def.description);

    // Start with the base command defined in the TOML
    let mut command_to_run = cmd_def.command.clone();

    // Append each user-provided argument, escaped/quoted for the shell
    for arg in cmd_args {
        command_to_run.push(' '); // Add a space separator

        if cfg!(target_os = "windows") {
            // Basic quoting for cmd.exe: wrap in double quotes if it contains spaces or is empty.
            // Note: This is a heuristic and might not cover all edge cases for cmd.exe's complex parsing.
            if arg.is_empty() || arg.contains(char::is_whitespace) || arg.contains('"') {
                // Added check for quotes
                command_to_run.push('"');
                // Basic escape for internal quotes (double them up for cmd.exe) - still imperfect
                command_to_run.push_str(&arg.replace('"', "\"\""));
                command_to_run.push('"');
            } else {
                command_to_run.push_str(arg); // No spaces or quotes, append directly
            }
        } else {
            // --- LLM CHANGE START ---
            // Use shell_escape::escape for robust Bourne shell (sh, bash, zsh) escaping.
            // It returns a Cow<str>, which dereferences to &str.
            // .into() converts the &String to the Cow<str> expected by escape.
            let escaped_arg = escape(arg.into());
            command_to_run.push_str(&escaped_arg);
            // --- LLM CHANGE END ---
        }
    }

    #[cfg(debug_assertions)]
    println!("  Final Command String: {}", command_to_run);

    // Execute the final command string using the shell
    let mut cmd_process = if cfg!(target_os = "windows") {
        let mut cmd = ProcessCommand::new("cmd");
        cmd.args(["/C", &command_to_run]); // Use /C to execute the command string
        cmd
    } else {
        let mut cmd = ProcessCommand::new("sh");
        cmd.arg("-c"); // Use -c to execute the command string
        cmd.arg(&command_to_run);
        cmd
    };

    // Inherit stdio for interactive commands or seeing output/errors
    let status = cmd_process
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status() // Execute and wait for status
        .with_context(|| format!("Failed to start command '{}'", name))?;

    if !status.success() {
        // Provide more info if the command fails
        bail!("Command '{}' failed with status: {}", name, status);
    }

    #[cfg(debug_assertions)]
    println!("Command '{}' executed successfully.", name);
    Ok(())
}

/// Prints a list of available commands and their descriptions. (Modified)
pub fn list_available_commands(commands: &HashMap<String, CommandDef>, config_dir: &Path) {
    if commands.is_empty() {
        println!("No commands defined.");
        println!("Looked for *.toml files in: {}", config_dir.display());
        println!("Create .toml files in this directory to define commands like:");
        println!("  description = \"Your command description\"");
        println!("  command = \"your command string\"");
        return;
    }

    println!("Available commands (from {}):", config_dir.display());
    let mut names: Vec<_> = commands.keys().collect();
    names.sort(); // Sort command names alphabetically for consistent listing
    for name in names {
        if let Some(cmd_def) = commands.get(name) {
            // Only print name and description, aligned
            println!("  {: <15} - {}", name, cmd_def.description);
            // Removed the parameter listing block
        }
    }
    println!("\nRun 'cmdy <command_name> [args...]' to execute.");
    println!("Any [args...] will be appended to the command string.");
    println!("Use 'cmdy --dir <directory>' to load commands from a different location.");
    // Removed help text about parameters
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
        fs::create_dir_all(dir_path)?;
        for (name, content) in files {
            // Ensure .toml extension is added here
            let file_path = dir_path.join(format!("{}.toml", name));
            let mut file = fs::File::create(&file_path)
                .with_context(|| format!("Failed to create test file: {}", file_path.display()))?;
            writeln!(file, "{}", content)?;
        }
        Ok(())
    }

    #[test]
    fn test_determine_config_directory_flag_override() -> Result<()> {
        // Use a more specific test path if possible
        let flag_path = tempdir()?.path().join("custom_cmdy_dir_test");
        // Ensure the path exists for canonicalization checks if needed, or handle errors
        // fs::create_dir_all(&flag_path)?;
        let cli_dir = Some(flag_path.clone());
        let result = determine_config_directory(&cli_dir)?;
        assert_eq!(result, flag_path);
        Ok(())
    }

    #[test]
    fn test_determine_config_directory_default() -> Result<()> {
        let cli_dir = None;
        let result = determine_config_directory(&cli_dir)?;

        // Logic based on dirs crate and OS specifics
        let expected_path = if cfg!(target_os = "macos") {
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

        match expected_path {
            Some(expected) => assert_eq!(result, expected),
            // Test fallback path if home/config dir cannot be determined
            None => assert_eq!(
                result,
                PathBuf::from("./commands"),
                "Fallback path check failed"
            ),
        }
        Ok(())
    }

    #[test]
    fn test_load_commands_success() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        let files = [
            (
                "greet", // Name only, extension added by helper
                r#"
                description = "Greets the world"
                command = "echo Hello World"
            "#,
            ),
            (
                "list",
                r#"
                description = "Lists files"
                command = "ls -l"
            "#,
            ),
        ];
        setup_test_config(dir_path, &files)?;

        let commands = load_commands(dir_path)?;
        assert_eq!(commands.len(), 2);
        assert!(commands.contains_key("greet"));
        assert!(commands.contains_key("list"));

        let greet_def = commands.get("greet").unwrap();
        assert_eq!(greet_def.description, "Greets the world");
        assert_eq!(greet_def.command, "echo Hello World");

        let list_def = commands.get("list").unwrap();
        assert_eq!(list_def.description, "Lists files");
        assert_eq!(list_def.command, "ls -l");

        Ok(())
    }

    #[test]
    fn test_load_commands_invalid_toml_syntax() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();
        // Pass name and content separately to helper
        let files = [("bad_syntax", "description = No quotes\ncommand = foo")];
        setup_test_config(dir_path, &files)?;

        // Should print a warning (in debug) but return Ok with 0 commands loaded
        let commands = load_commands(dir_path)?;
        assert!(
            commands.is_empty(),
            "Invalid TOML syntax should prevent loading"
        );
        Ok(())
    }

    #[test]
    fn test_find_command_definition_success() -> Result<()> {
        let mut commands = HashMap::new();
        commands.insert(
            "hello".to_string(),
            CommandDef {
                description: "Says hello".to_string(),
                command: "echo hello".to_string(),
            },
        );
        let found_def = find_command_definition("hello", &commands)?;
        assert_eq!(found_def.description, "Says hello");
        Ok(())
    }

    #[test]
    fn test_find_command_definition_not_found() {
        let commands: HashMap<String, CommandDef> = HashMap::new();
        let result = find_command_definition("goodbye", &commands);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Command 'goodbye' not found."));
    }

    // Tests for process_and_substitute_args and parameter validation REMOVED

    // Note: Testing execute_command directly is complex due to mocking std::process::Command.
    // Manual testing or integration tests are more practical for verifying execution.
}
