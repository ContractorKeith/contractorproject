pub mod application;
mod domain;
mod error;
pub mod gantt;
pub mod scheduling;
mod storage;
mod work_breakdown;

use application::{
    AddDependencyRequest, ApplicationError, ApplicationService, ArchiveJobRequest, BackupResult,
    CommandActor, CommandContext, CreateBackupRequest, CreateJobRequest, CreateTaskRequest, Job,
    JobStatus, RemoveDependencyRequest, ReorderTaskRequest, RestoreJobRequest, TaskHierarchy,
    TaskMutation, UpdateScheduleRequest, UpdateTaskConstraintRequest, UpdateTaskDurationRequest,
    UpdateTaskRequest,
};
use gantt::GanttReadModel;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    kind: &'static str,
    message: String,
    #[serde(flatten)]
    details: Box<CommandErrorDetails>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", untagged)]
enum CommandErrorDetails {
    InvalidInput {
        field: &'static str,
    },
    Validation {
        code: &'static str,
        field: &'static str,
    },
    Record {
        resource: &'static str,
        record_id: String,
    },
    VersionConflict {
        resource: &'static str,
        record_id: String,
        expected_version: i64,
        current_version: i64,
    },
    DuplicateCommand {
        command_id: String,
    },
    None {},
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        let details = match &error {
            ApplicationError::InvalidInput { field, .. } => {
                CommandErrorDetails::InvalidInput { field }
            }
            ApplicationError::ValidationFailed { code, field, .. } => {
                CommandErrorDetails::Validation { code, field }
            }
            ApplicationError::NotFound { resource, id } => CommandErrorDetails::Record {
                resource,
                record_id: id.clone(),
            },
            ApplicationError::VersionConflict {
                resource,
                id,
                expected,
                current,
            } => CommandErrorDetails::VersionConflict {
                resource,
                record_id: id.clone(),
                expected_version: *expected,
                current_version: *current,
            },
            ApplicationError::DuplicateCommand { command_id } => {
                CommandErrorDetails::DuplicateCommand {
                    command_id: command_id.clone(),
                }
            }
            ApplicationError::BackupDestinationExists
            | ApplicationError::BackupFailed
            | ApplicationError::BackupVerificationFailed
            | ApplicationError::RestoreTargetExists
            | ApplicationError::RestoreFailed
            | ApplicationError::RestoreVerificationFailed
            | ApplicationError::InvalidStoredData(_)
            | ApplicationError::Database(_)
            | ApplicationError::Io(_) => CommandErrorDetails::None {},
        };
        let message = match &error {
            ApplicationError::BackupDestinationExists => {
                "Choose a destination that does not already exist.".to_owned()
            }
            ApplicationError::BackupFailed
            | ApplicationError::BackupVerificationFailed
            | ApplicationError::RestoreFailed
            | ApplicationError::RestoreVerificationFailed => {
                "The backup could not be verified. Your local data was not changed.".to_owned()
            }
            ApplicationError::InvalidStoredData(_)
            | ApplicationError::Database(_)
            | ApplicationError::Io(_) => {
                "Local storage is unavailable. Please try again.".to_owned()
            }
            _ => error.to_string(),
        };
        Self {
            kind: error.kind(),
            message,
            details: Box::new(details),
        }
    }
}

#[tauri::command]
fn create_job(
    service: State<'_, ApplicationService>,
    request: CreateJobRequest,
) -> Result<Job, CommandError> {
    service
        .create_job(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn list_jobs(
    service: State<'_, ApplicationService>,
    status: Option<JobStatus>,
) -> Result<Vec<Job>, CommandError> {
    match status {
        Some(status) => service.list_jobs_by_status(status),
        None => service.list_jobs(),
    }
    .map_err(Into::into)
}

#[tauri::command]
fn archive_job(
    service: State<'_, ApplicationService>,
    request: ArchiveJobRequest,
) -> Result<Job, CommandError> {
    service
        .archive_job(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn restore_job(
    service: State<'_, ApplicationService>,
    request: RestoreJobRequest,
) -> Result<Job, CommandError> {
    service
        .restore_job(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
async fn create_verified_backup(
    app: AppHandle,
    service: State<'_, ApplicationService>,
) -> Result<Option<BackupResult>, CommandError> {
    let suggested_name = format!(
        "ContractorProject-backup-{}.sqlite3",
        chrono::Utc::now().format("%Y-%m-%d")
    );
    let Some(destination) = app
        .dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter("SQLite backup", &["sqlite3"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let destination = destination
        .into_path()
        .map_err(|_| ApplicationError::InvalidInput {
            field: "destination",
            message: "Choose a local filesystem destination.".into(),
        })
        .map_err(CommandError::from)?;

    service
        .create_verified_backup(CreateBackupRequest {
            destination: destination.to_string_lossy().into_owned(),
        })
        .map(Some)
        .map_err(Into::into)
}

#[tauri::command]
fn create_task(
    service: State<'_, ApplicationService>,
    request: CreateTaskRequest,
) -> Result<TaskMutation, CommandError> {
    service
        .create_task(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn list_tasks(
    service: State<'_, ApplicationService>,
    job_id: String,
) -> Result<TaskHierarchy, CommandError> {
    service.list_tasks(&job_id).map_err(Into::into)
}

#[tauri::command]
fn get_schedule(
    service: State<'_, ApplicationService>,
    job_id: String,
) -> Result<GanttReadModel, CommandError> {
    service.get_schedule(&job_id).map_err(Into::into)
}

#[tauri::command]
fn update_task(
    service: State<'_, ApplicationService>,
    request: UpdateTaskRequest,
) -> Result<TaskMutation, CommandError> {
    service
        .update_task(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn reorder_task(
    service: State<'_, ApplicationService>,
    request: ReorderTaskRequest,
) -> Result<TaskHierarchy, CommandError> {
    service
        .reorder_task(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn update_schedule(
    service: State<'_, ApplicationService>,
    request: UpdateScheduleRequest,
) -> Result<Job, CommandError> {
    service
        .update_schedule(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn update_task_duration(
    service: State<'_, ApplicationService>,
    request: UpdateTaskDurationRequest,
) -> Result<TaskMutation, CommandError> {
    service
        .update_task_duration(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn update_task_constraint(
    service: State<'_, ApplicationService>,
    request: UpdateTaskConstraintRequest,
) -> Result<TaskMutation, CommandError> {
    service
        .update_task_constraint(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn add_dependency(
    service: State<'_, ApplicationService>,
    request: AddDependencyRequest,
) -> Result<TaskHierarchy, CommandError> {
    service
        .add_dependency(tauri_command_context(), request)
        .map_err(Into::into)
}

#[tauri::command]
fn remove_dependency(
    service: State<'_, ApplicationService>,
    request: RemoveDependencyRequest,
) -> Result<TaskHierarchy, CommandError> {
    service
        .remove_dependency(tauri_command_context(), request)
        .map_err(Into::into)
}

fn tauri_command_context() -> CommandContext {
    CommandContext {
        command_id: uuid::Uuid::now_v7().to_string(),
        actor: CommandActor::User,
        client_name: "desktop-ui".into(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let service = ApplicationService::open(app_data.join("contractorproject.sqlite3"))?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_job,
            list_jobs,
            archive_job,
            restore_job,
            create_verified_backup,
            create_task,
            list_tasks,
            get_schedule,
            update_task,
            reorder_task,
            update_schedule,
            update_task_duration,
            update_task_constraint,
            add_dependency,
            remove_dependency
        ])
        .run(tauri::generate_context!())
        .expect("error while running ContractorProject");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ApplicationError, CommandError, UpdateTaskConstraintRequest};

    #[test]
    fn validation_command_error_includes_code_and_field_path() {
        let command_error = CommandError::from(ApplicationError::ValidationFailed {
            code: "task_parent_cycle",
            field: "newParentTaskId",
            message: "a task cannot be placed below its descendant".into(),
        });

        assert_eq!(
            serde_json::to_value(command_error).expect("serialize command error"),
            json!({
                "kind": "validation_failed",
                "message": "a task cannot be placed below its descendant",
                "code": "task_parent_cycle",
                "field": "newParentTaskId"
            })
        );
    }

    #[test]
    fn unknown_task_constraint_kind_is_rejected_at_the_command_boundary() {
        let request = serde_json::from_value::<UpdateTaskConstraintRequest>(json!({
            "taskId": "task-1",
            "kind": "unsupported_constraint",
            "value": "2026-08-20",
            "expectedVersion": 1,
            "expectedJobVersion": 1
        }));
        assert!(request.is_err());
    }
}
