use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub name: String,
    pub status: JobStatus,
    pub timezone: String,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
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
