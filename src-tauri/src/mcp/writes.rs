//! The write tools. Each deserializes into the same request type the Tauri
//! command takes and calls the same `ApplicationService` method, under a
//! `CommandContext` whose actor is `agent`, whose client name is the helper's
//! --client-name, and whose command ID comes from the caller. There is no second
//! write path: validation, versions, and the audit row are the app's.

use serde_json::Value;

use crate::application::{ApplicationService, CommandActor, CommandContext};
use crate::error::ApplicationError;

use super::{arguments, to_json};

/// Run a write tool, or return `None` when the name is not one.
pub fn call(
    service: &ApplicationService,
    client_name: &str,
    name: &str,
    args: &Value,
) -> Option<Result<Value, ApplicationError>> {
    if !WRITE_NAMES.contains(&name) {
        return None;
    }
    let context = match command_context(client_name, args) {
        Ok(context) => context,
        Err(error) => return Some(Err(error)),
    };
    let args = args.clone();
    let s = service;
    let c = context;
    let answer = match name {
        "create_job" => arguments(args).and_then(|r| to_json(s.create_job(c, r))),
        "archive_job" => arguments(args).and_then(|r| to_json(s.archive_job(c, r))),
        "restore_job" => arguments(args).and_then(|r| to_json(s.restore_job(c, r))),
        "create_task" => arguments(args).and_then(|r| to_json(s.create_task(c, r))),
        "update_task" => arguments(args).and_then(|r| to_json(s.update_task(c, r))),
        "reorder_task" => arguments(args).and_then(|r| to_json(s.reorder_task(c, r))),
        "add_dependency" => arguments(args).and_then(|r| to_json(s.add_dependency(c, r))),
        "remove_dependency" => arguments(args).and_then(|r| to_json(s.remove_dependency(c, r))),
        "update_schedule" => arguments(args).and_then(|r| to_json(s.update_schedule(c, r))),
        "update_task_duration" => {
            arguments(args).and_then(|r| to_json(s.update_task_duration(c, r)))
        }
        "update_task_constraint" => {
            arguments(args).and_then(|r| to_json(s.update_task_constraint(c, r)))
        }
        "update_job_data_date" => {
            arguments(args).and_then(|r| to_json(s.update_job_data_date(c, r)))
        }
        "add_calendar_exception" => {
            arguments(args).and_then(|r| to_json(s.add_calendar_exception(c, r)))
        }
        "remove_calendar_exception" => {
            arguments(args).and_then(|r| to_json(s.remove_calendar_exception(c, r)))
        }
        "update_task_progress" => {
            arguments(args).and_then(|r| to_json(s.update_task_progress(c, r)))
        }
        "create_baseline" => arguments(args).and_then(|r| to_json(s.create_baseline(c, r))),
        "set_baseline_comparison_default" => {
            arguments(args).and_then(|r| to_json(s.set_baseline_comparison_default(c, r)))
        }
        _ => return None,
    };
    Some(answer)
}

/// Build the audit context. The caller supplies `commandId` so a retry of the
/// same change is recognized as a duplicate instead of applied twice.
fn command_context(client_name: &str, args: &Value) -> Result<CommandContext, ApplicationError> {
    let command_id =
        args.get("commandId")
            .and_then(Value::as_str)
            .ok_or(ApplicationError::InvalidInput {
                field: "commandId",
                message: "every write needs a commandId string".into(),
            })?;
    Ok(CommandContext {
        command_id: command_id.to_string(),
        actor: CommandActor::Agent,
        client_name: client_name.to_string(),
    })
}

/// Mirrors the match arms above; a test keeps it in step with the catalog.
pub const WRITE_NAMES: &[&str] = &[
    "create_job",
    "archive_job",
    "restore_job",
    "create_task",
    "update_task",
    "reorder_task",
    "add_dependency",
    "remove_dependency",
    "update_schedule",
    "update_task_duration",
    "update_task_constraint",
    "update_job_data_date",
    "add_calendar_exception",
    "remove_calendar_exception",
    "update_task_progress",
    "create_baseline",
    "set_baseline_comparison_default",
];
