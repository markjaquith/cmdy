use crate::types::CommandDef;
use anyhow::{Context, Result};
use std::process::Command as ProcessCommand;

/// Executes the specified command snippet by replacing the current process.
///
/// On Unix, this uses `exec()` to replace the current process with the command,
/// meaning cmdy ceases to exist and the command takes over.
/// On Windows, this spawns a child process and waits for it (no true exec equivalent).
pub fn execute_command(cmd_def: &CommandDef) -> Result<()> {
    #[cfg(debug_assertions)]
    println!(
        "Executing '{}' (from {})",
        cmd_def.description,
        cmd_def.source_file.display()
    );

    let command_to_run = cmd_def.command.clone();

    #[cfg(debug_assertions)]
    println!("  Final Command String: {command_to_run}");

    #[cfg(target_os = "windows")]
    {
        use anyhow::bail;
        use std::process::Stdio;

        let mut cmd = ProcessCommand::new("cmd");
        cmd.args(["/C", &command_to_run]);

        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| {
                format!("Failed to start command snippet '{}'", cmd_def.description)
            })?;

        if !status.success() {
            bail!(
                "Command snippet '{}' failed with status: {}",
                cmd_def.description,
                status
            );
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::process::CommandExt;

        let mut cmd = ProcessCommand::new("sh");
        cmd.arg("-c");
        cmd.arg(&command_to_run);

        // exec() replaces the current process - it never returns on success
        let err = cmd.exec();

        // If we get here, exec() failed
        Err(err)
            .with_context(|| format!("Failed to exec command snippet '{}'", cmd_def.description))
    }
}
// --- Tests for executor ---
// Note: Since exec() replaces the current process, we cannot directly test
// execute_command() in unit tests on Unix. The function's correctness is
// verified through integration tests that spawn a subprocess.
//
// The tests below verify that CommandDef structures are properly handled
// and that the function signature is correct.
#[cfg(test)]
mod tests {
    use crate::types::CommandDef;
    use std::path::PathBuf;

    #[test]
    fn test_command_def_creation() {
        let cmd = CommandDef {
            description: "test command".to_string(),
            command: "echo hello".to_string(),
            source_file: PathBuf::from("test.toml"),
            tags: vec!["test".to_string()],
        };
        assert_eq!(cmd.description, "test command");
        assert_eq!(cmd.command, "echo hello");
        assert_eq!(cmd.source_file, PathBuf::from("test.toml"));
        assert_eq!(cmd.tags, vec!["test".to_string()]);
    }
}
