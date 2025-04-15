use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use dirs; // Import the dirs crate
use regex::Regex; // For validation
use serde::Deserialize;
use std::{
    collections::HashMap,
    fmt, // For custom Display impl
    fs,
    // No longer need net::IpAddr specifically for validation here
    path::{Path, PathBuf},
    process::Command as ProcessCommand, // Alias standard Command
                                        // No longer need FromStr for IpAddr validation here
};

// --- Structs ---

// Defines validation rules for a parameter (simplified)
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")] // Allows TOML keys like regex = "..." or length { min=.., max=..}
pub enum ValidationRule {
    None, // No specific validation
    MinLength(usize),
    MaxLength(usize),
    Length { min: usize, max: usize },
    Regex(String), // Store the regex pattern as a string
                   // Removed Email, Domain, IpAddress
}

// Represents a single parameter definition in TOML
#[derive(Deserialize, Debug, Clone)]
pub struct ParamDef {
    pub name: String, // Identifier for the parameter (used for substitution)
    pub description: String,
    #[serde(default)] // Default to false if not specified
    pub required: bool,
    pub default: Option<String>, // Single default value if argument is missing
    #[serde(default)] // Default to empty vec if not specified
    pub allowed_values: Vec<String>, // Predefined list of allowed values
    #[serde(default = "default_validation_rule")] // Use a function for default
    pub validation: ValidationRule, // Validation rules
}

// Function to provide the default value for `validation`
fn default_validation_rule() -> ValidationRule {
    ValidationRule::None
}

// Represents the data loaded from a command's TOML file
#[derive(Deserialize, Debug, Clone)]
pub struct CommandDef {
    pub description: String,
    pub command: String,
    #[serde(default)] // Parameters are optional
    pub params: Vec<ParamDef>, // List of parameter definitions
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

    /// Arguments to pass to the command
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_args: Vec<String>,
}

// Custom error type for parameter validation
#[derive(Debug)]
enum ParamError {
    MissingRequired(String),
    InvalidValue {
        param_name: String,
        value: String,
        reason: String,
    },
    TooFewArguments(usize, usize),  // expected, got
    TooManyArguments(usize, usize), // expected, got
}

// Implement Display for nice error messages
impl fmt::Display for ParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamError::MissingRequired(name) => {
                write!(f, "Missing required parameter: '{}'", name)
            }
            ParamError::InvalidValue {
                param_name,
                value,
                reason,
            } => write!(
                f,
                "Invalid value for parameter '{}': '{}'. Reason: {}",
                param_name, value, reason
            ),
            ParamError::TooFewArguments(expected, got) => write!(
                f,
                "Too few arguments provided. Expected at least {}, got {}.",
                expected, got
            ),
            ParamError::TooManyArguments(expected, got) => write!(
                f,
                "Too many arguments provided. Expected at most {}, got {}.",
                expected, got
            ),
        }
    }
}

// Implement std::error::Error
impl std::error::Error for ParamError {}

// Removed EmailTarget struct

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

            // Process parameters and execute the command
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
        // It's not an error if the default dir doesn't exist, just return empty.
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
                    // Basic validation of parameter definitions
                    for param in &cmd_def.params {
                        if let ValidationRule::Regex(pattern) = &param.validation {
                            if Regex::new(pattern).is_err() {
                                eprintln!(
                                    "Warning: Invalid regex pattern '{}' for parameter '{}' in command '{}'. File: {}",
                                    pattern, param.name, name, path.display()
                                );
                                // Potentially skip loading this command or handle differently
                            }
                        }
                        // Add more validation if needed (e.g., default value matches allowed_values)
                    }

                    commands.insert(name, cmd_def);
                }
                Err(e) => {
                    // Provide more context on TOML parsing errors
                    eprintln!(
                        "Warning: Failed to parse TOML from file: {}. Error: {}",
                        path.display(),
                        e // Print the actual error
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

/// Validates a single value against parameter definition constraints.
fn validate_parameter_value(param: &ParamDef, value: &str) -> Result<(), ParamError> {
    // 1. Check against allowed_values if defined
    if !param.allowed_values.is_empty() && !param.allowed_values.contains(&value.to_string()) {
        return Err(ParamError::InvalidValue {
            param_name: param.name.clone(),
            value: value.to_string(),
            reason: format!("Value must be one of: {:?}", param.allowed_values),
        });
    }

    // 2. Apply validation rules
    match &param.validation {
        ValidationRule::None => {} // No validation needed
        ValidationRule::MinLength(min) => {
            if value.len() < *min {
                return Err(ParamError::InvalidValue {
                    param_name: param.name.clone(),
                    value: value.to_string(),
                    reason: format!("Minimum length required is {}", min),
                });
            }
        }
        ValidationRule::MaxLength(max) => {
            if value.len() > *max {
                return Err(ParamError::InvalidValue {
                    param_name: param.name.clone(),
                    value: value.to_string(),
                    reason: format!("Maximum length allowed is {}", max),
                });
            }
        }
        ValidationRule::Length { min, max } => {
            if value.len() < *min || value.len() > *max {
                return Err(ParamError::InvalidValue {
                    param_name: param.name.clone(),
                    value: value.to_string(),
                    reason: format!("Length must be between {} and {}", min, max),
                });
            }
        }
        ValidationRule::Regex(pattern_str) => {
            // Compile regex on demand - consider pre-compiling if performance is critical
            match Regex::new(pattern_str) {
                Ok(re) => {
                    if !re.is_match(value) {
                        return Err(ParamError::InvalidValue {
                            param_name: param.name.clone(),
                            value: value.to_string(),
                            reason: format!("Value does not match regex pattern: {}", pattern_str),
                        });
                    }
                }
                Err(_) => {
                    // This should ideally be caught during loading, but handle defensively
                    eprintln!(
                        "Warning: Invalid regex pattern '{}' encountered during validation for param '{}'. Skipping regex check.",
                        pattern_str, param.name
                    );
                    // It might be better to return an error here instead of silently skipping
                    return Err(ParamError::InvalidValue {
                        param_name: param.name.clone(),
                        value: value.to_string(),
                        reason: format!(
                            "Internal error: Invalid regex pattern configured: {}",
                            pattern_str
                        ),
                    });
                }
            }
        } // Removed Email, Domain, IpAddress match arms
    }

    Ok(())
}

/// Processes arguments based on parameter definitions and substitutes them into the command string.
fn process_and_substitute_args(cmd_def: &CommandDef, cmd_args: &[String]) -> Result<String> {
    let mut processed_args = HashMap::new();
    let num_params = cmd_def.params.len();
    let num_args = cmd_args.len();

    // --- Argument Count Check ---
    let required_params = cmd_def.params.iter().filter(|p| p.required).count();
    if num_args < required_params {
        bail!(ParamError::TooFewArguments(required_params, num_args));
    }
    if num_args > num_params {
        // Allow extra args only if there are no defined params, otherwise it's an error
        // Or, could add a specific flag to allow extra args later.
        if num_params > 0 {
            bail!(ParamError::TooManyArguments(num_params, num_args));
        }
        // If no params defined, let all args pass through (or handle differently if needed)
    }

    // --- Process each defined parameter ---
    for (i, param) in cmd_def.params.iter().enumerate() {
        let value = if i < num_args {
            // Argument provided by user
            let user_value = &cmd_args[i];
            validate_parameter_value(param, user_value).map_err(|e| anyhow!(e))?; // Convert ParamError to anyhow::Error
            user_value.clone()
        } else {
            // Argument not provided, check for default or required
            if let Some(default_value) = &param.default {
                // Use default, but still validate it (in case default is invalid)
                validate_parameter_value(param, default_value)
                    .map_err(|e| anyhow!("Invalid default value for '{}': {}", param.name, e))?;
                default_value.clone()
            } else if param.required {
                // Should have been caught by the initial count check, but defensive check
                bail!(ParamError::MissingRequired(param.name.clone()));
            } else {
                // Optional parameter without default, treat as empty string or skip substitution?
                // Let's treat as empty for substitution purposes.
                String::new()
            }
        };
        processed_args.insert(param.name.clone(), value);
    }

    // --- Substitution ---
    let mut final_command = cmd_def.command.clone();
    for (name, value) in processed_args {
        let placeholder = format!("{{{}}}", name); // e.g., {filename}
                                                   // Ensure value is properly escaped for shell command if necessary.
                                                   // Simple replacement might be okay if commands are trusted, but consider
                                                   // using libraries like `shellexpand` or careful quoting if values
                                                   // can contain special characters. For now, direct replacement.
        final_command = final_command.replace(&placeholder, &value);
    }

    // --- Handle potential leftover placeholders ---
    // This regex finds patterns like {some_name}
    let placeholder_re = Regex::new(r"\{[a-zA-Z0-9_]+\}").unwrap();
    if placeholder_re.is_match(&final_command) {
        // Find the first unmatched placeholder for a clearer error
        let unmatched = placeholder_re
            .find(&final_command)
            .map_or("unknown", |m| m.as_str());
        bail!("Command template still contains unsubstituted placeholder(s) like '{}'. Check parameter definitions and names.", unmatched);
    }

    // --- Handle extra arguments if no params were defined ---
    // If cmd_def.params is empty, append all cmd_args directly?
    // Current logic errors if args > params when params are defined.
    // If you want to allow extra args to be appended when NO params are defined:
    // if num_params == 0 && num_args > 0 {
    //     // Add the shellexpand crate: `shellexpand = "3.1"`
    //     use shellexpand;
    //     for arg in cmd_args {
    //         final_command.push(' ');
    //         // Basic shell escaping (might need more robust solution depending on shell/OS)
    //         final_command.push_str(&shellexpand::escape(arg));
    //     }
    // }

    Ok(final_command)
}

/// Executes the specified command using its definition and processed arguments.
pub fn execute_command(
    name: &str,
    cmd_def: &CommandDef,
    cmd_args: &[String], // Raw arguments from the user
) -> Result<()> {
    #[cfg(debug_assertions)]
    println!("Executing '{}': {}", name, cmd_def.description);

    // Process arguments, validate, and substitute into the command template
    let command_to_run = process_and_substitute_args(cmd_def, cmd_args)
        .with_context(|| format!("Failed to process arguments for command '{}'", name))?;

    #[cfg(debug_assertions)]
    println!("  Processed Command: {}", command_to_run);

    // Execute the final command string
    let mut cmd_process = if cfg!(target_os = "windows") {
        let mut cmd = ProcessCommand::new("cmd");
        cmd.args(["/C", &command_to_run]);
        cmd
    } else {
        let mut cmd = ProcessCommand::new("sh");
        cmd.arg("-c");
        cmd.arg(&command_to_run);
        cmd
    };

    // Consider inheriting stdio for interactive commands or capturing output
    // cmd_process.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let status = cmd_process
        .status()
        .with_context(|| format!("Failed to start command '{}'", name))?;

    if !status.success() {
        // Provide more info if the command fails (e.g., exit code)
        bail!("Command '{}' failed with status: {}", name, status);
    }

    #[cfg(debug_assertions)]
    println!("Command '{}' executed successfully.", name);
    Ok(())
}

/// Prints a list of available commands and their parameters.
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
            // List parameters if any
            if !cmd_def.params.is_empty() {
                print!("    Params: ");
                let param_details: Vec<String> = cmd_def
                    .params
                    .iter()
                    .map(|p| {
                        let req = if p.required { "*" } else { "" };
                        let def = p
                            .default
                            .as_ref()
                            .map_or("".to_string(), |d| format!("[={}]", d));
                        // Use name for the placeholder indicator
                        format!("<{}{}{}>", p.name, req, def)
                    })
                    .collect();
                println!("{}", param_details.join(" "));
                // Optionally print descriptions too
                for p in &cmd_def.params {
                    println!("      {:<13} - {}", p.name, p.description);
                    if !p.allowed_values.is_empty() {
                        println!("        Allowed: {:?}", p.allowed_values);
                    }
                    if p.validation != ValidationRule::None {
                        // Make validation output slightly cleaner
                        let validation_str = match &p.validation {
                            ValidationRule::None => "None".to_string(), // Should not happen if check above works, but safe
                            ValidationRule::MinLength(v) => format!("MinLength({})", v),
                            ValidationRule::MaxLength(v) => format!("MaxLength({})", v),
                            ValidationRule::Length { min, max } => {
                                format!("Length({}, {})", min, max)
                            }
                            ValidationRule::Regex(v) => format!("Regex('{}')", v),
                            // Removed Email, Domain, IpAddress formatting
                        };
                        println!("        Validation: {}", validation_str);
                    }
                }
            }
        }
    }
    println!("\nRun 'cmdy <command_name> [args...]' to execute.");
    println!("Use 'cmdy --dir <directory>' to load commands from a different location.");
    println!("'*' indicates a required parameter. '[=default]' indicates a default value.");
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

    #[test]
    fn test_determine_config_directory_default() -> Result<()> {
        let cli_dir = None;
        let result = determine_config_directory(&cli_dir)?;

        if cfg!(target_os = "macos") {
            if let Some(mut expected_base) = dirs::home_dir() {
                expected_base.push(".config");
                expected_base.push("cmdy");
                expected_base.push("commands");
                assert_eq!(result, expected_base, "Test failed on macOS");
            } else {
                assert_eq!(
                    result,
                    PathBuf::from("./commands"),
                    "Test fallback failed on macOS"
                );
            }
        } else {
            if let Some(mut expected_base) = dirs::config_dir() {
                expected_base.push("cmdy");
                expected_base.push("commands");
                assert_eq!(result, expected_base, "Test failed on non-macOS");
            } else {
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
    fn test_load_commands_with_params_success() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();

        let files = [(
            "greet.toml",
            r#"
                description = "Greets someone"
                command = "echo Hello, {name}! You are {age} years old."
                [[params]]
                name = "name"
                description = "The name to greet"
                required = true
                validation = { min_length = 2 }

                [[params]]
                name = "age"
                description = "The person's age"
                required = false
                default = "unknown"
                # Note: TOML requires escaping backslashes in strings
                validation = { regex = "^\\d+$" } # Must be digits
            "#,
        )];
        setup_test_config(dir_path, &files)?;

        let commands = load_commands(dir_path)?;
        assert_eq!(commands.len(), 1);
        assert!(commands.contains_key("greet"));
        let cmd_def = commands.get("greet").unwrap();
        assert_eq!(cmd_def.params.len(), 2);
        assert_eq!(cmd_def.params[0].name, "name");
        assert!(cmd_def.params[0].required);
        assert_eq!(cmd_def.params[0].validation, ValidationRule::MinLength(2));
        assert_eq!(cmd_def.params[1].name, "age");
        assert!(!cmd_def.params[1].required);
        assert_eq!(cmd_def.params[1].default, Some("unknown".to_string()));
        // Ensure the regex string is loaded correctly (double backslash in TOML -> single in Rust string)
        assert_eq!(
            cmd_def.params[1].validation,
            ValidationRule::Regex("^\\d+$".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_load_commands_invalid_toml_syntax() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();
        let files = [("bad_syntax.toml", "description = No quotes\ncommand = foo")];
        setup_test_config(dir_path, &files)?;

        // Should print a warning (in debug) but return Ok
        let commands = load_commands(dir_path)?;
        // It should fail parsing and not load the command
        assert!(
            commands.is_empty(),
            "Invalid TOML syntax should prevent loading"
        );
        Ok(())
    }

    #[test]
    fn test_load_commands_invalid_param_regex() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir_path = temp_dir.path();
        let files = [(
            "bad_regex.toml",
            r#"
                description = "Command with bad regex"
                command = "echo {val}"
                [[params]]
                name = "val"
                description = "Value with bad regex"
                validation = { regex = "[" } # Invalid regex
            "#,
        )];
        setup_test_config(dir_path, &files)?;

        // Should print a warning (in debug) but still load the command
        let commands = load_commands(dir_path)?;
        assert_eq!(commands.len(), 1);
        assert!(commands.contains_key("bad_regex"));
        // Validation will fail later at runtime if regex isn't fixed
        Ok(())
    }

    #[test]
    fn test_process_args_success_required_and_default() -> Result<()> {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --name={name} --age={age}".into(),
            params: vec![
                ParamDef {
                    name: "name".into(),
                    description: "".into(),
                    required: true,
                    default: None,
                    allowed_values: vec![],
                    validation: ValidationRule::None,
                },
                ParamDef {
                    name: "age".into(),
                    description: "".into(),
                    required: false,
                    default: Some("30".into()),
                    allowed_values: vec![],
                    validation: ValidationRule::None,
                },
            ],
        };
        let args = vec!["Alice".to_string()];
        let result = process_and_substitute_args(&cmd_def, &args)?;
        assert_eq!(result, "cmd --name=Alice --age=30");
        Ok(())
    }

    #[test]
    fn test_process_args_success_all_provided() -> Result<()> {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --name={name} --age={age}".into(),
            params: vec![
                ParamDef {
                    name: "name".into(),
                    description: "".into(),
                    required: true,
                    default: None,
                    allowed_values: vec![],
                    validation: ValidationRule::None,
                },
                ParamDef {
                    name: "age".into(),
                    description: "".into(),
                    required: false,
                    default: Some("30".into()),
                    allowed_values: vec![],
                    validation: ValidationRule::None,
                },
            ],
        };
        let args = vec!["Bob".to_string(), "45".to_string()];
        let result = process_and_substitute_args(&cmd_def, &args)?;
        assert_eq!(result, "cmd --name=Bob --age=45");
        Ok(())
    }

    #[test]
    fn test_process_args_missing_required() {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --name={name}".into(),
            params: vec![ParamDef {
                name: "name".into(),
                description: "".into(),
                required: true,
                default: None,
                allowed_values: vec![],
                validation: ValidationRule::None,
            }],
        };
        let args = vec![]; // Missing required arg
        let result = process_and_substitute_args(&cmd_def, &args);
        assert!(result.is_err()); // Check it's an error first
        let err_ref = result.as_ref().unwrap_err(); // Borrow the error
        assert!(err_ref.downcast_ref::<ParamError>().is_some()); // Check it's our specific error type
        assert!(err_ref.to_string().contains("Too few arguments")); // Check the error message
    }

    #[test]
    fn test_process_args_too_many_args() {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --name={name}".into(),
            params: vec![ParamDef {
                name: "name".into(),
                description: "".into(),
                required: true,
                default: None,
                allowed_values: vec![],
                validation: ValidationRule::None,
            }],
        };
        let args = vec!["Alice".to_string(), "extra".to_string()]; // Too many args
        let result = process_and_substitute_args(&cmd_def, &args);
        assert!(result.is_err()); // Check it's an error first
        let err_ref = result.as_ref().unwrap_err(); // Borrow the error
        assert!(err_ref.downcast_ref::<ParamError>().is_some()); // Check it's our specific error type
        assert!(err_ref.to_string().contains("Too many arguments")); // Check the error message
    }

    #[test]
    fn test_process_args_validation_allowed_values_pass() -> Result<()> {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --mode={mode}".into(),
            params: vec![ParamDef {
                name: "mode".into(),
                description: "".into(),
                required: true,
                default: None,
                allowed_values: vec!["read".into(), "write".into()],
                validation: ValidationRule::None,
            }],
        };
        let args = vec!["read".to_string()];
        let result = process_and_substitute_args(&cmd_def, &args)?;
        assert_eq!(result, "cmd --mode=read");
        Ok(())
    }

    #[test]
    fn test_process_args_validation_allowed_values_fail() {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --mode={mode}".into(),
            params: vec![ParamDef {
                name: "mode".into(),
                description: "".into(),
                required: true,
                default: None,
                allowed_values: vec!["read".into(), "write".into()],
                validation: ValidationRule::None,
            }],
        };
        let args = vec!["delete".to_string()]; // Not allowed
        let result = process_and_substitute_args(&cmd_def, &args);
        assert!(result.is_err());
        let err_ref = result.as_ref().unwrap_err(); // Borrow
        assert!(err_ref.downcast_ref::<ParamError>().is_some());
        assert!(err_ref
            .to_string()
            .contains("Invalid value for parameter 'mode'"));
        assert!(err_ref.to_string().contains("must be one of"));
    }

    #[test]
    fn test_process_args_validation_regex_pass() -> Result<()> {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --id={id}".into(),
            params: vec![
                // Note: Double backslash needed for TOML literal string, becomes single in Rust
                ParamDef {
                    name: "id".into(),
                    description: "".into(),
                    required: true,
                    default: None,
                    allowed_values: vec![],
                    validation: ValidationRule::Regex("^[a-z]{3}-\\d{4}$".into()),
                }, // e.g., abc-1234
            ],
        };
        let args = vec!["xyz-9876".to_string()];
        let result = process_and_substitute_args(&cmd_def, &args)?;
        assert_eq!(result, "cmd --id=xyz-9876");
        Ok(())
    }

    #[test]
    fn test_process_args_validation_regex_fail() {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --id={id}".into(),
            params: vec![ParamDef {
                name: "id".into(),
                description: "".into(),
                required: true,
                default: None,
                allowed_values: vec![],
                validation: ValidationRule::Regex("^[a-z]{3}-\\d{4}$".into()),
            }],
        };
        let args = vec!["abc-defg".to_string()]; // Fails regex
        let result = process_and_substitute_args(&cmd_def, &args);
        assert!(result.is_err());
        let err_ref = result.as_ref().unwrap_err(); // Borrow
        assert!(err_ref.downcast_ref::<ParamError>().is_some());
        assert!(err_ref
            .to_string()
            .contains("Invalid value for parameter 'id'"));
        assert!(err_ref.to_string().contains("does not match regex pattern"));
    }

    #[test]
    fn test_process_args_validation_invalid_regex_config_fail() {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --id={id}".into(),
            params: vec![
                // Invalid regex pattern defined
                ParamDef {
                    name: "id".into(),
                    description: "".into(),
                    required: true,
                    default: None,
                    allowed_values: vec![],
                    validation: ValidationRule::Regex("[".into()),
                },
            ],
        };
        let args = vec!["some-value".to_string()]; // Value doesn't matter here
        let result = process_and_substitute_args(&cmd_def, &args);
        assert!(result.is_err());
        let err_ref = result.as_ref().unwrap_err(); // Borrow
        assert!(err_ref.downcast_ref::<ParamError>().is_some());
        assert!(err_ref
            .to_string()
            .contains("Invalid value for parameter 'id'"));
        // Check that the reason indicates an internal configuration error
        assert!(err_ref
            .to_string()
            .contains("Internal error: Invalid regex pattern configured"));
    }

    // Removed email and IP validation tests

    #[test]
    fn test_process_args_unmatched_placeholder() {
        let cmd_def = CommandDef {
            description: "Test".into(),
            command: "cmd --name={name} --extra={unprovided}".into(), // {unprovided} won't be filled
            params: vec![
                ParamDef {
                    name: "name".into(),
                    description: "".into(),
                    required: true,
                    default: None,
                    allowed_values: vec![],
                    validation: ValidationRule::None,
                },
                // No definition for "unprovided"
            ],
        };
        let args = vec!["Alice".to_string()];
        let result = process_and_substitute_args(&cmd_def, &args);
        assert!(result.is_err());
        // This error comes directly from bail!, not ParamError, so no downcast needed.
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsubstituted placeholder(s) like '{unprovided}'"));
    }

    // Note: execute_command tests are harder without mocking std::process::Command
    // Focus tests on load_commands and process_and_substitute_args
}
