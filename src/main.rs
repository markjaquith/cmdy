use anyhow::{bail, Context, Result};
use clap::Parser;
use dirs;
use serde::Deserialize;
use shell_escape::escape;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write}, // For reading fzf output and writing to fzf stdin
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio}, // Alias Command, Added Stdio
};

// --- Structs ---

/// Represents a single command snippet definition within a TOML file.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)] // Error if unknown fields are in TOML
pub struct CommandSnippet {
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
#[command(name = "cmdy", author, version, about = "Lists and runs predefined command snippets.", long_about = None)]
struct CliArgs {
    /// Optional directory to load command definitions from.
    /// Defaults to standard config locations based on OS.
    #[arg(long, value_name = "DIRECTORY")]
    dir: Option<PathBuf>,

    /// Arguments to append to the selected command snippet's command string.
    /// These are passed directly to the executed command.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_args: Vec<String>,
}

// --- Main Logic ---

fn main() -> Result<()> {
    // Parse command-line arguments (primarily for --dir and trailing args)
    let cli_args = CliArgs::parse();

    // Determine the configuration directory to use
    let config_dir = determine_config_directory(&cli_args.dir)?;

    #[cfg(debug_assertions)]
    println!("Using configuration directory: {:?}", config_dir);

    // Load commands from TOML files into a temporary HashMap for duplicate checking
    let commands_map = load_commands(&config_dir)
        .with_context(|| format!("Failed to load command definitions from {:?}", config_dir))?;

    // Convert the HashMap into a Vec<CommandDef> for ordered display and selection.
    let mut commands_vec: Vec<CommandDef> = commands_map.into_values().collect();

    // Sort the commands alphabetically by name for a consistent numbered list.
    commands_vec.sort_by(|a, b| a.description.cmp(&b.description));

    // Display the list, prompt the user for selection, and execute the chosen command.
    // Pass the command-line arguments (`cli_args.command_args`) to be appended.
    select_and_execute_command(&commands_vec, &cli_args.command_args, &config_dir)
        .context("Failed during command selection or execution")?;

    Ok(())
}

// --- Helper Functions ---

/// Determines the directory to load command definitions from.
/// Uses the `--dir` flag if provided, otherwise determines a default
/// based on the operating system conventions (e.g., ~/.config/cmdy/commands).
fn determine_config_directory(cli_dir_flag: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = cli_dir_flag {
        // If a directory is specified via flag, use it directly.
        Ok(dir.clone())
    } else {
        // Otherwise, determine the default configuration directory based on OS.
        let default_path = if cfg!(target_os = "macos") {
            // On macOS, prefer ~/.config/cmdy/commands
            dirs::home_dir().map(|mut path| {
                path.push(".config"); // Using .config for consistency
                path.push("cmdy");
                path.push("commands");
                path
            })
        } else {
            // On Linux/other Unix-like/Windows, use the standard config dir
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
/// Returns a HashMap where the key is the snippet `description` and the value is the `CommandDef`.
/// The HashMap is used temporarily to easily check for duplicate names across files.
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
        // It's not an error if the directory doesn't exist.
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
                        let snippet_name = snippet.description;

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
                                path.file_name().unwrap_or_default().to_string_lossy() // Show filename
                            );

                            // Create the CommandDef, storing the necessary info including the name.
                            let cmd_def = CommandDef {
                                // FIX THIS
                                description: snippet_name.clone(),
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
                    // Continue processing other files.
                }
            }
        }
    }
    Ok(commands) // Return the map of loaded commands
}

/// Displays a numbered list of commands, prompts the user for selection,
/// reads the input, and executes the chosen command with provided arguments.
fn select_and_execute_command(
    commands_vec: &[CommandDef], // Takes a slice of the sorted CommandDefs
    cmd_args: &[String],         // Arguments from CLI to pass to the executed command
    config_dir: &Path,           // Directory where commands were loaded from (for display)
) -> Result<()> {
    // Handle the case where no commands were loaded.
    if commands_vec.is_empty() {
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
        return Ok(()); // Nothing to execute
    }

    // Use fzf for interactive command selection.
    let mut choice_map: HashMap<String, &CommandDef> = HashMap::new();
    for cmd_def in commands_vec.iter() {
        let filename = cmd_def
            .source_file
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_else(|| "<unknown>".into());
        let line = format!("{} (from {})", cmd_def.description, filename);
        choice_map.insert(line.clone(), cmd_def);
    }

    let mut fzf_child = ProcessCommand::new("fzf")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to spawn fzf for command selection")?;

    {
        let mut stdin = fzf_child
            .stdin
            .take()
            .context("Failed to open fzf stdin")?;
        for choice in choice_map.keys() {
            writeln!(stdin, "{}", choice).context("Failed to write to fzf stdin")?;
        }
    }

    let mut selected = String::new();
    {
        let mut stdout = fzf_child
            .stdout
            .take()
            .context("Failed to open fzf stdout")?;
        stdout
            .read_to_string(&mut selected)
            .context("Failed to read fzf output")?;
    }

    let status = fzf_child
        .wait()
        .context("Failed to wait for fzf process")?;
    if !status.success() {
        println!("No selection made. Exiting.");
        return Ok(());
    }

    let selected = selected.trim();
    let selected_cmd_def = choice_map
        .get(selected)
        .with_context(|| format!("Selected command '{}' not found", selected))?;

    return execute_command(selected_cmd_def, cmd_args).with_context(|| {
        format!("Failed to execute command snippet '{}'", selected_cmd_def.description)
    });
}

/// Executes the specified command snippet, appending any provided arguments safely quoted.
/// Now takes a reference to `CommandDef` directly.
pub fn execute_command(cmd_def: &CommandDef, cmd_args: &[String]) -> Result<()> {
    #[cfg(debug_assertions)]
    println!(
        "Executing '{}' (from {})",
        cmd_def.description, // Use description from CommandDef struct
        cmd_def.source_file.display()
    );

    // Start with the base command defined in the TOML snippet.
    let mut command_to_run = cmd_def.command.clone();

    // Append each user-provided argument (from CLI), escaped/quoted appropriately.
    for arg in cmd_args {
        command_to_run.push(' '); // Add a space separator.

        // Use shell_escape for POSIX shells, basic quoting for Windows cmd.exe.
        if cfg!(target_os = "windows") {
            // Basic quoting for cmd.exe: wrap in double quotes if needed.
            if arg.is_empty() || arg.contains(char::is_whitespace) || arg.contains('"') {
                command_to_run.push('"');
                // Basic escape for internal quotes (double them up).
                command_to_run.push_str(&arg.replace('"', "\"\""));
                command_to_run.push('"');
            } else {
                command_to_run.push_str(arg); // Append directly if no special chars.
            }
        } else {
            // Use shell_escape::escape for robust POSIX shell escaping.
            let escaped_arg = escape(arg.into()); // Converts &String to Cow<str>
            command_to_run.push_str(&escaped_arg);
        }
    }

    #[cfg(debug_assertions)]
    println!("  Final Command String: {}", command_to_run);

    // Determine the shell and arguments based on the OS.
    let mut cmd_process = if cfg!(target_os = "windows") {
        let mut cmd = ProcessCommand::new("cmd");
        cmd.args(["/C", &command_to_run]); // Use /C to execute the command string.
        cmd
    } else {
        let mut cmd = ProcessCommand::new("sh");
        cmd.arg("-c"); // Use -c to execute the command string.
        cmd.arg(&command_to_run);
        cmd
    };

    // Execute the command, inheriting standard I/O streams.
    let status = cmd_process
        .stdin(Stdio::inherit()) // Pass our stdin to the command
        .stdout(Stdio::inherit()) // Pass command's stdout to ours
        .stderr(Stdio::inherit()) // Pass command's stderr to ours
        .status() // Execute and wait for the exit status.
        .with_context(|| format!("Failed to start command snippet '{}'", cmd_def.description))?; // Use description from CommandDef

    // Check if the command executed successfully.
    if !status.success() {
        // If the command failed, return an error with the exit status.
        bail!(
            "Command snippet '{}' failed with status: {}",
            cmd_def.description,
            status
        );
    }

    Ok(()) // Command executed successfully.
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    use super::*; // Import items from the outer module
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

    // --- Tests for determine_config_directory ---
    // These tests remain unchanged as the function logic is the same.

    #[test]
    /// Tests that the `--dir` flag correctly overrides the default config directory.
    fn test_determine_config_directory_flag_override() -> Result<()> {
        let temp_dir = tempdir()?;
        let flag_path = temp_dir.path().join("custom_cmdy_dir_test");
        let cli_dir = Some(flag_path.clone());
        let result = determine_config_directory(&cli_dir)?;
        assert_eq!(result, flag_path); // Check if the returned path matches the flag path
        Ok(())
    }

    #[test]
    /// Tests that the default configuration directory logic works correctly.
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

    // --- Tests for load_commands ---
    // These tests remain largely the same, but check the fields of CommandDef.

    #[test]
    /// Tests loading valid command snippets from multiple files.
    fn test_load_commands_success_multiple_files_and_snippets() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        // Define content for two separate TOML files.
        let file1_content = r#"
            [[snippets]]
            description = "Greets the world"
            command = "echo Hello World"
            [[snippets]]
            description = "Lists files"
            command = "ls -l"
        "#;
        let file2_content = r#"
            [[snippets]]
            description = "Shows the current date"
            command = "date"
        "#;
        let files = [("commands1", file1_content), ("commands2", file2_content)];
        setup_test_config(dir_path, &files)?;

        let commands = load_commands(dir_path)?;

        // Assertions: Check total count and presence of keys in the map.
        assert_eq!(commands.len(), 3, "Should load 3 snippets in total");
        assert!(commands.contains_key("Greets the world"));
        assert!(commands.contains_key("Lists files"));
        assert!(commands.contains_key("Shows the current date"));

        // Assertions: Check details of loaded CommandDef structs.
        let greet_def = commands.get("Greets the world").unwrap();
        assert_eq!(greet_def.description, "Greets the world");
        assert_eq!(greet_def.command, "echo Hello World");
        assert!(greet_def.source_file.ends_with("commands1.toml")); // Check source file

        let list_def = commands.get("Lists files").unwrap();
        assert_eq!(list_def.description, "Lists files");
        assert_eq!(list_def.command, "ls -l");
        assert!(list_def.source_file.ends_with("commands1.toml"));

        let date_def = commands.get("Shows the current date").unwrap();
        assert_eq!(date_def.description, "Shows the current date");
        assert_eq!(date_def.command, "date");
        assert!(date_def.source_file.ends_with("commands2.toml"));

        Ok(())
    }

    #[test]
    /// Tests that invalid TOML syntax causes a warning (in debug) but allows loading
    /// of other valid files/snippets.
    fn test_load_commands_invalid_toml_syntax_warning() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();
        let invalid_content = "description = No quotes\ncommand = foo"; // Missing [[snippets]] etc.
        let valid_content = r#"
            [[snippets]]
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
        assert!(commands.contains_key("This one is okay"));
        assert!(!commands.contains_key("No quotes")); // Ensure the invalid one wasn't loaded

        Ok(())
    }

    // --- Tests removed/modified ---
    // - Removed tests for `find_command_definition` as it no longer exists.
    // - Unit testing `select_and_execute_command` is difficult due to its reliance on stdio.
    //   Integration tests (running the compiled binary and interacting with its prompt)
    //   would be more suitable for verifying this function.
    // - Unit testing `execute_command` remains complex due to mocking `std::process::Command`.
    //   Integration tests are generally more practical here as well.
}
