use serde::{Deserialize, Serialize};

use crate::scheduling::WorkingCalendar;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub name: String,
    pub status: JobStatus,
    pub timezone: String,
    pub schedule_start: Option<String>,
    pub calendar: WorkingCalendar,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub job_id: String,
    pub parent_task_id: Option<String>,
    pub sort_key: i64,
    pub name: String,
    /// None is an incomplete or summary input; scheduling is validated when requested.
    pub duration_minutes: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishStartDependency {
    pub predecessor_task_id: String,
    pub successor_task_id: String,
    pub lag_minutes: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Draft,
}

impl JobStatus {
    pub(crate) fn as_database_value(self) -> &'static str {
        match self {
            Self::Draft => "draft",
        }
    }

    pub(crate) fn from_database_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            _ => None,
        }
    }
}
