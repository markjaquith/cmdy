use crate::executor::execute_command;
use crate::types::CommandDef;
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Command as ProcessCommand, Stdio},
};
/// Remove ANSI escape sequences from the input string.
fn strip_ansi_escapes(s: &str) -> String {
    Regex::new(r"\x1b\[[0-9;]*m")
        .unwrap()
        .replace_all(s, "")
        .to_string()
}

#[cfg(test)]
mod ansi_tests {
    use super::strip_ansi_escapes;

    #[test]
    fn test_strip_ansi_escapes() {
        let input = "\x1b[31mHello\x1b[0m World \x1b[1;32m!";
        let expected = "Hello World !";
        assert_eq!(strip_ansi_escapes(input), expected);
    }
}

/// Present the interactive chooser and return the selected snippet.
pub fn choose_command<'a>(
    commands_vec: &'a [CommandDef],
    config_dir: &Path,
    filter_cmd: &str,
    initial_query: Option<&str>,
    exclude_tags: &[String],
) -> Result<&'a CommandDef> {
    // No snippets to choose from
    if commands_vec.is_empty() {
        bail!(
            "No command snippets defined. Looked in: {}",
            config_dir.display()
        );
    }
    // Build display lines: show description plus tags (prefixed with '#')
    let mut choice_map: HashMap<String, &CommandDef> = HashMap::new();
    let prefix = "\x1b[33m";
    let suffix = "\x1b[0m";
    let mut colored_lines = Vec::new();
    for cmd_def in commands_vec {
        // Prepare tag string: e.g., "#tag1 #tag2"
        let tags_str = if cmd_def.tags.is_empty() {
            String::new()
        } else {
            let filtered_tags: Vec<String> = cmd_def
                .tags
                .iter()
                .filter(|t| !exclude_tags.contains(t))
                .map(|t| format!("#{t}"))
                .collect();
            filtered_tags.join(" ")
        };
        // Raw (uncolored) line: description plus tags if any
        let raw_line = if tags_str.is_empty() {
            cmd_def.description.clone()
        } else {
            format!("{} {}", cmd_def.description, tags_str)
        };
        // Colored line for the filter UI
        let colored_line = if tags_str.is_empty() {
            cmd_def.description.clone()
        } else {
            format!("{} {}{}{}", cmd_def.description, prefix, tags_str, suffix)
        };
        choice_map.insert(raw_line.clone(), cmd_def);
        colored_lines.push(colored_line);
    }
    // Launch filter command with optional pre-populated query
    let mut parts = filter_cmd.split_whitespace();
    let filter_prog = parts.next().unwrap();
    // Collect base arguments
    let mut effective_args: Vec<String> = parts.map(std::string::ToString::to_string).collect();
    // Insert initial query based on underlying filter command
    if let Some(query) = initial_query {
        match filter_prog {
            "fzf" => {
                effective_args.push("--query".to_string());
                effective_args.push(query.to_string());
            }
            prog if prog == "gum" && effective_args.first().is_some_and(|s| s == "filter") => {
                effective_args.push("--filter".to_string());
                effective_args.push(query.to_string());
            }
            _ => {}
        }
    }
    // Add header for fzf when tags are filtered
    if filter_prog == "fzf" && !exclude_tags.is_empty() {
        let header = exclude_tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(" ");
        effective_args.push("--header".to_string());
        effective_args.push(header);
        effective_args.push("--header-first".to_string());
    }
    let mut filter_child = ProcessCommand::new(filter_prog)
        .args(&effective_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn filter command '{filter_cmd}'"))?;
    // Feed choices
    {
        let mut stdin = filter_child
            .stdin
            .take()
            .context("Failed to open filter stdin")?;
        for line in &colored_lines {
            writeln!(stdin, "{line}").context("Failed to write to filter stdin")?;
        }
    }
    // Read selection
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
        std::process::exit(1);
    }
    // Strip ANSI escapes
    let key = strip_ansi_escapes(selected.trim());
    // Lookup the corresponding CommandDef
    choice_map
        .get(&key)
        .copied()
        .with_context(|| format!("Selected command '{key}' not found"))
}
// --- Smoke test for full selection+execution flow ---
#[cfg(all(test, not(target_os = "windows")))]
mod smoke_tests {
    use super::*;
    use crate::types::CommandDef;
    use std::path::{Path, PathBuf};

    #[test]
    fn smoke_select_and_execute() {
        // Create two dummy commands; the filter will pick the first via head
        let cmd1 = CommandDef {
            description: "First".to_string(),
            command: "echo first".to_string(),
            source_file: PathBuf::from("x.toml"),
            tags: Vec::new(),
        };
        let cmd2 = CommandDef {
            description: "Second".to_string(),
            command: "false".to_string(),
            source_file: PathBuf::from("y.toml"),
            tags: Vec::new(),
        };
        let commands = vec![cmd1, cmd2];
        // Using head -n1 to auto-select the only entry
        let res =
            select_and_execute_command(&commands, Path::new("."), "head -n1", None, &[], false);
        assert!(res.is_ok(), "Expected Ok, got {res:?}");
    }
}

/// Uses an external filter command (e.g., fzf) to select from available snippets,
/// then executes the chosen command with provided arguments.
pub fn select_and_execute_command(
    commands_vec: &[CommandDef],
    config_dir: &Path,
    filter_cmd: &str,
    initial_query: Option<&str>,
    exclude_tags: &[String],
    overwrite_shell_command: bool,
) -> Result<()> {
    let cmd_def = choose_command(
        commands_vec,
        config_dir,
        filter_cmd,
        initial_query,
        exclude_tags,
    )?;
    execute_command(cmd_def, overwrite_shell_command).with_context(|| {
        format!(
            "Failed to execute command snippet '{}'",
            cmd_def.description
        )
    })
}
