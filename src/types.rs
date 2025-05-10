use serde::Deserialize;
use std::path::PathBuf;

/// Represents a single command snippet definition within a TOML file.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CommandSnippet {
    /// A short description of what the command does.
    pub description: String,
    /// The actual shell command string to execute.
    pub command: String,
    /// Optional tags for the command snippet (e.g., categories or keywords).
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Represents the structure of a TOML file containing one or more command snippets.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct FileDef {
    /// A list of command snippets defined in this file.
    pub commands: Vec<CommandSnippet>,
}

/// Represents the fully loaded command definition, including its source.
#[derive(Debug, Clone)]
pub struct CommandDef {
    /// A short description of what the command does.
    pub description: String,
    /// The actual shell command string to execute.
    pub command: String,
    /// The path to the TOML file where this command was defined.
    pub source_file: PathBuf,
    /// Optional tags associated with this command snippet.
    pub tags: Vec<String>,
}
// --- Tests for types deserialization ---
#[cfg(test)]
mod tests {
    use super::FileDef;
    use toml;

    #[test]
    fn test_filedef_deserialize_success() {
        let toml_str = r#"
[[commands]]
description = "desc"
command = "echo hi"
tags = ["a", "b"]
"#;
        let fd: FileDef = toml::from_str(toml_str).expect("Failed to parse FileDef");
        assert_eq!(fd.commands.len(), 1);
        let cs = &fd.commands[0];
        assert_eq!(cs.description, "desc");
        assert_eq!(cs.command, "echo hi");
        assert_eq!(cs.tags, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_command_snippet_default_tags() {
        let toml_str = r#"
[[commands]]
description = "no-tags"
command = "echo"
"#;
        let fd: FileDef = toml::from_str(toml_str).expect("Failed to parse FileDef");
        assert_eq!(fd.commands.len(), 1);
        let cs = &fd.commands[0];
        assert!(cs.tags.is_empty(), "Expected default empty tags");
    }

    #[test]
    fn test_filedef_deny_unknown_fields() {
        let toml_str = r#"
[[commands]]
description = "desc"
command = "echo"
unknown_field = 123
"#;
        let result: Result<FileDef, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "Unknown field should cause error");
    }
    
    #[test]
    fn test_missing_description_field() {
        // commands array exists but description is missing
        let toml_str = r#"[[commands]]
command = "echo hi"
"#;
        let result: Result<FileDef, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "Missing description should error");
    }

    #[test]
    fn test_missing_command_field() {
        // commands array exists but command is missing
        let toml_str = r#"[[commands]]
description = "desc"
"#;
        let result: Result<FileDef, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "Missing command should error");
    }

    #[test]
    fn test_missing_commands_array() {
        // No commands table at all
        let toml_str = "";
        let result: Result<FileDef, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "Missing commands array should error");
    }
}
