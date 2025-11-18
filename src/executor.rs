use crate::types::CommandDef;
use anyhow::{Context, Result, bail};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

#[derive(Debug, PartialEq)]
enum Shell {
    Bash,
    Zsh,
    Unknown,
}

/// Detects which shell is being used by checking the SHELL environment variable.
fn detect_shell() -> Shell {
    if let Ok(shell_path) = env::var("SHELL") {
        if shell_path.ends_with("/bash") {
            return Shell::Bash;
        } else if shell_path.ends_with("/zsh") {
            return Shell::Zsh;
        }
    }
    Shell::Unknown
}

/// Gets the path to the shell history file based on the detected shell.
fn get_history_file_path(shell: &Shell) -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    let home_path = PathBuf::from(home);

    match shell {
        Shell::Bash => Some(home_path.join(".bash_history")),
        Shell::Zsh => Some(home_path.join(".zsh_history")),
        Shell::Unknown => None,
    }
}

/// Appends the executed command to the shell's history file.
fn append_to_shell_history(shell: &Shell, command: &str) -> Result<()> {
    let history_path =
        get_history_file_path(shell).context("Unable to determine history file path")?;

    // Open file in append mode
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&history_path)
        .context("Failed to open history file for appending")?;

    // Format the entry based on the shell type
    match shell {
        Shell::Zsh => {
            // Zsh extended history format: `: timestamp:duration;command`
            // Use current timestamp
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let entry = format!(": {timestamp}:0;{command}\n");
            file.write_all(entry.as_bytes())
                .context("Failed to write to zsh history file")?;
        }
        Shell::Bash => {
            // Bash history is simple: one command per line
            file.write_all(command.as_bytes())
                .context("Failed to write to bash history file")?;
            file.write_all(b"\n")
                .context("Failed to write newline to bash history file")?;
        }
        Shell::Unknown => {
            return Ok(());
        }
    }

    // Ensure all data is flushed to disk before returning
    file.sync_all()
        .context("Failed to sync history file to disk")?;

    Ok(())
}

/// Executes the specified command snippet.
pub fn execute_command(cmd_def: &CommandDef, overwrite_shell_history: bool) -> Result<()> {
    #[cfg(debug_assertions)]
    println!(
        "Executing '{}' (from {})",
        cmd_def.description,
        cmd_def.source_file.display()
    );

    // Append to shell history BEFORE command executes
    // This works because:
    // 1. We append the selected command to the history file
    // 2. The command executes
    // 3. When the user presses up-arrow, the shell reads from the file and sees the new entry
    // 4. The executed command appears as the most recent history item
    if overwrite_shell_history {
        let shell = detect_shell();
        if shell != Shell::Unknown {
            if let Err(e) = append_to_shell_history(&shell, &cmd_def.command) {
                eprintln!("Warning: Failed to append to shell history: {e}");
            }
        }
    }

    // Use the base command defined in the snippet
    let command_to_run = cmd_def.command.clone();

    #[cfg(debug_assertions)]
    println!("  Final Command String: {command_to_run}");

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

    // Execute, inheriting IO streams
    let status = cmd_process
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to start command snippet '{}'", cmd_def.description))?;

    if !status.success() {
        bail!(
            "Command snippet '{}' failed with status: {}",
            cmd_def.description,
            status
        );
    }

    Ok(())
}
// --- Tests for executor ---
// Only run on non-Windows platforms where `sh -c` is available
#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;
    use crate::types::CommandDef;
    use std::path::PathBuf;

    #[test]
    fn test_execute_command_success() {
        let cmd = CommandDef {
            description: "success".to_string(),
            command: "true".to_string(),
            source_file: PathBuf::from("dummy.toml"),
            tags: Vec::new(),
        };
        // Should return Ok for exit status 0
        assert!(execute_command(&cmd, false).is_ok());
    }

    #[test]
    fn test_execute_command_failure() {
        let cmd = CommandDef {
            description: "failure".to_string(),
            command: "false".to_string(),
            source_file: PathBuf::from("dummy.toml"),
            tags: Vec::new(),
        };
        // Should return Err for non-zero exit status
        let err = execute_command(&cmd, false).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("failed with status"),
            "unexpected error: {msg}"
        );
    }
}
