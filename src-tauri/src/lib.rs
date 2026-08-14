pub mod application;
mod domain;
mod error;
mod storage;

use application::{
    ApplicationError, ApplicationService, CreateJobRequest, CreateTaskRequest, Job,
    ReorderTaskRequest, TaskHierarchy, TaskMutation, UpdateTaskRequest,
};
use serde::Serialize;
use tauri::{Manager, State};

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
    None {},
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        let details = match &error {
            ApplicationError::InvalidInput { field, .. } => {
                CommandErrorDetails::InvalidInput { field }
            }
            ApplicationError::ValidationFailed { code, .. } => {
                CommandErrorDetails::Validation { code }
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
            ApplicationError::InvalidStoredData(_)
            | ApplicationError::Database(_)
            | ApplicationError::Io(_) => CommandErrorDetails::None {},
        };
        Self {
            kind: error.kind(),
            message: error.to_string(),
            details: Box::new(details),
        }
    }
}

#[tauri::command]
fn create_job(
    service: State<'_, ApplicationService>,
    request: CreateJobRequest,
) -> Result<Job, CommandError> {
    service.create_job(request).map_err(Into::into)
}

#[tauri::command]
fn list_jobs(service: State<'_, ApplicationService>) -> Result<Vec<Job>, CommandError> {
    service.list_jobs().map_err(Into::into)
}

#[tauri::command]
fn create_task(
    service: State<'_, ApplicationService>,
    request: CreateTaskRequest,
) -> Result<TaskMutation, CommandError> {
    service.create_task(request).map_err(Into::into)
}

#[tauri::command]
fn list_tasks(
    service: State<'_, ApplicationService>,
    job_id: String,
) -> Result<TaskHierarchy, CommandError> {
    service.list_tasks(&job_id).map_err(Into::into)
}

#[tauri::command]
fn update_task(
    service: State<'_, ApplicationService>,
    request: UpdateTaskRequest,
) -> Result<TaskMutation, CommandError> {
    service.update_task(request).map_err(Into::into)
}

#[tauri::command]
fn reorder_task(
    service: State<'_, ApplicationService>,
    request: ReorderTaskRequest,
) -> Result<TaskHierarchy, CommandError> {
    service.reorder_task(request).map_err(Into::into)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let service = ApplicationService::open(app_data.join("contractorproject.sqlite3"))?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_job,
            list_jobs,
            create_task,
            list_tasks,
            update_task,
            reorder_task
        ])
        .run(tauri::generate_context!())
        .expect("error while running ContractorProject");
}
