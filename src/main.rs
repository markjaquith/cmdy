mod config;
mod executor;
mod loader;
mod types;
mod ui;

use anyhow::{Context, Result, bail};
use arboard::Clipboard;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use config::{determine_config_directory, load_app_config};
use loader::load_commands;
use types::CommandDef;
use ui::{choose_command, select_and_execute_command};

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
    #[arg(long, value_name = "DIRECTORY")]
    dir: Option<PathBuf>,

    /// Filter to only show commands tagged with this value. May be used multiple times.
    #[arg(short = 't', long = "tag", value_name = "TAG")]
    tags: Vec<String>,
    /// Pre-populate the initial filter query for the interactive selector
    #[arg(short = 'q', long = "query", value_name = "QUERY")]
    query: Option<String>,
    /// Subcommand to run (default: run the selected snippet)
    #[command(subcommand)]
    action: Option<Action>,
}

/// Subcommands supported by cmdy
#[derive(Subcommand, Debug)]
enum Action {
    /// Open the selected snippet in your $EDITOR
    Edit,
    /// Copy the selected snippet's command to the clipboard
    Clip,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let cli_args = CliArgs::parse();
    // Load global application configuration
    let app_config = load_app_config().context("Failed to load application configuration")?;

    // Determine the directory containing command definitions
    let config_dir = determine_config_directory(&cli_args.dir)?;
    #[cfg(debug_assertions)]
    println!("Using configuration directory: {:?}", config_dir);

    // Load commands from the primary directory
    let mut commands_map = load_commands(&config_dir)
        .with_context(|| format!("Failed to load command definitions from {:?}", config_dir))?;
    // Load additional directories from config
    for extra_dir in &app_config.directories {
        if extra_dir.is_dir() {
            let extra_map = load_commands(extra_dir).with_context(|| {
                format!("Failed to load command definitions from {:?}", extra_dir)
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
            eprintln!(
                "No command snippets found matching tag(s): {:?}",
                filter_tags
            );
            return Ok(());
        }
    }

    // Dispatch based on subcommand
    match cli_args.action {
        Some(Action::Edit) => {
            // Open selected snippet in editor
            let cmd_def = choose_command(
                &commands_vec,
                &config_dir,
                &app_config.filter_command,
                cli_args.query.as_deref(),
            )?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            std::process::Command::new(editor)
                .arg(&cmd_def.source_file)
                .status()
                .context("Failed to launch editor")?;
            return Ok(());
        }
        Some(Action::Clip) => {
            // Copy selected snippet's command to clipboard
            let cmd_def = choose_command(
                &commands_vec,
                &config_dir,
                &app_config.filter_command,
                cli_args.query.as_deref(),
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
    select_and_execute_command(
        &commands_vec,
        &config_dir,
        &app_config.filter_command,
        cli_args.query.as_deref(),
    )
    .context("Failed during command selection or execution")?;

    Ok(())
}
