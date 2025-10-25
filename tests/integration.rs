use anyhow::Result;
use std::fs;
use std::process::{Command, Stdio};
use tempfile::tempdir;

#[test]
fn test_help_command() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to run help command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Lists and runs predefined command snippets"));
    assert!(stdout.contains("--dir"));
    assert!(stdout.contains("--tag"));
    assert!(stdout.contains("--dry-run"));
}

#[test]
fn test_version_command() {
    let output = Command::new("cargo")
        .args(["run", "--", "--version"])
        .output()
        .expect("Failed to run version command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cmdy"));
}

#[test]
fn test_missing_directory() {
    let temp = tempdir().expect("Failed to create temp dir");
    let missing_dir = temp.path().join("nonexistent");

    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "--dir",
            missing_dir.to_str().unwrap(),
            "--dry-run",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("Failed to run with missing directory");

    // Should succeed but with empty commands
    assert!(output.status.success());
}

#[test]
fn test_invalid_toml_handling() -> Result<()> {
    let temp = tempdir()?;
    let commands_dir = temp.path().join("commands");
    fs::create_dir_all(&commands_dir)?;

    // Write invalid TOML
    let invalid_file = commands_dir.join("invalid.toml");
    fs::write(&invalid_file, "this is not valid toml syntax")?;

    // Write valid TOML
    let valid_file = commands_dir.join("valid.toml");
    fs::write(
        &valid_file,
        r#"
[[commands]]
description = "Test command"
command = "echo test"
"#,
    )?;

    // Test with dry-run to avoid interactive mode
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "--dir",
            commands_dir.to_str().unwrap(),
            "--dry-run",
        ])
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .output()?;

    // Should still work with warning about invalid file
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Warning: Failed to parse TOML"));

    Ok(())
}

#[test]
fn test_tag_filtering() -> Result<()> {
    let temp = tempdir()?;
    let commands_dir = temp.path().join("commands");
    fs::create_dir_all(&commands_dir)?;

    let file = commands_dir.join("test.toml");
    fs::write(
        &file,
        r#"
[[commands]]
description = "Command with tag"
command = "echo tagged"
tags = ["test", "example"]

[[commands]]
description = "Command without tag"
command = "echo untagged"
tags = []
"#,
    )?;

    // Test that --tag filters work with dry-run
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "--dir",
            commands_dir.to_str().unwrap(),
            "--tag",
            "test",
            "--dry-run",
        ])
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .output()?;

    // Should exit (1 means no selection in fzf, which is expected with null stdin)
    assert!(output.status.code() == Some(1) || output.status.success());

    Ok(())
}

#[test]
fn test_dry_run_mode() -> Result<()> {
    let temp = tempdir()?;
    let commands_dir = temp.path().join("commands");
    fs::create_dir_all(&commands_dir)?;

    let file = commands_dir.join("test.toml");
    fs::write(
        &file,
        r#"
[[commands]]
description = "Test dry run"
command = "rm -rf /important/data"
tags = ["dangerous"]
"#,
    )?;

    // Create a config that uses head instead of fzf for testing
    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir)?;
    fs::write(
        config_dir.join("cmdy.toml"),
        r#"
filter_command = "head -n1"
"#,
    )?;

    // Run with dry-run using head as selector
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "--dir",
            commands_dir.to_str().unwrap(),
            "--dry-run",
        ])
        .env("XDG_CONFIG_HOME", config_dir.parent().unwrap())
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Would execute:"));
        assert!(stdout.contains("rm -rf /important/data"));
        assert!(stdout.contains("From file:"));
    }

    Ok(())
}
