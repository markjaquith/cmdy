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

/// Represents a single command snippet definition within a TOML file.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)] // Error if unknown fields are in TOML
pub struct CommandSnippet {
    /// The unique name used to invoke this command snippet.
    pub name: String,
    /// A short description of what the command does.
    pub description: String,
    /// The actual shell command string to execute.
    pub command: String,
}

/// Represents the structure of a TOML file containing one or more command snippets.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)] // Error if unknown fields are in TOML
struct FileDef {
    /// A list of command snippets defined in this file.
    // Allows using [[snippets]] or [[commands]] or [[snippet]] etc. in the TOML
    #[serde(alias = "snippet", alias = "command", alias = "cmd")]
    snippets: Vec<CommandSnippet>,
}

/// Represents the fully loaded command definition, including its source.
#[derive(Debug, Clone)]
pub struct CommandDef {
    /// A short description of what the command does.
    pub description: String,
    /// The actual shell command string to execute.
    pub command: String,
    /// The path to the TOML file where this command was defined.
    pub source_file: PathBuf,
}

/// Defines the command-line arguments your tool accepts.
#[derive(Parser, Debug)]
#[command(name = "cmdy", author, version, about = "Runs predefined command snippets.", long_about = None)]
struct CliArgs {
    /// The name of the command snippet to execute
    command_name: Option<String>, // Make optional to allow listing commands

    /// Optional directory to load command definitions from.
    /// Defaults to standard config locations based on OS.
    #[arg(long, value_name = "DIRECTORY")]
    dir: Option<PathBuf>,

    /// Arguments to append to the command snippet's command string.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_args: Vec<String>,
}

// --- Main Logic ---

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();
    let config_dir = determine_config_directory(&cli_args.dir)?;

    #[cfg(debug_assertions)]
    println!("Using configuration directory: {:?}", config_dir);

    // Load commands, now keyed by snippet name from within the files
    let commands = load_commands(&config_dir)
        .with_context(|| format!("Failed to load command definitions from {:?}", config_dir))?;

    match cli_args.command_name {
        Some(name) => {
            // Find the command definition by its snippet name
            let cmd_def = find_command_definition(&name, &commands)?;
            // Execute with appended args
            execute_command(&name, cmd_def, &cli_args.command_args)
                .with_context(|| format!("Failed to execute command snippet '{}'", name))?;
        }
        None => {
            // List available commands if no name is provided
            // print_banner();
            list_available_commands(&commands, &config_dir);
        }
    }

    Ok(())
}

// --- Helper Functions ---

/// Prints the application banner and information.
//fn print_banner() {
//    println!("Cmdy");
//    println!("(it’s pronounced “commandy”)");
//    println!();
//    println!("(C) 2022-2025 Mark Jaquith");
//}

/// Determines the directory to load command definitions from.
/// Uses the `--dir` flag if provided, otherwise determines a default
/// based on the operating system conventions.
fn determine_config_directory(cli_dir_flag: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = cli_dir_flag {
        // If a directory is specified via flag, use it directly.
        Ok(dir.clone())
    } else {
        // Otherwise, determine the default configuration directory based on OS.
        let default_path = if cfg!(target_os = "macos") {
            // On macOS, prefer ~/.config/cmdy/commands
            dirs::home_dir().map(|mut path| {
                path.push(".config"); // Using .config for consistency across platforms
                path.push("cmdy");
                path.push("commands");
                path
            })
        } else {
            // On Linux/other Unix-like, use the standard config dir (e.g., ~/.config/cmdy/commands)
            // On Windows, use the standard config dir (e.g., %APPDATA%\cmdy\commands)
            dirs::config_dir().map(|mut path| {
                path.push("cmdy");
                path.push("commands");
                path
            })
        };

        match default_path {
            Some(path) => Ok(path),
            None => {
                // Fallback if we can't determine home/config directory.
                #[cfg(debug_assertions)]
                eprintln!("Warning: Could not determine standard home or config directory. Falling back to './commands'.");
                Ok(PathBuf::from("./commands")) // Use a relative path as a last resort
            }
        }
    }
}

// --- Core Functions ---

/// Loads all command snippets from `.toml` files in the specified directory.
/// Returns a HashMap where the key is the snippet `name` and the value is the `CommandDef`.
pub fn load_commands(dir: &Path) -> Result<HashMap<String, CommandDef>> {
    let mut commands = HashMap::new();

    #[cfg(debug_assertions)]
    {
        // Attempt to get canonical path for clearer debug messages
        let canonical_dir_display = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        println!(
            "Attempting to load command snippets from: {}",
            canonical_dir_display.display()
        );
    }

    // Check if the directory exists. If not, return an empty map.
    if !dir.is_dir() {
        #[cfg(debug_assertions)]
        {
            let canonical_dir_display = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            eprintln!(
                "Info: Configuration directory not found at {}. No commands loaded from this location.",
                canonical_dir_display.display()
            );
        }
        // It's not an error if the directory doesn't exist, just means no commands are loaded from there.
        return Ok(commands);
    }

    // Iterate through entries in the specified directory.
    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Process only files with a `.toml` extension.
        if path.is_file() && path.extension().map_or(false, |ext| ext == "toml") {
            let file_content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read command file: {}", path.display()))?;

            // Attempt to parse the TOML file content into our FileDef structure.
            match toml::from_str::<FileDef>(&file_content) {
                Ok(file_def) => {
                    // Successfully parsed the file, now process each snippet within it.
                    for snippet in file_def.snippets {
                        let snippet_name = snippet.name; // Get the name defined in the snippet

                        // Check for duplicate snippet names across all loaded files.
                        if commands.contains_key(&snippet_name) {
                            // Found a duplicate name. Report an error with context.
                            let existing_cmd = &commands[&snippet_name];
                            bail!(
                                "Duplicate command snippet name '{}' found.\n  Defined in: {}\n  Also defined in: {}",
                                snippet_name,
                                path.display(), // Current file being processed
                                existing_cmd.source_file.display() // File where it was first defined
                            );
                        } else {
                            // Snippet name is unique, add it to the map.
                            #[cfg(debug_assertions)]
                            println!(
                                "  Loaded snippet '{}' from {}",
                                snippet_name,
                                path.file_name().unwrap_or_default().to_string_lossy() // Show filename for clarity
                            );

                            // Create the CommandDef, storing the necessary info.
                            let cmd_def = CommandDef {
                                description: snippet.description,
                                command: snippet.command,
                                source_file: path.clone(), // Store the path of the source file
                            };
                            commands.insert(snippet_name, cmd_def);
                        }
                    }
                }
                Err(e) => {
                    // Failed to parse the TOML file. Print a warning and skip this file.
                    eprintln!(
                        "Warning: Failed to parse TOML from file: {}. Error: {}",
                        path.display(),
                        e
                    );
                    // Optionally, return an error here if strict parsing is required:
                    // return Err(anyhow!("Failed to parse TOML from file: {}: {}", path.display(), e));
                }
            }
        }
    }
    Ok(commands) // Return the map of loaded commands
}

/// Finds the command definition struct for a given command snippet name.
pub fn find_command_definition<'a>(
    name: &str,
    commands: &'a HashMap<String, CommandDef>,
) -> Result<&'a CommandDef> {
    // Look up the command definition in the HashMap using the provided name.
    commands
        .get(name)
        .ok_or_else(|| anyhow!("Command snippet '{}' not found.", name)) // Return an error if not found
}

/// Executes the specified command snippet, appending any provided arguments safely quoted.
pub fn execute_command(name: &str, cmd_def: &CommandDef, cmd_args: &[String]) -> Result<()> {
    #[cfg(debug_assertions)]
    println!(
        "Executing '{}' (from {}): {}",
        name,
        cmd_def.source_file.display(), // Show source file in debug
        cmd_def.description
    );

    // Start with the base command defined in the TOML snippet.
    let mut command_to_run = cmd_def.command.clone();

    // Append each user-provided argument, escaped/quoted appropriately for the shell.
    for arg in cmd_args {
        command_to_run.push(' '); // Add a space separator before each argument.

        // Use shell_escape for robust Bourne shell (sh, bash, zsh, etc.) escaping on non-Windows.
        // For Windows (cmd.exe), basic quoting is used as shell_escape is POSIX-focused.
        if cfg!(target_os = "windows") {
            // Basic quoting for cmd.exe: wrap in double quotes if it contains spaces, quotes, or is empty.
            // This is a heuristic and might not cover all cmd.exe quoting complexities.
            if arg.is_empty() || arg.contains(char::is_whitespace) || arg.contains('"') {
                command_to_run.push('"');
                // Basic escape for internal quotes (double them up for cmd.exe) - still potentially imperfect.
                command_to_run.push_str(&arg.replace('"', "\"\""));
                command_to_run.push('"');
            } else {
                command_to_run.push_str(arg); // No special characters requiring quotes, append directly.
            }
        } else {
            // Use shell_escape::escape for POSIX shells.
            // It returns a Cow<str>, which dereferences to &str.
            // `.into()` converts the &String to the Cow<str> expected by escape.
            let escaped_arg = escape(arg.into());
            command_to_run.push_str(&escaped_arg);
        }
    }

    #[cfg(debug_assertions)]
    println!("  Final Command String: {}", command_to_run);

    // Determine the shell to use based on the OS.
    let mut cmd_process = if cfg!(target_os = "windows") {
        let mut cmd = ProcessCommand::new("cmd");
        cmd.args(["/C", &command_to_run]); // Use /C to execute the command string in cmd.exe.
        cmd
    } else {
        let mut cmd = ProcessCommand::new("sh");
        cmd.arg("-c"); // Use -c to execute the command string in sh/bash/zsh.
        cmd.arg(&command_to_run);
        cmd
    };

    // Execute the command, inheriting standard input, output, and error streams
    // so the command behaves interactively and its output/errors are visible.
    let status = cmd_process
        .stdin(Stdio::inherit()) // Pass our stdin to the command
        .stdout(Stdio::inherit()) // Pass command's stdout to ours
        .stderr(Stdio::inherit()) // Pass command's stderr to ours
        .status() // Execute and wait for the command to finish, getting its exit status.
        .with_context(|| format!("Failed to start command snippet '{}'", name))?;

    // Check if the command executed successfully.
    if !status.success() {
        // If the command failed, return an error with the exit status.
        bail!("Command snippet '{}' failed with status: {}", name, status);
    }

    #[cfg(debug_assertions)]
    println!("Command snippet '{}' executed successfully.", name);

    Ok(()) // Command executed successfully.
}

/// Prints a list of available command snippets and their descriptions.
pub fn list_available_commands(commands: &HashMap<String, CommandDef>, config_dir: &Path) {
    if commands.is_empty() {
        // Provide guidance if no commands were found.
        println!("No command snippets defined.");
        println!(
            "Looked for *.toml files containing [[snippets]] in: {}",
            config_dir.display()
        );
        println!("Create .toml files in this directory to define commands, for example:");
        println!("\n[[snippets]]"); // Use [[snippets]] to indicate array of tables
        println!("name = \"your-command-name\"");
        println!("description = \"Your command description\"");
        println!("command = \"your command string\"");
        return;
    }

    println!(
        "Available command snippets (from {}):",
        config_dir.display()
    );

    // Find the longest command name for alignment purposes.
    let max_name_len = commands.keys().map(|name| name.len()).max().unwrap_or(0);

    // Sort command names alphabetically for consistent listing.
    let mut names: Vec<_> = commands.keys().collect();
    names.sort();

    // Print each command's name and description, aligned.
    for name in names {
        if let Some(cmd_def) = commands.get(name) {
            // Get the filename for optional display
            let filename = cmd_def
                .source_file
                .file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_else(|| "<unknown>".into());
            // Print snippet name, filename (optional), and description
            // Adjust padding based on max_name_len for alignment
            println!(
                "  {:<width$} - {} (from {})", // {:<width$} left-aligns name with padding
                name,
                cmd_def.description,
                filename,
                width = max_name_len + 1 // Add 1 for a space after the longest name
            );
        }
    }

    // Print usage instructions.
    println!("\nRun 'cmdy <snippet_name> [args...]' to execute.");
    println!("Any [args...] will be appended to the command string.");
    println!("Use 'cmdy --dir <directory>' to load commands from a different location.");
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    use super::*; // Import items from the outer module (main code)
    use std::fs;
    use std::io::Write; // Import Write trait for writing to files
    use tempfile::tempdir; // Use tempfile crate for easy test setup/cleanup

    /// Helper function to create a temporary directory and TOML files for testing.
    /// `files` is a slice of tuples: `(filename, content)`.
    fn setup_test_config(dir_path: &Path, files: &[(&str, &str)]) -> Result<()> {
        fs::create_dir_all(dir_path)?; // Ensure the directory exists
        for (name, content) in files {
            // Ensure the filename ends with .toml
            let filename = if name.ends_with(".toml") {
                name.to_string()
            } else {
                format!("{}.toml", name)
            };
            let file_path = dir_path.join(filename);
            let mut file = fs::File::create(&file_path)
                .with_context(|| format!("Failed to create test file: {}", file_path.display()))?;
            // Write the provided TOML content to the file.
            writeln!(file, "{}", content)?;
        }
        Ok(())
    }

    #[test]
    /// Tests that the `--dir` flag correctly overrides the default config directory.
    fn test_determine_config_directory_flag_override() -> Result<()> {
        // Create a unique path for the test using tempdir
        let temp_dir = tempdir()?;
        let flag_path = temp_dir.path().join("custom_cmdy_dir_test");
        // Note: The directory doesn't *need* to exist for this specific test,
        // as determine_config_directory just returns the path provided by the flag.
        let cli_dir = Some(flag_path.clone());
        let result = determine_config_directory(&cli_dir)?;
        assert_eq!(result, flag_path); // Check if the returned path matches the flag path
        Ok(())
    }

    #[test]
    /// Tests that the default configuration directory logic works correctly.
    /// This test depends on the `dirs` crate and OS-specific behavior.
    fn test_determine_config_directory_default() -> Result<()> {
        let cli_dir = None; // No --dir flag provided
        let result = determine_config_directory(&cli_dir)?;

        // Determine the expected path based on OS, mirroring the main function's logic.
        let expected_path_opt = if cfg!(target_os = "macos") {
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

        match expected_path_opt {
            Some(expected) => assert_eq!(result, expected), // Check against the OS-specific default
            None => assert_eq!(result, PathBuf::from("./commands")), // Check the fallback path
        }
        Ok(())
    }

    #[test]
    /// Tests loading valid command snippets from multiple files.
    fn test_load_commands_success_multiple_files_and_snippets() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        // Define content for two separate TOML files.
        let file1_content = r#"
            [[snippets]]
            name = "greet"
            description = "Greets the world"
            command = "echo Hello World"

            [[snippets]]
            name = "list"
            description = "Lists files"
            command = "ls -l"
        "#;
        let file2_content = r#"
            [[snippets]]
            name = "show_date"
            description = "Shows the current date"
            command = "date"
        "#;

        let files = [("commands1", file1_content), ("commands2", file2_content)];
        setup_test_config(dir_path, &files)?;

        let commands = load_commands(dir_path)?;

        // Assertions: Check total count and presence of keys.
        assert_eq!(commands.len(), 3, "Should load 3 snippets in total");
        assert!(commands.contains_key("greet"));
        assert!(commands.contains_key("list"));
        assert!(commands.contains_key("show_date"));

        // Assertions: Check details of loaded commands.
        let greet_def = commands.get("greet").unwrap();
        assert_eq!(greet_def.description, "Greets the world");
        assert_eq!(greet_def.command, "echo Hello World");
        assert!(greet_def.source_file.ends_with("commands1.toml")); // Check source file

        let list_def = commands.get("list").unwrap();
        assert_eq!(list_def.description, "Lists files");
        assert_eq!(list_def.command, "ls -l");
        assert!(list_def.source_file.ends_with("commands1.toml"));

        let date_def = commands.get("show_date").unwrap();
        assert_eq!(date_def.description, "Shows the current date");
        assert_eq!(date_def.command, "date");
        assert!(date_def.source_file.ends_with("commands2.toml"));

        Ok(())
    }

    #[test]
    /// Tests loading commands with aliases for the snippet array key (e.g., [[cmd]]).
    fn test_load_commands_with_aliases() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        let file_content = r#"
            [[cmd]] # Using alias 'cmd' instead of 'snippets'
            name = "ping_google"
            description = "Pings Google DNS"
            command = "ping 8.8.8.8"
        "#;

        let files = [("network", file_content)];
        setup_test_config(dir_path, &files)?;

        let commands = load_commands(dir_path)?;

        assert_eq!(commands.len(), 1, "Should load 1 snippet using alias");
        assert!(commands.contains_key("ping_google"));
        let ping_def = commands.get("ping_google").unwrap();
        assert_eq!(ping_def.description, "Pings Google DNS");
        assert_eq!(ping_def.command, "ping 8.8.8.8");
        assert!(ping_def.source_file.ends_with("network.toml"));

        Ok(())
    }

    #[test]
    /// Tests that loading stops and returns an error if duplicate snippet names are found.
    fn test_load_commands_duplicate_snippet_name_error() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        // Define two files with a snippet having the same name "common".
        let file1_content = r#"
            [[snippets]]
            name = "common"
            description = "First definition"
            command = "echo first"
        "#;
        let file2_content = r#"
            [[snippets]]
            name = "another"
            description = "Another command"
            command = "echo another"

            [[snippets]]
            name = "common" # Duplicate name
            description = "Second definition"
            command = "echo second"
        "#;

        let files = [("file1", file1_content), ("file2", file2_content)];
        setup_test_config(dir_path, &files)?;

        // Loading should fail because "common" is defined twice.
        let result = load_commands(dir_path);

        assert!(
            result.is_err(),
            "Loading should return an error for duplicate names"
        );
        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("Duplicate command snippet name 'common' found"),
            "Error message should indicate the duplicate name"
        );
        // Check if the error message mentions both files (order might vary)
        assert!(error_message.contains("file1.toml"));
        assert!(error_message.contains("file2.toml"));

        Ok(()) // Test passes if the error is correctly generated
    }

    #[test]
    /// Tests that invalid TOML syntax causes a warning (in debug) but allows loading
    /// of other valid files/snippets (or returns an empty map if it's the only file).
    fn test_load_commands_invalid_toml_syntax_warning() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        // One file with invalid TOML, another with valid TOML.
        let invalid_content = "description = No quotes\ncommand = foo"; // Missing [[snippets]] and quotes
        let valid_content = r#"
            [[snippets]]
            name = "valid_cmd"
            description = "This one is okay"
            command = "echo ok"
        "#;

        let files = [
            ("bad_syntax", invalid_content),
            ("good_syntax", valid_content),
        ];
        setup_test_config(dir_path, &files)?;

        // load_commands should print a warning for bad_syntax.toml (in debug builds)
        // but still successfully load good_syntax.toml.
        let commands = load_commands(dir_path)?;

        assert_eq!(commands.len(), 1, "Only the valid snippet should be loaded");
        assert!(commands.contains_key("valid_cmd"));
        assert!(!commands.contains_key("bad_syntax")); // Ensure the invalid one wasn't loaded somehow

        Ok(())
    }

    #[test]
    /// Tests finding an existing command definition.
    fn test_find_command_definition_success() -> Result<()> {
        let mut commands = HashMap::new();
        let source_path = PathBuf::from("test.toml"); // Dummy path
        commands.insert(
            "hello".to_string(),
            CommandDef {
                description: "Says hello".to_string(),
                command: "echo hello".to_string(),
                source_file: source_path.clone(),
            },
        );
        commands.insert(
            "bye".to_string(),
            CommandDef {
                description: "Says goodbye".to_string(),
                command: "echo bye".to_string(),
                source_file: source_path,
            },
        );

        // Find the "hello" command.
        let found_def = find_command_definition("hello", &commands)?;
        assert_eq!(found_def.description, "Says hello");
        assert_eq!(found_def.command, "echo hello");
        Ok(())
    }

    #[test]
    /// Tests attempting to find a command definition that doesn't exist.
    fn test_find_command_definition_not_found() {
        let commands: HashMap<String, CommandDef> = HashMap::new(); // Empty map
        let result = find_command_definition("nonexistent", &commands);

        // Assert that the result is an error.
        assert!(result.is_err());
        // Assert that the error message contains the expected text.
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Command snippet 'nonexistent' not found."));
    }

    // Note: Testing `execute_command` directly remains complex due to mocking
    // `std::process::Command`. Integration tests (running the compiled binary
    // against test TOML files and checking output/exit codes) are generally
    // more practical for verifying the execution logic.
}
