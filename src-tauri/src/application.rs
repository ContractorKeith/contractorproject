use std::path::Path;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::JobStatus;
pub use crate::domain::{Job, Task};
pub use crate::error::ApplicationError;
use crate::storage::SqliteStore;

pub struct ApplicationService {
    store: SqliteStore,
}

pub const MAX_COMMAND_ID_CHARACTERS: usize = 128;
pub const MAX_CLIENT_NAME_CHARACTERS: usize = 120;
pub const MAX_AUDIT_SUMMARY_CHARACTERS: usize = 240;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandActor {
    User,
    Agent,
    Import,
}

impl CommandActor {
    pub(crate) fn as_database_value(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Import => "import",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContext {
    pub command_id: String,
    pub actor: CommandActor,
    pub client_name: String,
}

impl CommandContext {
    pub fn validate(self) -> Result<Self, ApplicationError> {
        Ok(Self {
            command_id: required_text("commandId", self.command_id, MAX_COMMAND_ID_CHARACTERS)?,
            actor: self.actor,
            client_name: required_text("clientName", self.client_name, MAX_CLIENT_NAME_CHARACTERS)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobRequest {
    pub name: String,
    pub timezone: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub job_id: String,
    pub parent_task_id: Option<String>,
    pub name: String,
    pub expected_job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMutation {
    pub task: Task,
    pub job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHierarchy {
    pub job_id: String,
    pub job_version: i64,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    pub task_id: String,
    pub name: String,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderTaskRequest {
    pub task_id: String,
    pub new_parent_task_id: Option<String>,
    pub new_sibling_index: i64,
    pub expected_version: i64,
    pub expected_job_version: i64,
}

impl ApplicationService {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        Ok(Self {
            store: SqliteStore::open(database_path)?,
        })
    }

    pub fn create_job(
        &self,
        context: CommandContext,
        request: CreateJobRequest,
    ) -> Result<Job, ApplicationError> {
        let context = context.validate()?;
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

        self.store.insert_job(&job, &context)?;
        Ok(job)
    }

    pub fn list_jobs(&self) -> Result<Vec<Job>, ApplicationError> {
        self.store.list_jobs()
    }

    pub fn create_task(
        &self,
        context: CommandContext,
        request: CreateTaskRequest,
    ) -> Result<TaskMutation, ApplicationError> {
        let context = context.validate()?;
        let name = required_text("name", request.name, 200)?;
        required_version("expectedJobVersion", request.expected_job_version)?;

        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut task = Task {
            id: Uuid::now_v7().to_string(),
            sort_key: 0,
            job_id: request.job_id,
            parent_task_id: request.parent_task_id,
            name,
            created_at: now.clone(),
            updated_at: now,
            version: 1,
        };
        let job_version =
            self.store
                .create_task(&mut task, request.expected_job_version, &context)?;
        Ok(TaskMutation { task, job_version })
    }

    pub fn list_tasks(&self, job_id: &str) -> Result<TaskHierarchy, ApplicationError> {
        let (job_version, tasks) = self.store.list_tasks(job_id)?;
        Ok(TaskHierarchy {
            job_id: job_id.into(),
            job_version,
            tasks,
        })
    }

    pub fn update_task(
        &self,
        context: CommandContext,
        request: UpdateTaskRequest,
    ) -> Result<TaskMutation, ApplicationError> {
        let context = context.validate()?;
        let name = required_text("name", request.name, 200)?;
        required_version("expectedVersion", request.expected_version)?;
        let updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let (task, job_version) = self.store.update_task(
            &request.task_id,
            &name,
            request.expected_version,
            &updated_at,
            &context,
        )?;
        Ok(TaskMutation { task, job_version })
    }

    pub fn reorder_task(
        &self,
        context: CommandContext,
        request: ReorderTaskRequest,
    ) -> Result<TaskHierarchy, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedVersion", request.expected_version)?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        if request.new_sibling_index < 0 {
            return Err(ApplicationError::InvalidInput {
                field: "newSiblingIndex",
                message: "must be zero or greater".into(),
            });
        }
        let (job_id, job_version, tasks) = self.store.reorder_task(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )?;
        Ok(TaskHierarchy {
            job_id,
            job_version,
            tasks,
        })
    }
}

fn required_version(field: &'static str, version: i64) -> Result<(), ApplicationError> {
    if version < 1 {
        return Err(ApplicationError::InvalidInput {
            field,
            message: "must be a positive record version".into(),
        });
    }
    Ok(())
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
