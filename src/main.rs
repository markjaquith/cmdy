mod config;
mod executor;
mod loader;
mod types;
mod ui;

use anyhow::{Context, Result, bail};
// Clipboard integration: use real clipboard in normal builds, stub in tests to avoid link errors
#[cfg(not(test))]
use arboard::Clipboard;
#[cfg(test)]
/// Stub Clipboard for tests
pub struct Clipboard;
#[cfg(test)]
impl Clipboard {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
    pub fn set_text(&mut self, _text: String) -> Result<()> {
        Ok(())
    }
}
use clap::{Parser, Subcommand};

use config::{determine_config_directory, load_app_config};
use loader::load_commands;
use std::path::{Path, PathBuf};
use types::CommandDef;
use ui::{choose_command, select_and_execute_command};
/// Collect the list of directories to scan for command snippets.
/// Always include the primary directory; only include `extra_dirs` if no --dir flag is provided.
fn get_scan_dirs(
    cli_dir: &Option<PathBuf>,
    primary: &Path,
    extra_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(primary.to_path_buf());
    if cli_dir.is_none() {
        dirs.extend_from_slice(extra_dirs);
    }
    dirs
}
// Unit tests for directory scanning behavior
#[cfg(test)]
mod scan_dirs_tests {
    use super::get_scan_dirs;
    use std::path::PathBuf;

    #[test]
    fn with_dir_flag_only_primary() {
        let primary = PathBuf::from("/only");
        let cli_dir = Some(primary.clone());
        let extras = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let dirs = get_scan_dirs(&cli_dir, &primary, &extras);
        assert_eq!(dirs, vec![primary]);
    }

    #[test]
    fn without_dir_flag_includes_extras() {
        let primary = PathBuf::from("/base");
        let cli_dir: Option<PathBuf> = None;
        let extras = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let dirs = get_scan_dirs(&cli_dir, &primary, &extras);
        let expected = vec![
            PathBuf::from("/base"),
            PathBuf::from("/a"),
            PathBuf::from("/b"),
        ];
        assert_eq!(dirs, expected);
    }
}

/// Top-level CLI options and subcommand
#[derive(Parser, Debug)]
#[command(
    name = "cmdy",
    author,
    version,
    about = "Lists and runs predefined command snippets.",
    long_about = None,
    subcommand_required = false,
)]
struct CliArgs {
    /// Optional directory to load command definitions from.
    /// Defaults to standard config locations based on OS.
    /// When specified, only this directory is scanned; config.toml's `directories` are ignored.
    #[arg(long, value_name = "DIRECTORY")]
    dir: Option<PathBuf>,

    /// Filter to only show commands tagged with this value. May be used multiple times.
    #[arg(short = 't', long = "tag", value_name = "TAG")]
    tags: Vec<String>,
    /// Pre-populate the initial filter query for the interactive selector
    #[arg(short = 'q', long = "query", value_name = "QUERY")]
    query: Option<String>,
    /// Show the command that would be executed without running it
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Subcommand to run (default: run the selected snippet)
    #[command(subcommand)]
    action: Option<Action>,
}

/// Subcommands supported by cmdy
#[derive(Subcommand, Debug)]
enum Action {
    /// Open the selected snippet in your $EDITOR
    Edit {
        /// Filter to only show commands tagged with this value. May be used multiple times.
        #[arg(short = 't', long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Copy the selected snippet's command to the clipboard
    Clip {
        /// Filter to only show commands tagged with this value. May be used multiple times.
        #[arg(short = 't', long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    // Parse CLI arguments
    let cli_args = CliArgs::parse();
    // Load global application configuration
    let app_config = load_app_config().context("Failed to load application configuration")?;

    // Determine the directory containing command definitions
    let config_dir = determine_config_directory(&cli_args.dir)?;
    #[cfg(debug_assertions)]
    println!("Using configuration directory: {}", config_dir.display());

    // Collect directories to scan: primary first, extras only if no --dir flag
    let scan_dirs = get_scan_dirs(&cli_args.dir, &config_dir, &app_config.directories);

    // Load commands from the first directory
    let mut commands_map = load_commands(&scan_dirs[0])
        .with_context(|| format!("Failed to load command definitions from {}", scan_dirs[0].display()))?;

    // Merge commands from remaining directories
    for extra_dir in scan_dirs.iter().skip(1) {
        if extra_dir.is_dir() {
            let extra_map = load_commands(extra_dir).with_context(|| {
                format!("Failed to load command definitions from {}", extra_dir.display())
            })?;
            for (name, cmd_def) in extra_map {
                if commands_map.contains_key(&name) {
                    let existing = &commands_map[&name];
                    bail!(
                        "Duplicate command snippet name '{}' found.\n  Defined in: {}\n  Also defined in: {}",
                        name,
                        cmd_def.source_file.display(),
                        existing.source_file.display()
                    );
                }
                commands_map.insert(name, cmd_def);
            }
        }
    }

    // Convert to Vec for sorting and interactive selection
    let mut commands_vec: Vec<CommandDef> = commands_map.into_values().collect();
    commands_vec.sort_by(|a, b| a.description.cmp(&b.description));

    // Apply tag filters if provided
    if !cli_args.tags.is_empty() {
        let filter_tags = &cli_args.tags;
        commands_vec.retain(|cmd| cmd.tags.iter().any(|tag| filter_tags.contains(tag)));
        if commands_vec.is_empty() {
            eprintln!("No command snippets found matching tag(s): {filter_tags:?}");
            return Ok(());
        }
    }

    // Check if we have any commands to work with
    if commands_vec.is_empty() {
        eprintln!("No command snippets found in: {}", config_dir.display());
        return Ok(());
    }

    // Dispatch based on subcommand
    match cli_args.action {
        Some(Action::Edit {
            tags: subcommand_tags,
        }) => {
            // Combine top-level tags with subcommand tags
            let mut all_tags = cli_args.tags.clone();
            all_tags.extend(subcommand_tags);
            // Open selected snippet in editor
            let cmd_def = choose_command(
                &commands_vec,
                &config_dir,
                &app_config.filter_command,
                cli_args.query.as_deref(),
                &all_tags,
            )?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            std::process::Command::new(editor)
                .arg(&cmd_def.source_file)
                .status()
                .context("Failed to launch editor")?;
            return Ok(());
        }
        Some(Action::Clip {
            tags: subcommand_tags,
        }) => {
            // Combine top-level tags with subcommand tags
            let mut all_tags = cli_args.tags.clone();
            all_tags.extend(subcommand_tags);
            // Copy selected snippet's command to clipboard
            let cmd_def = choose_command(
                &commands_vec,
                &config_dir,
                &app_config.filter_command,
                cli_args.query.as_deref(),
                &all_tags,
            )?;
            let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
            clipboard
                .set_text(cmd_def.command.clone())
                .context("Failed to copy to clipboard")?;
            println!("Copied command to clipboard");
            return Ok(());
        }
        None => {}
    }
    // Default: run selected snippet
    if cli_args.dry_run {
        // Dry-run mode: show command without executing
        let cmd_def = choose_command(
            &commands_vec,
            &config_dir,
            &app_config.filter_command,
            cli_args.query.as_deref(),
            &cli_args.tags,
        )?;
        println!("Would execute: {}", cmd_def.command);
        println!("From file: {}", cmd_def.source_file.display());
    } else {
        select_and_execute_command(
            &commands_vec,
            &config_dir,
            &app_config.filter_command,
            cli_args.query.as_deref(),
            &cli_args.tags,
        )
        .context("Failed during command selection or execution")?;
    }

    Ok(())
}
