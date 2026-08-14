use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("{field}: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
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
            Self::InvalidStoredData(_) => "invalid_stored_data",
            Self::Database(_) | Self::Io(_) => "storage_unavailable",
        }
    }
}
