use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("{field}: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },

    #[error("{resource} {id} was not found")]
    NotFound { resource: &'static str, id: String },

    #[error("{message}")]
    ValidationFailed { code: &'static str, message: String },

    #[error("{resource} {id} changed: expected version {expected}, current version {current}")]
    VersionConflict {
        resource: &'static str,
        id: String,
        expected: i64,
        current: i64,
    },

    #[error("stored job data is invalid: {0}")]
    InvalidStoredData(String),

    #[error("local database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("local storage error: {0}")]
    Io(#[from] std::io::Error),
}

impl ApplicationError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "invalid_input",
            Self::NotFound { .. } => "not_found",
            Self::ValidationFailed { .. } => "validation_failed",
            Self::VersionConflict { .. } => "version_conflict",
            Self::InvalidStoredData(_) => "invalid_stored_data",
            Self::Database(_) | Self::Io(_) => "storage_unavailable",
        }
    }
}
