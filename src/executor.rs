use anyhow::{bail, Context, Result};
use shell_escape::escape;
use std::process::{Command as ProcessCommand, Stdio};
use crate::types::CommandDef;

/// Executes the specified command snippet, appending any provided arguments safely quoted.
pub fn execute_command(cmd_def: &CommandDef, cmd_args: &[String]) -> Result<()> {
    #[cfg(debug_assertions)]
    println!(
        "Executing '{}' (from {})",
        cmd_def.description,
        cmd_def.source_file.display()
    );

    // Build the command string
    let mut command_to_run = cmd_def.command.clone();
    for arg in cmd_args {
        command_to_run.push(' ');
        if cfg!(target_os = "windows") {
            if arg.is_empty() || arg.contains(char::is_whitespace) || arg.contains('"') {
                command_to_run.push('"');
                command_to_run.push_str(&arg.replace('"', "\"\""));
                command_to_run.push('"');
            } else {
                command_to_run.push_str(arg);
            }
        } else {
            let escaped_arg = escape(arg.into());
            command_to_run.push_str(&escaped_arg);
        }
    }

    #[cfg(debug_assertions)]
    println!("  Final Command String: {}", command_to_run);

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

    // Execute, inheriting IO
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