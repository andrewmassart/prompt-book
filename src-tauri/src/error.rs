use thiserror::Error;

/// Application-level errors for session parsing and I/O operations.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unknown session format: {0}")]
    UnknownFormat(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl From<AppError> for String {
    fn from(err: AppError) -> String {
        err.to_string()
    }
}
