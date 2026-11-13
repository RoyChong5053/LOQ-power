use std::path::PathBuf;

/// Errors returned by the core library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0} was not found on this system")]
    NotFound(String),

    #[error("{0} is not writable")]
    NotWritable(String),

    #[error("{what} must be between {min} and {max}, got {value}")]
    OutOfRange {
        what: String,
        value: i64,
        min: i64,
        max: i64,
    },

    #[error("{what} must move in steps of {step}, got {value}")]
    InvalidStep { what: String, value: i64, step: i64 },

    #[error("{0}")]
    InvalidValue(String),

    #[error("EC writes require CUSTOM thermal mode: {0}")]
    RequiresCustomMode(String),

    #[error("verification failed for {path}: expected {expected}, read back {actual}")]
    Verification {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
