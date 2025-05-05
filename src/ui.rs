use anyhow::{bail, Context, Result};
use regex::Regex;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Command as ProcessCommand, Stdio},
};
use crate::types::CommandDef;
use crate::executor::execute_command;

/// Present the interactive chooser and return the selected snippet.
pub fn choose_command<'a>(
    commands_vec: &'a [CommandDef],
    config_dir: &Path,
    filter_cmd: &str,
) -> Result<&'a CommandDef> {
    // No snippets to choose from
    if commands_vec.is_empty() {
        bail!("No command snippets defined. Looked in: {}", config_dir.display());
    }
    let mut choice_map: HashMap<String, &CommandDef> = HashMap::new();
    let prefix = "\x1b[33m";
    let suffix = "\x1b[0m";
    let mut colored_lines = Vec::new();
    for cmd_def in commands_vec.iter() {
        let filename = cmd_def
            .source_file
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_else(|| "<unknown>".into());
        let raw_line = format!("{} [{}]", cmd_def.description, filename);
        let colored_line = format!("{} {}[{}]{}", cmd_def.description, prefix, filename, suffix);
        choice_map.insert(raw_line.clone(), cmd_def);
        colored_lines.push(colored_line);
    }
    // Launch filter command
    let mut parts = filter_cmd.split_whitespace();
    let filter_prog = parts.next().unwrap();
    let filter_args: Vec<&str> = parts.collect();
    let mut filter_child = ProcessCommand::new(filter_prog)
        .args(&filter_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn filter command '{}'", filter_cmd))?;
    // Feed choices
    {
        let mut stdin = filter_child.stdin.take().context("Failed to open filter stdin")?;
        for line in &colored_lines {
            writeln!(stdin, "{}", line).context("Failed to write to filter stdin")?;
        }
    }
    // Read selection
    let mut selected = String::new();
    {
        let mut stdout = filter_child.stdout.take().context("Failed to open filter stdout")?;
        stdout.read_to_string(&mut selected).context("Failed to read filter output")?;
    }
    let status = filter_child.wait().context("Failed to wait for filter process")?;
    if !status.success() {
        bail!("No selection made. Exiting.");
    }
    // Strip ANSI escapes
    let key = Regex::new(r"\x1b\[[0-9;]*m").unwrap().replace_all(selected.trim(), "").to_string();
    // Lookup the corresponding CommandDef
    choice_map.get(&key).copied().with_context(|| format!("Selected command '{}' not found", key))
}

/// Uses an external filter command (e.g., fzf) to select from available snippets,
/// then executes the chosen command with provided arguments.
pub fn select_and_execute_command(
    commands_vec: &[CommandDef],
    config_dir: &Path,
    filter_cmd: &str,
) -> Result<()> {
    let cmd_def = choose_command(commands_vec, config_dir, filter_cmd)?;
    execute_command(cmd_def).with_context(|| format!("Failed to execute command snippet '{}'", cmd_def.description))
}