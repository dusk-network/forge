use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CliError>;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{0}")]
    Message(String),

    #[error("invalid contract name '{name}': {reason}")]
    InvalidContractName { name: String, reason: String },

    #[error("path already exists: {0}")]
    PathAlreadyExists(PathBuf),

    #[error("expected a Dusk Forge contract project at {0}")]
    NotAForgeProject(PathBuf),

    #[error("required tool not found: {tool}. {hint}")]
    MissingTool {
        tool: &'static str,
        hint: &'static str,
    },

    #[error("command failed: {program} (exit code {code})")]
    CommandFailed { program: String, code: i32 },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("cargo metadata error: {0}")]
    CargoMetadata(#[from] cargo_metadata::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}
