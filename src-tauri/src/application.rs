use std::path::Path;

use chrono::{NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::domain::{FinishStartDependency, Job, JobStatus, Task};
pub use crate::error::ApplicationError;
use crate::gantt::{
    build_gantt_read_model, GanttPredecessorSource, GanttReadModel, GanttReadModelSource,
    GanttTaskSource,
};
use crate::scheduling::{
    calculate_schedule_with_constraints, CalendarWeekday, ScheduleInput, ScheduleTask,
    TaskConstraint, WorkingCalendar,
};
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
pub struct ArchiveJobRequest {
    pub job_id: String,
    pub expected_job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreJobRequest {
    pub job_id: String,
    pub expected_job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupRequest {
    pub destination: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub destination: String,
    pub created_at_utc: String,
    pub byte_size: u64,
    pub verified: bool,
}

/// A developer-facing verification request. The target is a new app-data
/// directory; this API never replaces the running application's database.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRestoreRequest {
    pub backup_path: String,
    pub target_app_data_dir: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreVerificationResult {
    pub verified: bool,
    pub job_count: i64,
    pub task_count: i64,
    pub dependency_count: i64,
    pub command_log_count: i64,
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
    pub dependencies: Vec<FinishStartDependency>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScheduleRequest {
    pub job_id: String,
    pub schedule_start: Option<String>,
    pub calendar: WorkingCalendar,
    pub expected_job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskDurationRequest {
    pub task_id: String,
    pub duration_minutes: Option<i64>,
    pub expected_version: i64,
    pub expected_job_version: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskConstraintKind {
    StartNoEarlierThan,
    FinishNoLaterThan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskConstraintRequest {
    pub task_id: String,
    pub kind: TaskConstraintKind,
    pub value: Option<String>,
    pub expected_version: i64,
    pub expected_job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDependencyRequest {
    pub job_id: String,
    pub predecessor_task_id: String,
    pub successor_task_id: String,
    pub lag_minutes: i64,
    pub expected_job_version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDependencyRequest {
    pub job_id: String,
    pub predecessor_task_id: String,
    pub successor_task_id: String,
    pub expected_job_version: i64,
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
            schedule_start: None,
            calendar: default_calendar(),
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

    pub fn list_jobs_by_status(&self, status: JobStatus) -> Result<Vec<Job>, ApplicationError> {
        self.store.list_jobs_by_status(status)
    }

    /// Creates and verifies a consistent online snapshot of the local database.
    /// This is intentionally a storage operation, not an audited domain command.
    pub fn create_verified_backup(
        &self,
        request: CreateBackupRequest,
    ) -> Result<BackupResult, ApplicationError> {
        let destination = required_text("destination", request.destination, 1_024)?;
        self.store.create_verified_backup(&destination)
    }

    /// Restores a verified snapshot into a fresh app-data directory for
    /// developer verification. It does not alter the source backup or this
    /// service's live database and is not an audited domain command.
    pub fn verify_restore_into_fresh_app_data(
        &self,
        request: VerifyRestoreRequest,
    ) -> Result<RestoreVerificationResult, ApplicationError> {
        let backup_path = required_text("backupPath", request.backup_path, 1_024)?;
        let target_app_data_dir =
            required_text("targetAppDataDir", request.target_app_data_dir, 1_024)?;
        let published = self
            .store
            .restore_verified_backup_into_fresh_app_data(&backup_path, &target_app_data_dir)?;

        // Opening after the pre-publish verification proves the restored
        // directory can be used through the normal application seam.
        let verification = (|| {
            let restored = Self::open(&published.target_database_path)
                .map_err(|_| ApplicationError::RestoreVerificationFailed)?;
            let result = restored
                .store
                .verify_restored_database()
                .map_err(|_| ApplicationError::RestoreVerificationFailed)?;
            drop(restored);
            Ok(result)
        })();
        match verification {
            Ok(result) => {
                SqliteStore::finalize_published_restore(&published)?;
                Ok(result)
            }
            Err(error) => {
                SqliteStore::rollback_published_restore(&published)?;
                Err(error)
            }
        }
    }

    pub fn archive_job(
        &self,
        context: CommandContext,
        request: ArchiveJobRequest,
    ) -> Result<Job, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        self.store.archive_job(
            &request.job_id,
            request.expected_job_version,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )
    }

    pub fn restore_job(
        &self,
        context: CommandContext,
        request: RestoreJobRequest,
    ) -> Result<Job, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        self.store.restore_job(
            &request.job_id,
            request.expected_job_version,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )
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
            duration_minutes: None,
            start_no_earlier_than: None,
            finish_no_later_than: None,
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
        let (job_version, tasks, dependencies) = self.store.list_tasks(job_id)?;
        Ok(TaskHierarchy {
            job_id: job_id.into(),
            job_version,
            tasks,
            dependencies,
        })
    }

    /// Builds the read-only schedule projection from canonical SQLite inputs.
    pub fn get_schedule(&self, job_id: &str) -> Result<GanttReadModel, ApplicationError> {
        let (job, tasks, dependencies) = self.store.schedule_inputs(job_id)?;
        let schedule_start =
            job.schedule_start
                .as_deref()
                .ok_or(ApplicationError::ValidationFailed {
                    code: "schedule_start_required",
                    field: "scheduleStart",
                    message: "set a schedule start before viewing the schedule".into(),
                })?;
        let schedule_start =
            NaiveDate::parse_from_str(schedule_start, "%Y-%m-%d").map_err(|_| {
                ApplicationError::InvalidStoredData(format!(
                    "job {} has an invalid schedule start",
                    job.id
                ))
            })?;
        let constraints = tasks
            .iter()
            .filter(|task| {
                task.start_no_earlier_than.is_some() || task.finish_no_later_than.is_some()
            })
            .map(|task| {
                Ok(TaskConstraint {
                    task_id: task.id.clone(),
                    start_no_earlier_than: task
                        .start_no_earlier_than
                        .as_deref()
                        .map(parse_constraint_date)
                        .transpose()?,
                    finish_no_later_than: task
                        .finish_no_later_than
                        .as_deref()
                        .map(parse_constraint_date)
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, ApplicationError>>()?;
        let schedule = calculate_schedule_with_constraints(
            &ScheduleInput {
                schedule_start,
                calendar: job.calendar.clone(),
                tasks: tasks
                    .iter()
                    .map(|task| ScheduleTask {
                        id: task.id.clone(),
                        parent_task_id: task.parent_task_id.clone(),
                        duration_minutes: task.duration_minutes,
                    })
                    .collect(),
                dependencies: dependencies
                    .iter()
                    .map(|dependency| crate::scheduling::FinishStartDependency {
                        predecessor_task_id: dependency.predecessor_task_id.clone(),
                        successor_task_id: dependency.successor_task_id.clone(),
                        lag_minutes: dependency.lag_minutes,
                    })
                    .collect(),
            },
            &constraints,
        )
        .map_err(|error| ApplicationError::ValidationFailed {
            code: error.code(),
            field: "schedule",
            message: error.to_string(),
        })?;
        build_gantt_read_model(GanttReadModelSource {
            job_id: job.id,
            job_version: job.version,
            tasks: tasks
                .iter()
                .map(|task| GanttTaskSource {
                    id: task.id.clone(),
                    parent_task_id: task.parent_task_id.clone(),
                    sort_key: task.sort_key,
                    name: task.name.clone(),
                    start_no_earlier_than: task
                        .start_no_earlier_than
                        .as_deref()
                        .map(parse_constraint_date)
                        .transpose()
                        .expect("constraints were validated before building the read model"),
                    finish_no_later_than: task
                        .finish_no_later_than
                        .as_deref()
                        .map(parse_constraint_date)
                        .transpose()
                        .expect("constraints were validated before building the read model"),
                })
                .collect(),
            schedule,
            baseline: None,
            predecessors: tasks
                .iter()
                .map(|task| GanttPredecessorSource {
                    task_id: task.id.clone(),
                    predecessor_ids: dependencies
                        .iter()
                        .filter(|dependency| dependency.successor_task_id == task.id)
                        .map(|dependency| dependency.predecessor_task_id.clone())
                        .collect(),
                })
                .collect(),
        })
        .map_err(|error| {
            ApplicationError::InvalidStoredData(format!("schedule read model {}", error.code()))
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
        let (job_id, job_version, tasks, dependencies) = self.store.reorder_task(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )?;
        Ok(TaskHierarchy {
            job_id,
            job_version,
            tasks,
            dependencies,
        })
    }

    pub fn update_schedule(
        &self,
        context: CommandContext,
        request: UpdateScheduleRequest,
    ) -> Result<Job, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        validate_calendar(&request.calendar)?;
        if let Some(start) = &request.schedule_start {
            validate_schedule_start(start)?;
        }
        self.store.update_schedule(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )
    }

    pub fn update_task_duration(
        &self,
        context: CommandContext,
        request: UpdateTaskDurationRequest,
    ) -> Result<TaskMutation, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedVersion", request.expected_version)?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        if matches!(request.duration_minutes, Some(value) if value < 0) {
            return Err(ApplicationError::InvalidInput {
                field: "durationMinutes",
                message: "must be zero or greater".into(),
            });
        }
        let (task, job_version) = self.store.update_task_duration(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )?;
        Ok(TaskMutation { task, job_version })
    }

    pub fn update_task_constraint(
        &self,
        context: CommandContext,
        request: UpdateTaskConstraintRequest,
    ) -> Result<TaskMutation, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedVersion", request.expected_version)?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        if let Some(value) = request.value.as_deref() {
            parse_constraint_date(value)?;
        }
        let (task, job_version) = self.store.update_task_constraint(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )?;
        Ok(TaskMutation { task, job_version })
    }

    pub fn add_dependency(
        &self,
        context: CommandContext,
        request: AddDependencyRequest,
    ) -> Result<TaskHierarchy, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        if request.lag_minutes < 0 {
            return Err(ApplicationError::InvalidInput {
                field: "lagMinutes",
                message: "must be zero or greater".into(),
            });
        }
        let (job_version, tasks, dependencies) = self.store.add_dependency(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )?;
        Ok(TaskHierarchy {
            job_id: request.job_id,
            job_version,
            tasks,
            dependencies,
        })
    }

    pub fn remove_dependency(
        &self,
        context: CommandContext,
        request: RemoveDependencyRequest,
    ) -> Result<TaskHierarchy, ApplicationError> {
        let context = context.validate()?;
        required_version("expectedJobVersion", request.expected_job_version)?;
        let (job_version, tasks, dependencies) = self.store.remove_dependency(
            &request,
            &Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            &context,
        )?;
        Ok(TaskHierarchy {
            job_id: request.job_id,
            job_version,
            tasks,
            dependencies,
        })
    }
}

pub fn default_calendar() -> WorkingCalendar {
    WorkingCalendar {
        working_weekdays: vec![
            CalendarWeekday::Monday,
            CalendarWeekday::Tuesday,
            CalendarWeekday::Wednesday,
            CalendarWeekday::Thursday,
            CalendarWeekday::Friday,
        ],
        workday_start_minute: 8 * 60,
        workday_duration_minutes: 480,
    }
}

fn validate_calendar(calendar: &WorkingCalendar) -> Result<(), ApplicationError> {
    if calendar.working_weekdays.is_empty()
        || calendar.workday_start_minute >= 24 * 60
        || calendar.workday_duration_minutes == 0
        || u32::from(calendar.workday_start_minute) + u32::from(calendar.workday_duration_minutes)
            > 24 * 60
    {
        return Err(ApplicationError::ValidationFailed {
            code: "invalid_calendar",
            field: "calendar",
            message: "select at least one weekday and a working interval within one day".into(),
        });
    }
    Ok(())
}

fn validate_schedule_start(value: &str) -> Result<(), ApplicationError> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        ApplicationError::InvalidInput {
            field: "scheduleStart",
            message: "must be an ISO date".into(),
        }
    })?;
    Ok(())
}

fn parse_constraint_date(value: &str) -> Result<NaiveDate, ApplicationError> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        ApplicationError::InvalidInput {
            field: "value",
            message: "must be an ISO date-only value".into(),
        }
    })?;
    if date.format("%Y-%m-%d").to_string() != value {
        return Err(ApplicationError::InvalidInput {
            field: "value",
            message: "must be an ISO date-only value".into(),
        });
    }
    Ok(date)
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
