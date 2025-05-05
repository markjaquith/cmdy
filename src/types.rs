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