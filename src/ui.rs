use anyhow::{Context, Result};
use regex::Regex;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Command as ProcessCommand, Stdio},
};
use crate::types::CommandDef;
use crate::executor::execute_command;

/// Uses an external filter command (e.g., fzf) to select from available snippets,
/// then executes the chosen command with provided arguments.
pub fn select_and_execute_command(
    commands_vec: &[CommandDef],
    cmd_args: &[String],
    config_dir: &Path,
    filter_cmd: &str,
) -> Result<()> {
    if commands_vec.is_empty() {
        println!("No command snippets defined.");
        println!(
            "Looked for *.toml files containing [[commands]] in: {}",
            config_dir.display()
        );
        println!("Create .toml files in this directory to define commands, for example:");
        println!("\n[[commands]]");
        println!("name = \"your-command-name\"");
        println!("description = \"Your command description\"");
        println!("command = \"your command string\"");
        return Ok(());
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

    let mut parts = filter_cmd.split_whitespace();
    let filter_prog = parts.next().unwrap();
    let filter_args: Vec<&str> = parts.collect();
    let mut filter_child = ProcessCommand::new(filter_prog);
    filter_child.args(&filter_args);
    let mut filter_child = filter_child
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn filter command '{}'", filter_cmd))?;

    {
        let mut stdin = filter_child
            .stdin
            .take()
            .context("Failed to open filter stdin")?;
        for line in &colored_lines {
            writeln!(stdin, "{}", line).context("Failed to write to filter stdin")?;
        }
    }

    let mut selected = String::new();
    {
        let mut stdout = filter_child
            .stdout
            .take()
            .context("Failed to open filter stdout")?;
        stdout
            .read_to_string(&mut selected)
            .context("Failed to read filter output")?;
    }

    let status = filter_child
        .wait()
        .context("Failed to wait for filter process")?;
    if !status.success() {
        println!("No selection made. Exiting.");
        return Ok(());
    }

    let selected = selected.trim();
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let key = re.replace_all(selected, "").to_string();
    let cmd_def = choice_map
        .get(&key)
        .with_context(|| format!("Selected command '{}' not found", key))?;

    execute_command(cmd_def, cmd_args).with_context(|| {
        format!("Failed to execute command snippet '{}'", cmd_def.description)
    })
}