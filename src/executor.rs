use crate::types::CommandDef;
use anyhow::{bail, Context, Result};
use std::process::{Command as ProcessCommand, Stdio};

/// Executes the specified command snippet, appending any provided arguments safely quoted.
pub fn execute_command(cmd_def: &CommandDef) -> Result<()> {
    #[cfg(debug_assertions)]
    println!(
        "Executing '{}' (from {})",
        cmd_def.description,
        cmd_def.source_file.display()
    );

    // Use the base command defined in the snippet
    let command_to_run = cmd_def.command.clone();

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
