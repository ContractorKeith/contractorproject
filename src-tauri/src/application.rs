use std::path::Path;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::domain::Job;
use crate::domain::JobStatus;
pub use crate::error::ApplicationError;
use crate::storage::SqliteJobStore;

pub struct ApplicationService {
    jobs: SqliteJobStore,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobRequest {
    pub name: String,
    pub timezone: String,
}

impl ApplicationService {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        Ok(Self {
            jobs: SqliteJobStore::open(database_path)?,
        })
    }

    pub fn create_job(&self, request: CreateJobRequest) -> Result<Job, ApplicationError> {
        let name = required_text("name", request.name, 120)?;
        let timezone = required_text("timezone", request.timezone, 80)?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let job = Job {
            id: Uuid::now_v7().to_string(),
            name,
            status: JobStatus::Draft,
            timezone,
            created_at: now.clone(),
            updated_at: now,
            version: 1,
        };

        self.jobs.insert_job(&job)?;
        Ok(job)
    }

    pub fn list_jobs(&self) -> Result<Vec<Job>, ApplicationError> {
        self.jobs.list_jobs()
    }
}

fn required_text(
    field: &'static str,
    value: String,
    maximum_characters: usize,
) -> Result<String, ApplicationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApplicationError::InvalidInput {
            field,
            message: "is required".into(),
        });
    }
    if value.chars().count() > maximum_characters {
        return Err(ApplicationError::InvalidInput {
            field,
            message: format!("must be {maximum_characters} characters or fewer"),
        });
    }
    Ok(value.into())
}
