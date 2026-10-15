use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse {path}: {message}")]
    Parse { path: String, message: String },

    #[error("failed to write a record: {0}")]
    Render(String),

    #[error("preset `{0}` not found")]
    PresetNotFound(String),

    #[error("a preset named `{0}` already exists")]
    PresetNameTaken(String),

    #[error("log `{0}` not found")]
    LogNotFound(String),

    #[error("`{0}` already exists")]
    AlreadyExists(String),

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
