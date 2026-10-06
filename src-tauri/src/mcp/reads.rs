//! The read tools. Each calls the same `ApplicationService` read the desktop UI
//! uses; nothing here touches SQLite or computes schedule facts itself.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::application::{ApplicationService, Job, JobStatus};
use crate::error::ApplicationError;

use super::catalog::{DEFAULT_LIMIT, MAX_LIMIT};
use super::{arguments, to_json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListJobsArgs {
    status: Option<JobStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobArgs {
    job_id: String,
    #[serde(default)]
    include_tasks: bool,
}

/// Run a read tool, or return `None` when the name is not one.
pub fn call(
    service: &ApplicationService,
    name: &str,
    args: &Value,
) -> Option<Result<Value, ApplicationError>> {
    let args = args.clone();
    let answer = match name {
        "list_jobs" => arguments(args).and_then(|a| list_jobs(service, a)),
        "get_job" => arguments(args).and_then(|a| get_job(service, a)),
        "list_tasks" => {
            arguments::<JobArgs>(args).and_then(|a| to_json(service.list_tasks(&a.job_id)))
        }
        "get_schedule" => {
            arguments::<JobArgs>(args).and_then(|a| to_json(service.get_schedule(&a.job_id)))
        }
        "list_baselines" => {
            arguments::<JobArgs>(args).and_then(|a| to_json(service.list_baselines(&a.job_id)))
        }
        _ => return None,
    };
    Some(answer)
}

/// Bounded, offset-paged job list.
fn list_jobs(service: &ApplicationService, args: ListJobsArgs) -> Result<Value, ApplicationError> {
    let limit = match args.limit {
        None => DEFAULT_LIMIT,
        Some(n) if (1..=MAX_LIMIT as i64).contains(&n) => n as usize,
        Some(_) => {
            return Err(ApplicationError::InvalidInput {
                field: "limit",
                message: format!("must be between 1 and {MAX_LIMIT}"),
            })
        }
    };
    let offset = match args.offset {
        None => 0,
        Some(n) if n >= 0 => n as usize,
        Some(_) => {
            return Err(ApplicationError::InvalidInput {
                field: "offset",
                message: "must be 0 or more".into(),
            })
        }
    };
    let jobs = service.list_jobs_by_status(args.status.unwrap_or(JobStatus::Draft))?;
    let total = jobs.len();
    let items: Vec<Job> = jobs.into_iter().skip(offset).take(limit).collect();
    Ok(json!({ "items": items, "totalCount": total, "limit": limit, "offset": offset }))
}

/// One job by ID, searching draft then archived jobs through the seam.
fn get_job(service: &ApplicationService, args: JobArgs) -> Result<Value, ApplicationError> {
    let job = find_job(service, &args.job_id)?;
    let mut value = json!({ "job": job });
    if args.include_tasks {
        value["tasks"] = serde_json::to_value(service.list_tasks(&args.job_id)?)
            .map_err(|error| ApplicationError::InvalidStoredData(error.to_string()))?;
    }
    Ok(value)
}

fn find_job(service: &ApplicationService, job_id: &str) -> Result<Job, ApplicationError> {
    for status in [JobStatus::Draft, JobStatus::Archived] {
        if let Some(job) = service
            .list_jobs_by_status(status)?
            .into_iter()
            .find(|job| job.id == job_id)
        {
            return Ok(job);
        }
    }
    Err(ApplicationError::NotFound {
        resource: "job",
        id: super::bounded(job_id),
    })
}
