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
    /// Sorted canonical YYYY-MM-DD dated non-working calendar exceptions, loaded
    /// with the job so the UI can render and manage them without a new query.
    #[serde(default)]
    pub calendar_exceptions: Vec<String>,
    /// Canonical YYYY-MM-DD job-local data date; None while the job is unstatused.
    pub data_date: Option<String>,
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
    pub start_no_earlier_than: Option<String>,
    pub finish_no_later_than: Option<String>,
    /// Percent complete in [0, 100]; None is unstatused (equivalent to 0 with no actuals).
    pub percent_complete: Option<i64>,
    /// Canonical YYYY-MM-DD actual start; None when unreported.
    pub actual_start: Option<String>,
    /// Canonical YYYY-MM-DD actual finish; None until the leaf is complete.
    pub actual_finish: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
}

/// An immutable named schedule snapshot. Task rows live in `baseline_tasks`;
/// exactly one baseline per job may be the comparison default.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub id: String,
    pub job_id: String,
    pub name: String,
    pub created_at: String,
    pub is_comparison_default: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishStartDependency {
    pub predecessor_task_id: String,
    pub successor_task_id: String,
    #[serde(default)]
    pub dependency_type: crate::scheduling::DependencyType,
    pub lag_minutes: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Draft,
    Archived,
}

impl JobStatus {
    pub(crate) fn as_database_value(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Archived => "archived",
        }
    }

    pub(crate) fn from_database_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}
