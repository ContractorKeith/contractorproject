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
    ValidationFailed {
        code: &'static str,
        field: &'static str,
        message: String,
    },

    #[error("{resource} {id} changed: expected version {expected}, current version {current}")]
    VersionConflict {
        resource: &'static str,
        id: String,
        expected: i64,
        current: i64,
    },

    #[error("command {command_id} was already applied")]
    DuplicateCommand { command_id: String },

    #[error("backup destination already exists")]
    BackupDestinationExists,

    #[error("backup could not be created")]
    BackupFailed,

    #[error("backup verification failed")]
    BackupVerificationFailed,

    #[error("restore target already exists")]
    RestoreTargetExists,

    #[error("restore could not be completed")]
    RestoreFailed,

    #[error("restore verification failed")]
    RestoreVerificationFailed,

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
            Self::DuplicateCommand { .. } => "duplicate_command",
            Self::BackupDestinationExists => "backup_destination_exists",
            Self::BackupFailed => "backup_failed",
            Self::BackupVerificationFailed => "backup_verification_failed",
            Self::RestoreTargetExists => "restore_target_exists",
            Self::RestoreFailed => "restore_failed",
            Self::RestoreVerificationFailed => "restore_verification_failed",
            Self::InvalidStoredData(_) => "invalid_stored_data",
            Self::Database(_) | Self::Io(_) => "storage_unavailable",
        }
    }
}
