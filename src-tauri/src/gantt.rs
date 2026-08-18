use std::collections::{HashMap, HashSet};

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::scheduling::{ScheduleResult, ScheduledTask};

pub const GANTT_READ_MODEL_VERSION: u16 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttReadModelSource {
    pub job_id: String,
    pub job_version: i64,
    /// Canonical task metadata in deterministic hierarchy pre-order.
    pub tasks: Vec<GanttTaskSource>,
    pub schedule: ScheduleResult,
    pub baseline: Option<GanttBaselineSource>,
    pub predecessors: Vec<GanttPredecessorSource>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttTaskSource {
    pub id: String,
    pub parent_task_id: Option<String>,
    pub sort_key: i64,
    pub name: String,
    pub start_no_earlier_than: Option<NaiveDate>,
    pub finish_no_later_than: Option<NaiveDate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttBaselineSource {
    pub id: String,
    /// May be partial when tasks were added after the immutable baseline.
    pub tasks: Vec<GanttBaselineTaskSource>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttBaselineTaskSource {
    pub task_id: String,
    pub start: NaiveDateTime,
    pub finish: NaiveDateTime,
    pub duration_minutes: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttPredecessorSource {
    pub task_id: String,
    pub predecessor_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GanttTaskKind {
    Summary,
    Task,
    Milestone,
}

/// Rust-derived progress status. Leaves use their own percent/actuals; summaries
/// use their duration-weighted rollup. React renders this fact and never derives it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GanttProgressStatus {
    Completed,
    InProgress,
    NotStarted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttBaselineComparison {
    pub start: NaiveDateTime,
    pub finish: NaiveDateTime,
    pub duration_minutes: i64,
    pub start_variance_minutes: i64,
    pub finish_variance_minutes: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttReadModel {
    pub contract_version: u16,
    pub job_id: String,
    pub job_version: i64,
    pub schedule_start: NaiveDateTime,
    pub schedule_finish: NaiveDateTime,
    /// Normalized job-local data-date instant, or null when the job is unstatused.
    pub data_date: Option<NaiveDateTime>,
    pub baseline_id: Option<String>,
    pub row_count: usize,
    pub critical_task_ids: Vec<String>,
    pub critical_path: Vec<String>,
    pub rows: Vec<GanttRow>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttRow {
    pub task_id: String,
    pub parent_task_id: Option<String>,
    pub logical_index: usize,
    pub depth: usize,
    pub position_in_set: usize,
    pub set_size: usize,
    pub sort_key: i64,
    pub wbs: String,
    pub name: String,
    pub kind: GanttTaskKind,
    pub has_children: bool,
    pub duration_minutes: i64,
    pub start: NaiveDateTime,
    pub finish: NaiveDateTime,
    pub total_float_minutes: i64,
    pub start_no_earlier_than: Option<NaiveDate>,
    pub finish_no_later_than: Option<NaiveDate>,
    pub constraint_violated: bool,
    pub critical: bool,
    pub milestone: bool,
    pub summary: bool,
    /// Percent complete: leaf canonical value, summary duration-weighted rollup.
    pub percent_complete: u8,
    /// Normalized actual start/finish instants; null when the leaf lacks that actual
    /// and always null on summaries (which do not fabricate actual dates).
    pub actual_start: Option<NaiveDateTime>,
    pub actual_finish: Option<NaiveDateTime>,
    /// Rust-derived completed/in-progress/not-started state.
    pub progress_status: GanttProgressStatus,
    pub predecessor_ids: Vec<String>,
    pub baseline: Option<GanttBaselineComparison>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum GanttReadModelError {
    #[error("job version must be positive")]
    InvalidJobVersion,
    #[error("task {task_id} appears more than once")]
    DuplicateTask { task_id: String },
    #[error("task metadata and schedule rows do not match for {task_id}")]
    ScheduleJoinMismatch { task_id: String },
    #[error("task hierarchy is invalid at {task_id}: {message}")]
    InvalidHierarchy { task_id: String, message: String },
    #[error("baseline contains duplicate task {task_id}")]
    DuplicateBaselineTask { task_id: String },
    #[error("baseline references unknown task {task_id}")]
    UnknownBaselineTask { task_id: String },
    #[error("predecessors are listed more than once for task {task_id}")]
    DuplicatePredecessorRow { task_id: String },
    #[error("predecessor metadata references unknown task {task_id}")]
    UnknownPredecessorTask { task_id: String },
    #[error("task {task_id} lists predecessor {predecessor_id} more than once")]
    DuplicatePredecessor {
        task_id: String,
        predecessor_id: String,
    },
    #[error("summary task {task_id} cannot carry task constraints")]
    SummaryConstraint { task_id: String },
}

impl GanttReadModelError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidJobVersion => "gantt_job_version_invalid",
            Self::DuplicateTask { .. } => "gantt_task_duplicate",
            Self::ScheduleJoinMismatch { .. } => "gantt_schedule_join_mismatch",
            Self::InvalidHierarchy { .. } => "gantt_hierarchy_invalid",
            Self::DuplicateBaselineTask { .. } => "gantt_baseline_task_duplicate",
            Self::UnknownBaselineTask { .. } => "gantt_baseline_task_unknown",
            Self::DuplicatePredecessorRow { .. } => "gantt_predecessor_row_duplicate",
            Self::UnknownPredecessorTask { .. } => "gantt_predecessor_task_unknown",
            Self::DuplicatePredecessor { .. } => "gantt_predecessor_duplicate",
            Self::SummaryConstraint { .. } => "gantt_summary_constraint_invalid",
        }
    }
}

struct HierarchyMetadata {
    source_index: usize,
    depth: usize,
    position_in_set: usize,
    set_size: usize,
    wbs: String,
    has_children: bool,
}

/// Joins Rust-owned schedule truth to the versioned transport used by React.
pub fn build_gantt_read_model(
    source: GanttReadModelSource,
) -> Result<GanttReadModel, GanttReadModelError> {
    if source.job_version < 1 {
        return Err(GanttReadModelError::InvalidJobVersion);
    }

    let task_ids = validate_unique_task_ids(&source.tasks)?;
    let schedule_by_id = join_schedule(&source.tasks, &source.schedule.tasks)?;
    let hierarchy = derive_hierarchy(&source.tasks, &task_ids)?;
    let baseline_id = source.baseline.as_ref().map(|baseline| baseline.id.clone());
    let baseline_by_task = join_baseline(source.baseline, &task_ids)?;
    let predecessors_by_task = join_predecessors(source.predecessors, &task_ids)?;
    let progress_statuses = derive_progress_statuses(&source.tasks, &schedule_by_id);

    let rows = hierarchy
        .into_iter()
        .enumerate()
        .map(|(logical_index, hierarchy)| {
            let task = &source.tasks[hierarchy.source_index];
            let scheduled = schedule_by_id[task.id.as_str()];
            if task.parent_task_id != scheduled.parent_task_id {
                return Err(GanttReadModelError::ScheduleJoinMismatch {
                    task_id: task.id.clone(),
                });
            }
            let kind = if scheduled.summary {
                GanttTaskKind::Summary
            } else if scheduled.milestone {
                GanttTaskKind::Milestone
            } else {
                GanttTaskKind::Task
            };
            if scheduled.summary
                && (task.start_no_earlier_than.is_some() || task.finish_no_later_than.is_some())
            {
                return Err(GanttReadModelError::SummaryConstraint {
                    task_id: task.id.clone(),
                });
            }
            Ok(GanttRow {
                task_id: task.id.clone(),
                parent_task_id: task.parent_task_id.clone(),
                logical_index,
                depth: hierarchy.depth,
                position_in_set: hierarchy.position_in_set,
                set_size: hierarchy.set_size,
                sort_key: task.sort_key,
                wbs: hierarchy.wbs,
                name: task.name.clone(),
                kind,
                has_children: hierarchy.has_children,
                duration_minutes: scheduled.duration_minutes,
                start: scheduled.early_start,
                finish: scheduled.early_finish,
                total_float_minutes: scheduled.total_float_minutes,
                start_no_earlier_than: task.start_no_earlier_than,
                finish_no_later_than: task.finish_no_later_than,
                // Leaf values are rejected on summaries, while the scheduler derives
                // summary violation state from constrained descendants.
                constraint_violated: scheduled.constraint_violated,
                critical: scheduled.critical,
                milestone: scheduled.milestone,
                summary: scheduled.summary,
                percent_complete: scheduled.percent_complete,
                actual_start: scheduled.actual_start,
                actual_finish: scheduled.actual_finish,
                progress_status: progress_statuses[task.id.as_str()],
                predecessor_ids: predecessors_by_task
                    .get(task.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                baseline: baseline_by_task.get(task.id.as_str()).map(|baseline| {
                    GanttBaselineComparison {
                        start: baseline.start,
                        finish: baseline.finish,
                        duration_minutes: baseline.duration_minutes,
                        start_variance_minutes: scheduled
                            .early_start
                            .signed_duration_since(baseline.start)
                            .num_minutes(),
                        finish_variance_minutes: scheduled
                            .early_finish
                            .signed_duration_since(baseline.finish)
                            .num_minutes(),
                    }
                }),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GanttReadModel {
        contract_version: GANTT_READ_MODEL_VERSION,
        job_id: source.job_id,
        job_version: source.job_version,
        schedule_start: source.schedule.schedule_start,
        schedule_finish: source.schedule.schedule_finish,
        data_date: source.schedule.data_date,
        baseline_id,
        row_count: rows.len(),
        critical_task_ids: source.schedule.critical_task_ids,
        critical_path: source.schedule.critical_path,
        rows,
    })
}

/// Derives completed/in-progress/not-started for every task. Leaves use their own
/// percent and actuals; summaries aggregate their direct children so that started
/// work is never announced as "not started" even when the duration-weighted percent
/// floors to zero (e.g. partially-started milestones).
fn derive_progress_statuses<'a>(
    tasks: &'a [GanttTaskSource],
    schedule_by_id: &HashMap<&'a str, &'a ScheduledTask>,
) -> HashMap<&'a str, GanttProgressStatus> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in tasks {
        if let Some(parent) = task.parent_task_id.as_deref() {
            children.entry(parent).or_default().push(task.id.as_str());
        }
    }
    // Tasks are in deterministic hierarchy pre-order, so iterating in reverse
    // visits every descendant before its summary parent.
    let mut statuses: HashMap<&str, GanttProgressStatus> = HashMap::with_capacity(tasks.len());
    for task in tasks.iter().rev() {
        let scheduled = schedule_by_id[task.id.as_str()];
        let status = if scheduled.summary {
            aggregate_summary_status(
                children
                    .get(task.id.as_str())
                    .into_iter()
                    .flatten()
                    .map(|child| statuses[child]),
            )
        } else {
            leaf_progress_status(scheduled)
        };
        statuses.insert(task.id.as_str(), status);
    }
    statuses
}

fn leaf_progress_status(scheduled: &ScheduledTask) -> GanttProgressStatus {
    if scheduled.percent_complete == 100 {
        GanttProgressStatus::Completed
    } else if scheduled.percent_complete == 0 && scheduled.actual_start.is_none() {
        GanttProgressStatus::NotStarted
    } else {
        GanttProgressStatus::InProgress
    }
}

/// All descendants complete -> completed; all not started -> not started;
/// otherwise in progress. An empty summary (no children) reports not started.
fn aggregate_summary_status(
    statuses: impl Iterator<Item = GanttProgressStatus>,
) -> GanttProgressStatus {
    let mut any = false;
    let mut all_completed = true;
    let mut all_not_started = true;
    for status in statuses {
        any = true;
        all_completed &= status == GanttProgressStatus::Completed;
        all_not_started &= status == GanttProgressStatus::NotStarted;
    }
    if !any || all_not_started {
        GanttProgressStatus::NotStarted
    } else if all_completed {
        GanttProgressStatus::Completed
    } else {
        GanttProgressStatus::InProgress
    }
}

fn validate_unique_task_ids(
    tasks: &[GanttTaskSource],
) -> Result<HashSet<&str>, GanttReadModelError> {
    let mut task_ids = HashSet::with_capacity(tasks.len());
    for task in tasks {
        if !task_ids.insert(task.id.as_str()) {
            return Err(GanttReadModelError::DuplicateTask {
                task_id: task.id.clone(),
            });
        }
    }
    Ok(task_ids)
}

fn join_schedule<'a>(
    tasks: &[GanttTaskSource],
    scheduled_tasks: &'a [ScheduledTask],
) -> Result<HashMap<&'a str, &'a ScheduledTask>, GanttReadModelError> {
    let mut schedule_by_id = HashMap::with_capacity(scheduled_tasks.len());
    for scheduled in scheduled_tasks {
        if schedule_by_id
            .insert(scheduled.id.as_str(), scheduled)
            .is_some()
        {
            return Err(GanttReadModelError::ScheduleJoinMismatch {
                task_id: scheduled.id.clone(),
            });
        }
    }
    for task in tasks {
        if !schedule_by_id.contains_key(task.id.as_str()) {
            return Err(GanttReadModelError::ScheduleJoinMismatch {
                task_id: task.id.clone(),
            });
        }
    }
    if tasks.len() != scheduled_tasks.len() {
        let task_ids = tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<HashSet<_>>();
        let unmatched = scheduled_tasks
            .iter()
            .find(|scheduled| !task_ids.contains(scheduled.id.as_str()))
            .map(|scheduled| scheduled.id.clone())
            .unwrap_or_else(|| "unknown".into());
        return Err(GanttReadModelError::ScheduleJoinMismatch { task_id: unmatched });
    }
    Ok(schedule_by_id)
}

fn derive_hierarchy(
    tasks: &[GanttTaskSource],
    task_ids: &HashSet<&str>,
) -> Result<Vec<HierarchyMetadata>, GanttReadModelError> {
    let mut children: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (index, task) in tasks.iter().enumerate() {
        if task.sort_key < 0 {
            return Err(GanttReadModelError::InvalidHierarchy {
                task_id: task.id.clone(),
                message: "sort key must be zero or greater".into(),
            });
        }
        if let Some(parent_id) = task.parent_task_id.as_deref() {
            if !task_ids.contains(parent_id) {
                return Err(GanttReadModelError::InvalidHierarchy {
                    task_id: task.id.clone(),
                    message: format!("parent {parent_id} does not exist"),
                });
            }
        }
        children
            .entry(task.parent_task_id.as_deref())
            .or_default()
            .push(index);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|left, right| {
            tasks[*left]
                .sort_key
                .cmp(&tasks[*right].sort_key)
                .then_with(|| tasks[*left].id.cmp(&tasks[*right].id))
        });
        for pair in siblings.windows(2) {
            if tasks[pair[0]].sort_key == tasks[pair[1]].sort_key {
                return Err(GanttReadModelError::InvalidHierarchy {
                    task_id: tasks[pair[1]].id.clone(),
                    message: "siblings must have unique sort keys".into(),
                });
            }
        }
    }

    let roots = children.get(&None).cloned().unwrap_or_default();
    let mut stack = roots
        .iter()
        .enumerate()
        .rev()
        .map(|(position, &source_index)| {
            (
                source_index,
                0_usize,
                position + 1,
                roots.len(),
                (position + 1).to_string(),
            )
        })
        .collect::<Vec<_>>();
    let mut hierarchy = Vec::with_capacity(tasks.len());
    let mut visited = HashSet::with_capacity(tasks.len());
    while let Some((source_index, depth, position_in_set, set_size, wbs)) = stack.pop() {
        let task = &tasks[source_index];
        if !visited.insert(task.id.as_str()) {
            return Err(GanttReadModelError::InvalidHierarchy {
                task_id: task.id.clone(),
                message: "task is part of a parent cycle".into(),
            });
        }
        let task_children = children
            .get(&Some(task.id.as_str()))
            .cloned()
            .unwrap_or_default();
        hierarchy.push(HierarchyMetadata {
            source_index,
            depth,
            position_in_set,
            set_size,
            wbs: wbs.clone(),
            has_children: !task_children.is_empty(),
        });
        for (position, child_index) in task_children.iter().copied().enumerate().rev() {
            stack.push((
                child_index,
                depth + 1,
                position + 1,
                task_children.len(),
                format!("{wbs}.{}", position + 1),
            ));
        }
    }
    if hierarchy.len() != tasks.len() {
        let task_id = tasks
            .iter()
            .find(|task| !visited.contains(task.id.as_str()))
            .map(|task| task.id.clone())
            .unwrap_or_else(|| "unknown".into());
        return Err(GanttReadModelError::InvalidHierarchy {
            task_id,
            message: "task is not reachable from a root".into(),
        });
    }
    for (logical_index, item) in hierarchy.iter().enumerate() {
        if item.source_index != logical_index {
            return Err(GanttReadModelError::InvalidHierarchy {
                task_id: tasks[item.source_index].id.clone(),
                message: "tasks must use deterministic hierarchy pre-order".into(),
            });
        }
    }
    Ok(hierarchy)
}

fn join_baseline(
    baseline: Option<GanttBaselineSource>,
    task_ids: &HashSet<&str>,
) -> Result<HashMap<String, GanttBaselineTaskSource>, GanttReadModelError> {
    let mut baseline_by_task = HashMap::new();
    let Some(baseline) = baseline else {
        return Ok(baseline_by_task);
    };
    for task in baseline.tasks {
        if !task_ids.contains(task.task_id.as_str()) {
            return Err(GanttReadModelError::UnknownBaselineTask {
                task_id: task.task_id,
            });
        }
        let task_id = task.task_id.clone();
        if baseline_by_task.insert(task_id.clone(), task).is_some() {
            return Err(GanttReadModelError::DuplicateBaselineTask { task_id });
        }
    }
    Ok(baseline_by_task)
}

fn join_predecessors(
    predecessors: Vec<GanttPredecessorSource>,
    task_ids: &HashSet<&str>,
) -> Result<HashMap<String, Vec<String>>, GanttReadModelError> {
    let mut predecessors_by_task = HashMap::new();
    for row in predecessors {
        if !task_ids.contains(row.task_id.as_str()) {
            return Err(GanttReadModelError::UnknownPredecessorTask {
                task_id: row.task_id,
            });
        }
        let mut seen = HashSet::new();
        let mut predecessor_ids = row.predecessor_ids;
        for predecessor_id in &predecessor_ids {
            if !task_ids.contains(predecessor_id.as_str()) {
                return Err(GanttReadModelError::UnknownPredecessorTask {
                    task_id: predecessor_id.clone(),
                });
            }
            if !seen.insert(predecessor_id.as_str()) {
                return Err(GanttReadModelError::DuplicatePredecessor {
                    task_id: row.task_id,
                    predecessor_id: predecessor_id.clone(),
                });
            }
        }
        predecessor_ids.sort();
        if predecessors_by_task
            .insert(row.task_id.clone(), predecessor_ids)
            .is_some()
        {
            return Err(GanttReadModelError::DuplicatePredecessorRow {
                task_id: row.task_id,
            });
        }
    }
    Ok(predecessors_by_task)
}
