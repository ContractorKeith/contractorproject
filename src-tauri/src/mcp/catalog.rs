//! The tool catalog: every tool the helper exposes, its mode, and its input schema.
//!
//! Schemas are hints for the agent; the real gate is the Rust application seam,
//! which validates every field exactly as it does for the desktop UI.
//! docs/LOCAL_API.md is kept in step with this list by hand.

use serde_json::{json, Map, Value};

/// Read tools are listed in both modes; write tools only with --read-write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
}

#[derive(Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub ty: &'static str,
    pub required: bool,
    pub note: &'static str,
}

pub struct ToolSpec {
    pub name: &'static str,
    pub access: Access,
    pub description: &'static str,
    pub fields: &'static [Field],
}

const fn req(name: &'static str, ty: &'static str, note: &'static str) -> Field {
    Field {
        name,
        ty,
        required: true,
        note,
    }
}

const fn opt(name: &'static str, ty: &'static str, note: &'static str) -> Field {
    Field {
        name,
        ty,
        required: false,
        note,
    }
}

/// Default and maximum page size for list_jobs.
pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 200;

/// Every write carries this; reuse it only when retrying the same change.
const COMMAND_ID: Field = req(
    "commandId",
    "string",
    "Unique ID for this change (max 128 chars). Reuse it only to retry the same change; \
     a reused ID that already applied returns duplicate_command and changes nothing.",
);
const JOB_ID: Field = req("jobId", "string", "Job ID from list_jobs");
const TASK_ID: Field = req("taskId", "string", "Task ID from list_tasks");
const EXPECTED_JOB_VERSION: Field = req(
    "expectedJobVersion",
    "integer",
    "The job version you last read; a stale value returns version_conflict with the current version",
);
const EXPECTED_VERSION: Field = req(
    "expectedVersion",
    "integer",
    "The task version you last read",
);

pub fn tools() -> &'static [ToolSpec] {
    TOOLS
}

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// The JSON Schema a client sees for one tool.
pub fn input_schema(tool: &ToolSpec) -> Value {
    let mut properties = Map::new();
    let mut required: Vec<Value> = Vec::new();
    for field in tool.fields {
        let schema = if field.ty == "string|null" {
            json!({ "type": ["string", "null"], "description": field.note })
        } else if field.ty == "integer|null" {
            json!({ "type": ["integer", "null"], "description": field.note })
        } else {
            json!({ "type": field.ty, "description": field.note })
        };
        properties.insert(field.name.into(), schema);
        if field.required {
            required.push(json!(field.name));
        }
    }
    json!({ "type": "object", "properties": properties, "required": required })
}

pub fn descriptor(tool: &ToolSpec) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": input_schema(tool),
        "annotations": { "readOnlyHint": tool.access == Access::Read },
    })
}

static TOOLS: &[ToolSpec] = &[
    // --- reads -----------------------------------------------------------------
    ToolSpec {
        name: "list_jobs",
        access: Access::Read,
        description: "List jobs. Draft jobs by default; pass status \"archived\" for archived \
                      ones. Returns { items, totalCount, limit, offset }; default 50, max 200.",
        fields: &[
            opt("status", "string", "\"draft\" (default) or \"archived\""),
            opt("limit", "integer", "1–200; default 50"),
            opt("offset", "integer", "rows to skip; default 0"),
        ],
    },
    ToolSpec {
        name: "get_job",
        access: Access::Read,
        description: "Get one job (draft or archived) with its version, calendar, schedule start \
                      and data date. Set includeTasks to also return its task hierarchy.",
        fields: &[
            JOB_ID,
            opt("includeTasks", "boolean", "also return list_tasks output"),
        ],
    },
    ToolSpec {
        name: "list_tasks",
        access: Access::Read,
        description: "The job's task hierarchy as a flat pre-order list with parent IDs, plus its \
                      dependencies and the current job version.",
        fields: &[JOB_ID],
    },
    ToolSpec {
        name: "get_schedule",
        access: Access::Read,
        description: "The calculated schedule (the same Gantt read model the desktop app shows): \
                      task dates, critical path, float, and baseline comparison. Fails with \
                      validation_failed when the job has no schedule start or durations.",
        fields: &[JOB_ID],
    },
    ToolSpec {
        name: "list_baselines",
        access: Access::Read,
        description: "List the job's saved schedule baselines.",
        fields: &[JOB_ID],
    },
    // --- writes ----------------------------------------------------------------
    ToolSpec {
        name: "create_job",
        access: Access::Write,
        description: "Create a draft job.",
        fields: &[
            COMMAND_ID,
            req("name", "string", "Job name, max 120 chars"),
            req("timezone", "string", "IANA timezone, e.g. America/New_York"),
        ],
    },
    ToolSpec {
        name: "archive_job",
        access: Access::Write,
        description: "Archive a draft job. Archived jobs cannot be edited until restored.",
        fields: &[COMMAND_ID, JOB_ID, EXPECTED_JOB_VERSION],
    },
    ToolSpec {
        name: "restore_job",
        access: Access::Write,
        description: "Restore an archived job to draft.",
        fields: &[COMMAND_ID, JOB_ID, EXPECTED_JOB_VERSION],
    },
    ToolSpec {
        name: "create_task",
        access: Access::Write,
        description: "Add a task to a job, optionally under a parent task. Returns the task and \
                      the new job version.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            opt(
                "parentTaskId",
                "string|null",
                "Parent task ID, or omit for a top-level task",
            ),
            req("name", "string", "Task name, max 200 chars"),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "update_task",
        access: Access::Write,
        description: "Rename a task.",
        fields: &[
            COMMAND_ID,
            TASK_ID,
            req("name", "string", "New name"),
            EXPECTED_VERSION,
        ],
    },
    ToolSpec {
        name: "reorder_task",
        access: Access::Write,
        description: "Move a task to a new parent and position. Returns the updated hierarchy.",
        fields: &[
            COMMAND_ID,
            TASK_ID,
            opt(
                "newParentTaskId",
                "string|null",
                "New parent, or null for top level",
            ),
            req(
                "newSiblingIndex",
                "integer",
                "0-based position among the new siblings",
            ),
            EXPECTED_VERSION,
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "add_dependency",
        access: Access::Write,
        description:
            "Link two leaf tasks. Both tasks need a duration first (update_task_duration). \
                      A link that would create a cycle returns dependency_cycle.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            req("predecessorTaskId", "string", "Task that drives the link"),
            req("successorTaskId", "string", "Task that follows"),
            opt("dependencyType", "string", "FS (default), SS, FF, or SF"),
            req(
                "lagMinutes",
                "integer",
                "Working-minute lag; 0 for none, negative for lead",
            ),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "remove_dependency",
        access: Access::Write,
        description: "Remove a link between two tasks.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            req("predecessorTaskId", "string", "Predecessor task ID"),
            req("successorTaskId", "string", "Successor task ID"),
            opt("dependencyType", "string", "FS (default), SS, FF, or SF"),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "update_schedule",
        access: Access::Write,
        description: "Set the job's schedule start and weekly working calendar.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            opt(
                "scheduleStart",
                "string|null",
                "ISO date YYYY-MM-DD, or null to clear",
            ),
            req(
                "calendar",
                "object",
                "{ workingWeekdays: [\"monday\",…], workdayStartMinute: 420, \
                 workdayDurationMinutes: 480 } — minutes after midnight and per day",
            ),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "update_task_duration",
        access: Access::Write,
        description: "Set a leaf task's duration in working minutes (480 = one 8-hour day).",
        fields: &[
            COMMAND_ID,
            TASK_ID,
            opt(
                "durationMinutes",
                "integer|null",
                "Working minutes, or null to clear",
            ),
            EXPECTED_VERSION,
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "update_task_constraint",
        access: Access::Write,
        description: "Set or clear a leaf task's date constraint.",
        fields: &[
            COMMAND_ID,
            TASK_ID,
            req(
                "kind",
                "string",
                "start_no_earlier_than or finish_no_later_than",
            ),
            opt(
                "value",
                "string|null",
                "ISO date YYYY-MM-DD, or null to clear",
            ),
            EXPECTED_VERSION,
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "update_job_data_date",
        access: Access::Write,
        description:
            "Set or clear the job's data date (the status date progress is measured from).",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            opt(
                "dataDate",
                "string|null",
                "ISO date YYYY-MM-DD, or null to clear",
            ),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "add_calendar_exception",
        access: Access::Write,
        description: "Mark a date as non-working for this job (holiday, closure, rain day).",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            req("date", "string", "ISO date YYYY-MM-DD"),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "remove_calendar_exception",
        access: Access::Write,
        description: "Remove a non-working date from this job's calendar.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            req("date", "string", "ISO date YYYY-MM-DD"),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "update_task_progress",
        access: Access::Write,
        description: "Record progress on a leaf task: percent complete and actual start/finish \
                      dates. Set clear to true to remove all progress.",
        fields: &[
            COMMAND_ID,
            TASK_ID,
            req(
                "clear",
                "boolean",
                "true removes progress and ignores the other fields",
            ),
            opt("percentComplete", "integer|null", "0–100"),
            opt("actualStart", "string|null", "ISO date YYYY-MM-DD"),
            opt("actualFinish", "string|null", "ISO date YYYY-MM-DD"),
            EXPECTED_VERSION,
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "create_baseline",
        access: Access::Write,
        description: "Save the current calculated schedule as a named baseline.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            req("name", "string", "Baseline name, max 120 chars"),
            EXPECTED_JOB_VERSION,
        ],
    },
    ToolSpec {
        name: "set_baseline_comparison_default",
        access: Access::Write,
        description: "Choose which baseline the schedule compares against by default.",
        fields: &[
            COMMAND_ID,
            JOB_ID,
            req("baselineId", "string", "Baseline ID from list_baselines"),
            EXPECTED_JOB_VERSION,
        ],
    },
];
