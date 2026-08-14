pub mod application;
mod domain;
mod error;
mod storage;

use application::{ApplicationError, ApplicationService, CreateJobRequest, Job};
use serde::Serialize;
use tauri::{Manager, State};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    kind: &'static str,
    message: String,
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let service = ApplicationService::open(app_data.join("contractorproject.sqlite3"))?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![create_job, list_jobs])
        .run(tauri::generate_context!())
        .expect("error while running ContractorProject");
}
