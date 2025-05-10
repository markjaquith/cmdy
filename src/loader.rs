use crate::types::{CommandDef, FileDef};
use anyhow::{Context, Result, bail};
use std::{collections::HashMap, fs, path::Path};

/// Loads all command snippets from `.toml` files in the specified directory.
/// Returns a map of description -> CommandDef, checking for duplicates.
pub fn load_commands(dir: &Path) -> Result<HashMap<String, CommandDef>> {
    let mut commands = HashMap::new();

    if !dir.is_dir() {
        // No commands to load if directory doesn't exist
        return Ok(commands);
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read command file: {}", path.display()))?;
            match toml::from_str::<FileDef>(&content) {
                Ok(file_def) => {
                    for snippet in file_def.commands {
                        let key = snippet.description.clone();
                        if commands.contains_key(&key) {
                            let existing = &commands[&key];
                            bail!(
                                "Duplicate command snippet name '{}' found.\n  Defined in: {}\n  Also defined in: {}",
                                key,
                                path.display(),
                                existing.source_file.display()
                            );
                        }
                        let cmd_def = CommandDef {
                            description: key.clone(),
                            command: snippet.command,
                            source_file: path.clone(),
                            tags: snippet.tags,
                        };
                        commands.insert(key, cmd_def);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to parse TOML from file: {}. Error: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::{fs, io::Write};
    use tempfile::tempdir;

    /// Helper: create tmp dir and write TOML files for testing.
    fn setup_test_config(dir_path: &PathBuf, files: &[(&str, &str)]) -> Result<()> {
        fs::create_dir_all(dir_path)?;
        for (name, content) in files {
            let filename = if name.ends_with(".toml") {
                name.to_string()
            } else {
                format!("{}.toml", name)
            };
            let file_path = dir_path.join(filename);
            let mut file = fs::File::create(&file_path)
                .with_context(|| format!("Failed to create test file: {}", file_path.display()))?;
            writeln!(file, "{}", content)?;
        }
        Ok(())
    }

    #[test]
    fn test_load_commands_success_multiple_files_and_snippets() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir = temp_dir.path().to_path_buf();
        let file1 = (
            "commands1.toml",
            r#"[[commands]]
description = "A"
command = "echo A"
[[commands]]
description = "B"
command = "echo B"
"#,
        );
        let file2 = (
            "commands2.toml",
            r#"[[commands]]
description = "C"
command = "echo C"
"#,
        );
        setup_test_config(&dir, &[file1, file2])?;
        let commands = load_commands(&dir)?;
        assert_eq!(commands.len(), 3);
        assert!(commands.contains_key("A"));
        assert!(commands.contains_key("B"));
        assert!(commands.contains_key("C"));
        Ok(())
    }

    #[test]
    fn test_load_commands_invalid_toml_syntax_warning() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir = temp_dir.path().to_path_buf();
        let invalid = ("bad.toml", "not a valid toml");
        let valid = (
            "good.toml",
            r#"[[commands]]
description = "OK"
command = "echo ok"
"#,
        );
        setup_test_config(&dir, &[invalid, valid])?;
        let commands = load_commands(&dir)?;
        assert_eq!(commands.len(), 1);
        assert!(commands.contains_key("OK"));
        Ok(())
    }
    
    #[test]
    fn test_load_commands_duplicate_names() -> Result<()> {
        let temp_dir = tempdir()?;
        let dir = temp_dir.path().to_path_buf();
        let file1 = (
            "one.toml",
            r#"[[commands]]
description = "X"
command = "echo 1"
"#,
        );
        let file2 = (
            "two.toml",
            r#"[[commands]]
description = "X"
command = "echo 2"
"#,
        );
        setup_test_config(&dir, &[file1, file2])?;
        let err = load_commands(&dir).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Duplicate command snippet name 'X'"), "error message was: {}", msg);
        Ok(())
    }
}
