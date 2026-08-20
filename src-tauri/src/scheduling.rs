use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_HIERARCHY_DEPTH: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl CalendarWeekday {
    fn chrono(self) -> Weekday {
        match self {
            Self::Monday => Weekday::Mon,
            Self::Tuesday => Weekday::Tue,
            Self::Wednesday => Weekday::Wed,
            Self::Thursday => Weekday::Thu,
            Self::Friday => Weekday::Fri,
            Self::Saturday => Weekday::Sat,
            Self::Sunday => Weekday::Sun,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingCalendar {
    pub working_weekdays: Vec<CalendarWeekday>,
    pub workday_start_minute: u16,
    pub workday_duration_minutes: u16,
    /// Dated non-working civil days (holidays/closures). Each is treated exactly
    /// like a weekly non-working day by every traversal. Serialized only when
    /// non-empty so calendars without exceptions stay byte-identical; persisted
    /// exceptions live in the `calendar_exceptions` table, not `calendar_json`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<NaiveDate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleTask {
    pub id: String,
    pub parent_task_id: Option<String>,
    /// `None` identifies a summary whose schedule is derived from its children.
    pub duration_minutes: Option<i64>,
}

/// The four dependency relationship types. Serialized as the two-letter codes
/// `FS`, `SS`, `FF`, `SF`. The declaration order is the deterministic tie-break
/// order for edges that share a predecessor/successor pair.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum DependencyType {
    #[default]
    #[serde(rename = "FS")]
    FinishStart,
    #[serde(rename = "SS")]
    StartStart,
    #[serde(rename = "FF")]
    FinishFinish,
    #[serde(rename = "SF")]
    StartFinish,
}

impl DependencyType {
    /// The canonical two-letter code used in persistence and JSON.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::FinishStart => "FS",
            Self::StartStart => "SS",
            Self::FinishFinish => "FF",
            Self::StartFinish => "SF",
        }
    }

    /// Parses a canonical two-letter code, returning `None` for anything else.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "FS" => Some(Self::FinishStart),
            "SS" => Some(Self::StartStart),
            "FF" => Some(Self::FinishFinish),
            "SF" => Some(Self::StartFinish),
            _ => None,
        }
    }
}

/// A typed dependency between two leaf tasks. The name is retained for
/// compatibility; the `dependency_type` field generalizes it to all four
/// relationship types. An omitted type deserializes as `FS`, so FS-only inputs
/// are byte-identical to earlier slices.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishStartDependency {
    pub predecessor_task_id: String,
    pub successor_task_id: String,
    #[serde(default)]
    pub dependency_type: DependencyType,
    pub lag_minutes: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInput {
    pub schedule_start: NaiveDate,
    pub calendar: WorkingCalendar,
    pub tasks: Vec<ScheduleTask>,
    pub dependencies: Vec<FinishStartDependency>,
}

/// Job-local civil-date constraints applied to one leaf task while calculating a schedule.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskConstraint {
    pub task_id: String,
    pub start_no_earlier_than: Option<NaiveDate>,
    pub finish_no_later_than: Option<NaiveDate>,
}

/// A compatibility name that makes the leaf-only constraint scope explicit.
pub type LeafTaskConstraint = TaskConstraint;

/// Reported progress for one leaf task, supplied separately from the task list.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub task_id: String,
    /// Integer percent complete in `[0, 100]`.
    pub percent_complete: u8,
    pub actual_start: Option<NaiveDate>,
    pub actual_finish: Option<NaiveDate>,
}

/// A job data date and its per-leaf progress entries for a status update.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleProgress {
    /// Job-local civil data date. Required when any progress entry is present.
    pub data_date: Option<NaiveDate>,
    pub entries: Vec<TaskProgress>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    pub id: String,
    pub parent_task_id: Option<String>,
    pub duration_minutes: i64,
    pub early_start: NaiveDateTime,
    pub early_finish: NaiveDateTime,
    pub late_start: NaiveDateTime,
    pub late_finish: NaiveDateTime,
    pub total_float_minutes: i64,
    pub critical: bool,
    pub constraint_violated: bool,
    pub milestone: bool,
    pub summary: bool,
    /// Percent complete: leaf value, or duration-weighted rollup for summaries.
    pub percent_complete: u8,
    /// Normalized actual start instant when the leaf has reported progress.
    pub actual_start: Option<NaiveDateTime>,
    /// Normalized actual finish instant when the leaf is complete.
    pub actual_finish: Option<NaiveDateTime>,
}

/// One forward-pass lower bound on a leaf's remaining-work start. Every driver
/// serializes with an internal `kind` tag and camelCase fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ScheduleDriver {
    /// The schedule-start floor (including a negative-lag pull clamped at start).
    ScheduleStart {},
    /// The leaf's SNET, carrying the entered civil date and the applied normalized date.
    StartConstraint {
        date: NaiveDate,
        normalized_date: NaiveDate,
    },
    /// The normalized data-date instant pushing incomplete work.
    DataDate { date: NaiveDateTime },
    /// One typed predecessor link. FS/SS bound the start; FF/SF bound the
    /// remaining-work finish, converted through the successor's remaining duration.
    Predecessor {
        task_id: String,
        dependency_type: DependencyType,
        lag_minutes: i64,
    },
}

/// A run of non-working civil days between a driver's reference date and the
/// leaf's arrival date. Endpoints are ascending; the count covers `[from, to)`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarGap {
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub non_working_day_count: i64,
}

/// What bounded a leaf's late finish in the backward pass (one level only).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LateFinishLimit {
    /// The leaf's own FNLT, with entered and normalized dates.
    Deadline {
        date: NaiveDate,
        normalized_date: NaiveDate,
    },
    /// The minimum binding successor bound as a typed link.
    Successor {
        task_id: String,
        dependency_type: DependencyType,
        lag_minutes: i64,
    },
    /// The project-finish anchor.
    ProjectFinish {},
}

/// A deterministic, typed explanation of why one task starts and finishes when
/// it does. It is a pure post-pass over the same forward/backward passes: it
/// introduces no scheduling semantics and changes no dates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskExplanation {
    /// A summary whose dates are derived from its children by the rollup rules.
    Summary { task_id: String },
    /// A complete leaf, anchored at its normalized actual start and finish.
    Complete {
        task_id: String,
        actual_start: NaiveDateTime,
        actual_finish: NaiveDateTime,
    },
    /// Any other leaf (not started or in progress), with its driver and float rationale.
    Scheduled {
        task_id: String,
        primary_driver: ScheduleDriver,
        other_binding_drivers: Vec<ScheduleDriver>,
        started_actual_start: Option<NaiveDateTime>,
        calendar_gap: Option<CalendarGap>,
        total_float_minutes: i64,
        critical: bool,
        late_finish_limit: LateFinishLimit,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleResult {
    pub schedule_start: NaiveDateTime,
    pub schedule_finish: NaiveDateTime,
    pub tasks: Vec<ScheduledTask>,
    pub critical_task_ids: Vec<String>,
    pub critical_path: Vec<String>,
    pub directly_violated_leaf_task_ids: Vec<String>,
    /// Normalized data-date instant when the job carries a data date.
    pub data_date: Option<NaiveDateTime>,
    /// One deterministic explanation per task, ordered exactly like `tasks`.
    pub explanations: Vec<TaskExplanation>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ScheduleError {
    #[error("{message}")]
    InvalidCalendar { code: &'static str, message: String },
    #[error("{message}")]
    InvalidTask { code: &'static str, message: String },
    #[error("{message}")]
    InvalidDependency { code: &'static str, message: String },
    #[error("{message}")]
    InvalidProgress { code: &'static str, message: String },
    #[error("task dependencies contain a cycle")]
    DependencyCycle { task_ids: Vec<String> },
    #[error("the calculated schedule exceeds the supported date range")]
    ScheduleOutOfRange,
}

impl ScheduleError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCalendar { code, .. }
            | Self::InvalidTask { code, .. }
            | Self::InvalidDependency { code, .. }
            | Self::InvalidProgress { code, .. } => code,
            Self::DependencyCycle { .. } => "dependency_cycle",
            Self::ScheduleOutOfRange => "schedule_out_of_range",
        }
    }
}

/// Calculates one deterministic, unconstrained finish-to-start schedule.
pub fn calculate_schedule(input: &ScheduleInput) -> Result<ScheduleResult, ScheduleError> {
    calculate_schedule_with_constraints(input, &[])
}

/// Calculates one deterministic finish-to-start schedule with optional leaf constraints.
pub fn calculate_schedule_with_constraints(
    input: &ScheduleInput,
    constraints: &[TaskConstraint],
) -> Result<ScheduleResult, ScheduleError> {
    calculate_schedule_with_progress(input, constraints, &ScheduleProgress::default())
}

/// Calculates one deterministic finish-to-start schedule with optional leaf
/// constraints and optional data-date/progress status.
pub fn calculate_schedule_with_progress(
    input: &ScheduleInput,
    constraints: &[TaskConstraint],
    progress: &ScheduleProgress,
) -> Result<ScheduleResult, ScheduleError> {
    let calendar = CalendarMath::new(&input.calendar, input.schedule_start)?;
    let hierarchy = TaskHierarchy::new(&input.tasks)?;
    let graph = LeafGraph::new(&hierarchy, &input.dependencies)?;
    // Capture the entered civil dates (used by explanations) before the typed
    // `constraints`/`progress` sets shadow the raw inputs.
    let entered_snet: HashMap<&str, NaiveDate> = constraints
        .iter()
        .filter_map(|c| c.start_no_earlier_than.map(|d| (c.task_id.as_str(), d)))
        .collect();
    let entered_fnlt: HashMap<&str, NaiveDate> = constraints
        .iter()
        .filter_map(|c| c.finish_no_later_than.map(|d| (c.task_id.as_str(), d)))
        .collect();
    let constraints = ConstraintSet::new(constraints, &hierarchy, &graph, &calendar)?;
    let progress = ProgressSet::new(progress, &hierarchy, &graph, &calendar)?;
    let task_count = graph.tasks.len();

    // Forward pass over remaining work. Complete leaves anchor at their actuals;
    // incomplete leaves lower-bound their remaining work by the data-date instant
    // (when set), their own SNET, and predecessor finishes plus lag.
    let mut early_bound = vec![0_i64; task_count];
    for (index, bound) in early_bound.iter_mut().enumerate() {
        if progress.status[index] != ProgressStatus::Complete {
            *bound = constraints.start_no_earlier_than[index].max(0);
            if let Some(data_date) = progress.data_date_offset {
                *bound = (*bound).max(data_date);
            }
        }
    }
    let mut early_start = vec![0_i64; task_count];
    let mut remaining_start = vec![0_i64; task_count];
    let mut early_finish = vec![0_i64; task_count];
    for &task_index in &graph.topological_order {
        match progress.status[task_index] {
            ProgressStatus::Complete => {
                let start =
                    progress.actual_start_offset[task_index].expect("complete actual start");
                early_start[task_index] = start;
                remaining_start[task_index] = start;
                early_finish[task_index] =
                    progress.actual_finish_offset[task_index].expect("complete actual finish");
            }
            status => {
                let start = early_bound[task_index];
                remaining_start[task_index] = start;
                early_start[task_index] = if status == ProgressStatus::InProgress {
                    progress.actual_start_offset[task_index].expect("in-progress actual start")
                } else {
                    start
                };
                early_finish[task_index] = start
                    .checked_add(progress.remaining_duration[task_index])
                    .ok_or(ScheduleError::ScheduleOutOfRange)?;
            }
        }
        for edge in &graph.successors[task_index] {
            // FS/FF bind the successor to the predecessor finish; SS/SF bind it
            // to the predecessor start (its actual start when progressed).
            let anchor = match edge.dep_type {
                DependencyType::FinishStart | DependencyType::FinishFinish => {
                    early_finish[task_index]
                }
                DependencyType::StartStart | DependencyType::StartFinish => early_start[task_index],
            };
            let mut contribution = anchor
                .checked_add(edge.lag)
                .ok_or(ScheduleError::ScheduleOutOfRange)?;
            if progress.status[task_index] == ProgressStatus::Complete {
                if let Some(data_date) = progress.data_date_offset {
                    contribution = contribution.max(data_date);
                }
            }
            // FF/SF bound the successor finish; derive its start by subtracting
            // the successor's remaining duration.
            let start_bound = match edge.dep_type {
                DependencyType::FinishStart | DependencyType::StartStart => contribution,
                DependencyType::FinishFinish | DependencyType::StartFinish => contribution
                    .checked_sub(progress.remaining_duration[edge.other])
                    .ok_or(ScheduleError::ScheduleOutOfRange)?,
            };
            early_bound[edge.other] = early_bound[edge.other].max(start_bound);
        }
    }

    // The project finish is driven by remaining (incomplete) work; only when
    // every leaf is complete does it fall back to the latest actual finish.
    let schedule_finish_offset = (0..task_count)
        .filter(|&index| progress.status[index] != ProgressStatus::Complete)
        .map(|index| early_finish[index])
        .max()
        .or_else(|| early_finish.iter().copied().max())
        .unwrap_or(0);
    // Backward pass over remaining work. Complete leaves are fixed points that
    // neither receive nor impose remaining-work float.
    let mut late_finish = vec![schedule_finish_offset; task_count];
    let mut late_start = vec![0_i64; task_count];
    for &task_index in graph.topological_order.iter().rev() {
        if progress.status[task_index] == ProgressStatus::Complete {
            late_finish[task_index] = early_finish[task_index];
            late_start[task_index] = early_start[task_index];
            continue;
        }
        late_finish[task_index] =
            late_finish[task_index].min(constraints.finish_no_later_than[task_index]);
        let remaining = progress.remaining_duration[task_index];
        // Each successor imposes an upper bound on this task's late finish,
        // mirroring the forward-pass rule per type.
        let mut successor_bound = i64::MAX;
        let predecessor_started = progress.status[task_index] == ProgressStatus::InProgress;
        for edge in &graph.successors[task_index] {
            if progress.status[edge.other] == ProgressStatus::Complete {
                continue;
            }
            // SS/SF anchor on this (predecessor) task's start. Once it is started
            // that start is an immovable actual, so the successor imposes no late
            // bound through the edge — mirroring the forward-pass retained anchor.
            if predecessor_started
                && matches!(
                    edge.dep_type,
                    DependencyType::StartStart | DependencyType::StartFinish
                )
            {
                continue;
            }
            let bound = match edge.dep_type {
                DependencyType::FinishStart => late_start[edge.other]
                    .checked_sub(edge.lag)
                    .ok_or(ScheduleError::ScheduleOutOfRange)?,
                DependencyType::StartStart => late_start[edge.other]
                    .checked_sub(edge.lag)
                    .and_then(|value| value.checked_add(remaining))
                    .ok_or(ScheduleError::ScheduleOutOfRange)?,
                DependencyType::FinishFinish => late_finish[edge.other]
                    .checked_sub(edge.lag)
                    .ok_or(ScheduleError::ScheduleOutOfRange)?,
                DependencyType::StartFinish => late_finish[edge.other]
                    .checked_sub(edge.lag)
                    .and_then(|value| value.checked_add(remaining))
                    .ok_or(ScheduleError::ScheduleOutOfRange)?,
            };
            successor_bound = successor_bound.min(bound);
        }
        if successor_bound != i64::MAX {
            late_finish[task_index] = late_finish[task_index].min(successor_bound);
        }
        late_start[task_index] = late_finish[task_index]
            .checked_sub(progress.remaining_duration[task_index])
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
    }

    let mut total_float = vec![0_i64; task_count];
    for index in 0..task_count {
        if progress.status[index] != ProgressStatus::Complete {
            total_float[index] = late_start[index]
                .checked_sub(remaining_start[index])
                .ok_or(ScheduleError::ScheduleOutOfRange)?;
        }
    }
    let mut directly_violated = Vec::new();
    for index in 0..task_count {
        let Some(deadline_date) = constraints.normalized_finish_dates[index] else {
            continue;
        };
        let milestone = graph.durations[index] == 0;
        let actual_finish = if progress.status[index] == ProgressStatus::Complete {
            // Anchor directly on the stored actual-finish instant; offsets are
            // ambiguous at day boundaries and are not clamped to schedule start.
            progress.actual_finish_instant[index].expect("complete actual finish")
        } else {
            let constrained_milestone_start = milestone
                && constraints.normalized_start_dates[index].is_some()
                && remaining_start[index] == constraints.start_no_earlier_than[index].max(0);
            if constrained_milestone_start {
                calendar.working_day_start(
                    constraints.normalized_start_dates[index]
                        .expect("constrained milestone start date"),
                )?
            } else {
                calendar.task_finish_instant(early_finish[index], milestone)?
            }
        };
        if actual_finish > calendar.working_day_finish(deadline_date)? {
            directly_violated.push(index);
        }
    }
    let completed = progress
        .status
        .iter()
        .map(|status| *status == ProgressStatus::Complete)
        .collect::<Vec<_>>();
    let critical_path_indices = graph.critical_path(
        &remaining_start,
        &early_start,
        &early_finish,
        &total_float,
        schedule_finish_offset,
        &directly_violated,
        &completed,
    );
    let critical_path = critical_path_indices
        .iter()
        .map(|&index| graph.tasks[index].id.clone())
        .collect();

    let mut offsets = vec![None; input.tasks.len()];
    let mut leaf_of_original = vec![None; input.tasks.len()];
    for (index, &original_index) in graph.original_indices.iter().enumerate() {
        leaf_of_original[original_index] = Some(index);
        let duration = graph.durations[index];
        let completed = progress.status[index] == ProgressStatus::Complete;
        let (weighted_num, positive_duration, milestone_total, milestone_complete) = if duration > 0
        {
            (
                i128::from(duration) * i128::from(progress.percent[index]),
                i128::from(duration),
                0,
                0,
            )
        } else {
            (0, 0, 1, u32::from(completed))
        };
        offsets[original_index] = Some(OffsetTask {
            early_start: early_start[index],
            early_finish: early_finish[index],
            late_start: late_start[index],
            late_finish: late_finish[index],
            total_float_minutes: total_float[index],
            duration_minutes: duration,
            constraint_violated: directly_violated.contains(&index),
            has_incomplete: !completed,
            incomplete_total_float: if completed { 0 } else { total_float[index] },
            percent_complete: progress.percent[index],
            weighted_percent_numerator: weighted_num,
            positive_leaf_duration: positive_duration,
            milestone_leaf_count: milestone_total,
            milestone_complete_count: milestone_complete,
        });
    }
    for index in 0..input.tasks.len() {
        derive_summary(index, &hierarchy, &mut offsets)?;
    }

    let mut instants = vec![None; input.tasks.len()];
    for index in 0..input.tasks.len() {
        derive_task_instants(
            index,
            &hierarchy,
            &graph,
            &constraints,
            &progress,
            &calendar,
            &offsets,
            &mut instants,
        )?;
    }

    // Pure post-pass: one deterministic explanation per task, ordered like
    // `input.tasks`, computed from the arrays the passes already produced.
    let explanations = compute_explanations(
        input,
        &hierarchy,
        &graph,
        &constraints,
        &progress,
        &calendar,
        &instants,
        &leaf_of_original,
        &remaining_start,
        &early_start,
        &early_finish,
        &late_start,
        &late_finish,
        &total_float,
        schedule_finish_offset,
        &entered_snet,
        &entered_fnlt,
    )?;

    let mut tasks = Vec::with_capacity(input.tasks.len());
    for (index, task) in input.tasks.iter().enumerate() {
        let offset = offsets[index].as_ref().expect("validated task calculation");
        let task_instants = instants[index].as_ref().expect("validated task instants");
        let summary = !hierarchy.children[index].is_empty();
        let zero_span = offset.duration_minutes == 0;
        let (actual_start, actual_finish) = match (summary, leaf_of_original[index]) {
            (false, Some(leaf_index)) => (
                progress.actual_start_instant[leaf_index],
                progress.actual_finish_instant[leaf_index],
            ),
            _ => (None, None),
        };
        tasks.push(ScheduledTask {
            id: task.id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            duration_minutes: offset.duration_minutes,
            early_start: task_instants.early_start,
            early_finish: task_instants.early_finish,
            late_start: task_instants.late_start,
            late_finish: task_instants.late_finish,
            total_float_minutes: offset.total_float_minutes,
            critical: offset.has_incomplete && offset.total_float_minutes <= 0,
            constraint_violated: offset.constraint_violated,
            milestone: !summary && zero_span,
            summary,
            percent_complete: offset.percent_complete,
            actual_start,
            actual_finish,
        });
    }

    let schedule_finish = tasks
        .iter()
        .map(|task| task.early_finish)
        .max()
        .map(Ok)
        .unwrap_or_else(|| calendar.start_instant(0))?;
    Ok(ScheduleResult {
        schedule_start: calendar.start_instant(0)?,
        schedule_finish,
        critical_task_ids: {
            let mut ids = tasks
                .iter()
                .filter(|task| task.critical)
                .map(|task| task.id.clone())
                .collect::<Vec<_>>();
            ids.sort();
            ids
        },
        critical_path,
        directly_violated_leaf_task_ids: {
            let mut ids = directly_violated
                .iter()
                .map(|&index| graph.tasks[index].id.clone())
                .collect::<Vec<_>>();
            ids.sort();
            ids
        },
        data_date: progress.data_date_instant,
        tasks,
        explanations,
    })
}

// Builds one deterministic explanation per task, ordered like `input.tasks`.
// It mirrors the forward/backward binding math exactly and introduces no new
// scheduling semantics. The wide argument list threads the already-computed
// arrays positionally, keeping the binding logic next to the passes.
#[allow(clippy::too_many_arguments)]
fn compute_explanations(
    input: &ScheduleInput,
    hierarchy: &TaskHierarchy<'_>,
    graph: &LeafGraph<'_>,
    constraints: &ConstraintSet,
    progress: &ProgressSet,
    calendar: &CalendarMath,
    instants: &[Option<TaskInstants>],
    leaf_of_original: &[Option<usize>],
    remaining_start: &[i64],
    early_start: &[i64],
    early_finish: &[i64],
    late_start: &[i64],
    late_finish: &[i64],
    total_float: &[i64],
    // The project-finish anchor; the residual `projectFinish` late-finish limit.
    schedule_finish_offset: i64,
    entered_snet: &HashMap<&str, NaiveDate>,
    entered_fnlt: &HashMap<&str, NaiveDate>,
) -> Result<Vec<TaskExplanation>, ScheduleError> {
    let mut leaf_by_id: HashMap<&str, usize> = HashMap::new();
    for (index, task) in graph.tasks.iter().enumerate() {
        leaf_by_id.insert(task.id.as_str(), index);
    }

    let mut explanations = Vec::with_capacity(input.tasks.len());
    for (original_index, task) in input.tasks.iter().enumerate() {
        if hierarchy.is_summary(original_index) {
            explanations.push(TaskExplanation::Summary {
                task_id: task.id.clone(),
            });
            continue;
        }
        let li = leaf_of_original[original_index].expect("validated leaf index");
        if progress.status[li] == ProgressStatus::Complete {
            explanations.push(TaskExplanation::Complete {
                task_id: task.id.clone(),
                actual_start: progress.actual_start_instant[li].expect("complete actual start"),
                actual_finish: progress.actual_finish_instant[li].expect("complete actual finish"),
            });
            continue;
        }

        let in_progress = progress.status[li] == ProgressStatus::InProgress;
        let rem_start = remaining_start[li];

        // Binding facts, pushed in precedence order: predecessor, startConstraint,
        // dataDate, scheduleStart. binding[0] is therefore the primary driver.
        let mut binding: Vec<ScheduleDriver> = Vec::new();
        // Whether a complete-predecessor data-date lift raised a binding edge; a
        // contributing dataDate fact is reported once when so.
        let mut data_date_binds = false;

        // Predecessors are pre-sorted lexical id then FS, SS, FF, SF.
        for edge in &graph.predecessors[li] {
            let p = edge.other;
            let anchor = match edge.dep_type {
                DependencyType::FinishStart | DependencyType::FinishFinish => early_finish[p],
                DependencyType::StartStart | DependencyType::StartFinish => early_start[p],
            };
            let raw = anchor
                .checked_add(edge.lag)
                .ok_or(ScheduleError::ScheduleOutOfRange)?;
            let mut contribution = raw;
            let mut lifted = false;
            if progress.status[p] == ProgressStatus::Complete {
                if let Some(data_date) = progress.data_date_offset {
                    if data_date > raw {
                        lifted = true;
                    }
                    contribution = contribution.max(data_date);
                }
            }
            let start_bound = match edge.dep_type {
                DependencyType::FinishStart | DependencyType::StartStart => contribution,
                DependencyType::FinishFinish | DependencyType::StartFinish => contribution
                    .checked_sub(progress.remaining_duration[li])
                    .ok_or(ScheduleError::ScheduleOutOfRange)?,
            };
            if start_bound == rem_start {
                binding.push(ScheduleDriver::Predecessor {
                    task_id: graph.tasks[p].id.clone(),
                    dependency_type: edge.dep_type,
                    lag_minutes: edge.lag,
                });
                if lifted {
                    data_date_binds = true;
                }
            }
        }

        // startConstraint: the un-floored normalized SNET offset equals the start.
        if constraints.normalized_start_dates[li].is_some()
            && constraints.start_no_earlier_than[li] == rem_start
        {
            binding.push(ScheduleDriver::StartConstraint {
                date: entered_snet
                    .get(task.id.as_str())
                    .copied()
                    .expect("entered SNET date"),
                normalized_date: constraints.normalized_start_dates[li].expect("normalized SNET"),
            });
        }

        // dataDate: the data-date instant pushes incomplete work, either directly
        // or via the complete-predecessor lift captured above. Reported once.
        if let Some(data_date) = progress.data_date_offset {
            if data_date == rem_start {
                data_date_binds = true;
            }
        }
        if data_date_binds {
            binding.push(ScheduleDriver::DataDate {
                date: progress.data_date_instant.expect("data-date instant"),
            });
        }

        // scheduleStart: the floor, always a candidate; binds at offset zero.
        if rem_start == 0 {
            binding.push(ScheduleDriver::ScheduleStart {});
        }

        let mut drivers = binding.into_iter();
        let primary_driver = drivers.next().expect("every scheduled leaf binds a fact");
        let other_binding_drivers: Vec<ScheduleDriver> = drivers.collect();

        let started_actual_start = if in_progress {
            Some(progress.actual_start_instant[li].expect("in-progress actual start"))
        } else {
            None
        };

        // Calendar gap between the primary driver's reference date and the leaf's
        // arrival date. Every reference is a reported/normalized civil date: the
        // predecessor's driving-anchor instant, the entered SNET date, the
        // normalized data-date instant, or the normalized schedule-start date.
        let reference_date: NaiveDate = match &primary_driver {
            ScheduleDriver::Predecessor {
                task_id,
                dependency_type,
                ..
            } => {
                let pred_original = graph.original_indices[leaf_by_id[task_id.as_str()]];
                let pred_instants = instants[pred_original]
                    .as_ref()
                    .expect("predecessor instants");
                match dependency_type {
                    DependencyType::FinishStart | DependencyType::FinishFinish => {
                        pred_instants.early_finish.date()
                    }
                    DependencyType::StartStart | DependencyType::StartFinish => {
                        pred_instants.early_start.date()
                    }
                }
            }
            ScheduleDriver::StartConstraint { date, .. } => *date,
            ScheduleDriver::DataDate { date } => date.date(),
            ScheduleDriver::ScheduleStart {} => calendar.first_working_date,
        };
        // The arrival primitive is chosen by the primary reference's anchor kind,
        // not by FF/SF. A finish-anchored FF reference measures to the leaf's
        // reported early-finish civil date. A finish-anchored FS reference keeps
        // the reported early-start date when not started (a milestone's event
        // instant aligns with the predecessor finish, preserving no-gap) and the
        // remaining-work start when in progress. A start-anchored SF reference
        // bounds the leaf's finish, so it converts the remaining-work finish
        // offset with the start-instant primitive — never the leaf's start,
        // which would sweep the leaf's own duration into the gap. The remaining
        // start-anchored references (SS, startConstraint, dataDate,
        // scheduleStart) use the remaining-work start converted the same way,
        // for both started and not-started leaves; that lands a milestone on
        // its actual working-day start instead of the prior-day event instant,
        // so undisplaced hand-offs report no gap.
        let leaf_instants = instants[original_index].as_ref().expect("leaf instants");
        let arrival_date: NaiveDate = match &primary_driver {
            ScheduleDriver::Predecessor {
                dependency_type: DependencyType::FinishFinish,
                ..
            } => leaf_instants.early_finish.date(),
            ScheduleDriver::Predecessor {
                dependency_type: DependencyType::FinishStart,
                ..
            } if !in_progress => leaf_instants.early_start.date(),
            ScheduleDriver::Predecessor {
                dependency_type: DependencyType::StartFinish,
                ..
            } => {
                let finish_offset = rem_start
                    .checked_add(progress.remaining_duration[li])
                    .ok_or(ScheduleError::ScheduleOutOfRange)?;
                calendar.start_instant(finish_offset)?.date()
            }
            _ => calendar.start_instant(rem_start)?.date(),
        };
        let (from_date, to_date) = if reference_date <= arrival_date {
            (reference_date, arrival_date)
        } else {
            (arrival_date, reference_date)
        };
        // Bounded scan: endpoints are civil dates within the 2000-2100 calendar
        // window, so the loop runs at most ~36.5k iterations in the worst case.
        let mut non_working_day_count = 0_i64;
        let mut cursor = from_date;
        while cursor < to_date {
            if !is_working_date(cursor, &calendar.working_weekdays, &calendar.exceptions) {
                non_working_day_count += 1;
            }
            cursor = cursor.succ_opt().ok_or(ScheduleError::ScheduleOutOfRange)?;
        }
        let calendar_gap = (non_working_day_count > 0).then_some(CalendarGap {
            from_date,
            to_date,
            non_working_day_count,
        });

        // Late-finish limit: recompute the leaf's own backward-pass bounds and
        // name the highest-precedence binding one (deadline > successor > project).
        let leaf_late_finish = late_finish[li];
        let late_finish_limit = if constraints.normalized_finish_dates[li].is_some()
            && constraints.finish_no_later_than[li] == leaf_late_finish
        {
            LateFinishLimit::Deadline {
                date: entered_fnlt
                    .get(task.id.as_str())
                    .copied()
                    .expect("entered FNLT date"),
                normalized_date: constraints.normalized_finish_dates[li].expect("normalized FNLT"),
            }
        } else {
            let remaining = progress.remaining_duration[li];
            let mut successor_limit: Option<LateFinishLimit> = None;
            for edge in &graph.successors[li] {
                if progress.status[edge.other] == ProgressStatus::Complete {
                    continue;
                }
                if in_progress
                    && matches!(
                        edge.dep_type,
                        DependencyType::StartStart | DependencyType::StartFinish
                    )
                {
                    continue;
                }
                let bound = match edge.dep_type {
                    DependencyType::FinishStart => late_start[edge.other]
                        .checked_sub(edge.lag)
                        .ok_or(ScheduleError::ScheduleOutOfRange)?,
                    DependencyType::StartStart => late_start[edge.other]
                        .checked_sub(edge.lag)
                        .and_then(|value| value.checked_add(remaining))
                        .ok_or(ScheduleError::ScheduleOutOfRange)?,
                    DependencyType::FinishFinish => late_finish[edge.other]
                        .checked_sub(edge.lag)
                        .ok_or(ScheduleError::ScheduleOutOfRange)?,
                    DependencyType::StartFinish => late_finish[edge.other]
                        .checked_sub(edge.lag)
                        .and_then(|value| value.checked_add(remaining))
                        .ok_or(ScheduleError::ScheduleOutOfRange)?,
                };
                if bound == leaf_late_finish {
                    successor_limit = Some(LateFinishLimit::Successor {
                        task_id: graph.tasks[edge.other].id.clone(),
                        dependency_type: edge.dep_type,
                        lag_minutes: edge.lag,
                    });
                    break;
                }
            }
            match successor_limit {
                Some(limit) => limit,
                None => {
                    // Residual: no deadline and no successor lowered the late
                    // finish, so it still sits at the project-finish anchor.
                    debug_assert_eq!(leaf_late_finish, schedule_finish_offset);
                    LateFinishLimit::ProjectFinish {}
                }
            }
        };

        explanations.push(TaskExplanation::Scheduled {
            task_id: task.id.clone(),
            primary_driver,
            other_binding_drivers,
            started_actual_start,
            calendar_gap,
            total_float_minutes: total_float[li],
            critical: total_float[li] <= 0,
            late_finish_limit,
        });
    }
    Ok(explanations)
}

#[derive(Clone)]
struct OffsetTask {
    early_start: i64,
    early_finish: i64,
    late_start: i64,
    late_finish: i64,
    total_float_minutes: i64,
    duration_minutes: i64,
    constraint_violated: bool,
    percent_complete: u8,
    // True when this task or any descendant leaf is not complete. Float and
    // criticality are derived from incomplete descendants only.
    has_incomplete: bool,
    // Minimum total float across incomplete descendant leaves; 0 when none.
    incomplete_total_float: i64,
    // Duration-weighted percent rollup accumulators over leaf descendants.
    weighted_percent_numerator: i128,
    positive_leaf_duration: i128,
    milestone_leaf_count: u32,
    milestone_complete_count: u32,
}

#[derive(Clone)]
struct TaskInstants {
    early_start: NaiveDateTime,
    early_finish: NaiveDateTime,
    late_start: NaiveDateTime,
    late_finish: NaiveDateTime,
}

struct TaskHierarchy<'a> {
    tasks: &'a [ScheduleTask],
    task_indices: HashMap<&'a str, usize>,
    children: Vec<Vec<usize>>,
    leaf_indices: Vec<usize>,
}

impl<'a> TaskHierarchy<'a> {
    fn new(tasks: &'a [ScheduleTask]) -> Result<Self, ScheduleError> {
        let mut task_indices = HashMap::new();
        for (index, task) in tasks.iter().enumerate() {
            if task.id.trim().is_empty() {
                return Err(invalid_task("task_id_required", "task IDs cannot be blank"));
            }
            if task_indices.insert(task.id.as_str(), index).is_some() {
                return Err(invalid_task(
                    "task_id_duplicate",
                    format!("task ID {} appears more than once", task.id),
                ));
            }
        }

        let mut parents = vec![None; tasks.len()];
        let mut children = vec![Vec::new(); tasks.len()];
        for (index, task) in tasks.iter().enumerate() {
            if let Some(parent_id) = task.parent_task_id.as_deref() {
                let parent_index = task_indices.get(parent_id).copied().ok_or_else(|| {
                    invalid_task(
                        "task_parent_missing",
                        format!("parent task {parent_id} does not exist"),
                    )
                })?;
                parents[index] = Some(parent_index);
                children[parent_index].push(index);
            }
        }
        validate_hierarchy(&parents, tasks)?;

        let mut leaf_indices = Vec::new();
        for (index, task) in tasks.iter().enumerate() {
            if children[index].is_empty() {
                let duration = task.duration_minutes.ok_or_else(|| {
                    invalid_task(
                        "summary_without_children",
                        format!("summary task {} must have at least one child", task.id),
                    )
                })?;
                if duration < 0 {
                    return Err(invalid_task(
                        "task_duration_negative",
                        format!("task {} has a negative duration", task.id),
                    ));
                }
                leaf_indices.push(index);
            } else if task.duration_minutes.is_some() {
                return Err(invalid_task(
                    "summary_duration_not_derived",
                    format!("summary task {} cannot have an entered duration", task.id),
                ));
            }
        }
        Ok(Self {
            tasks,
            task_indices,
            children,
            leaf_indices,
        })
    }

    fn is_summary(&self, index: usize) -> bool {
        !self.children[index].is_empty()
    }
}

fn validate_hierarchy(
    parents: &[Option<usize>],
    tasks: &[ScheduleTask],
) -> Result<(), ScheduleError> {
    let mut states = vec![0_u8; tasks.len()];
    let mut depths = vec![0_usize; tasks.len()];
    for start in 0..tasks.len() {
        if states[start] == 2 {
            continue;
        }
        let mut current = start;
        let mut path = Vec::new();
        let base_depth = loop {
            match states[current] {
                2 => break depths[current],
                1 => {
                    return Err(invalid_task(
                        "task_hierarchy_cycle",
                        format!("task hierarchy contains a cycle at {}", tasks[current].id),
                    ))
                }
                _ => {}
            }
            states[current] = 1;
            path.push(current);
            match parents[current] {
                Some(parent) => current = parent,
                None => break 0,
            }
        };

        let mut depth = base_depth;
        for index in path.into_iter().rev() {
            depth = depth
                .checked_add(1)
                .ok_or(ScheduleError::ScheduleOutOfRange)?;
            if depth > MAX_HIERARCHY_DEPTH {
                return Err(invalid_task(
                    "task_hierarchy_too_deep",
                    format!("task hierarchy cannot exceed {MAX_HIERARCHY_DEPTH} levels"),
                ));
            }
            depths[index] = depth;
            states[index] = 2;
        }
    }
    Ok(())
}

fn derive_summary(
    index: usize,
    hierarchy: &TaskHierarchy<'_>,
    offsets: &mut [Option<OffsetTask>],
) -> Result<OffsetTask, ScheduleError> {
    if let Some(calculated) = &offsets[index] {
        return Ok(calculated.clone());
    }
    let mut calculated_children = Vec::new();
    for &child in &hierarchy.children[index] {
        calculated_children.push(derive_summary(child, hierarchy, offsets)?);
    }
    let early_start = calculated_children
        .iter()
        .map(|task| task.early_start)
        .min()
        .ok_or_else(|| {
            invalid_task(
                "summary_without_children",
                format!(
                    "summary task {} must have children",
                    hierarchy.tasks[index].id
                ),
            )
        })?;
    let early_finish = calculated_children
        .iter()
        .map(|task| task.early_finish)
        .max()
        .expect("summary has children");
    let weighted_percent_numerator = calculated_children
        .iter()
        .map(|task| task.weighted_percent_numerator)
        .sum();
    let positive_leaf_duration: i128 = calculated_children
        .iter()
        .map(|task| task.positive_leaf_duration)
        .sum();
    let milestone_leaf_count = calculated_children
        .iter()
        .map(|task| task.milestone_leaf_count)
        .sum();
    let milestone_complete_count = calculated_children
        .iter()
        .map(|task| task.milestone_complete_count)
        .sum();
    // Duration-weighted percent over positive-duration leaves; fall back to
    // all-milestone completeness when no positive-duration leaf exists.
    let percent_complete = if positive_leaf_duration > 0 {
        (weighted_percent_numerator / positive_leaf_duration) as u8
    } else if milestone_leaf_count > 0 && milestone_complete_count == milestone_leaf_count {
        100
    } else {
        0
    };
    // Float and criticality derive from incomplete descendants only. A summary
    // whose descendants are all complete reports zero float and is not critical.
    let has_incomplete = calculated_children.iter().any(|task| task.has_incomplete);
    let incomplete_total_float = calculated_children
        .iter()
        .filter(|task| task.has_incomplete)
        .map(|task| task.incomplete_total_float)
        .min()
        .unwrap_or(0);
    let total_float_minutes = if has_incomplete {
        incomplete_total_float
    } else {
        0
    };
    let calculated = OffsetTask {
        early_start,
        early_finish,
        late_start: calculated_children
            .iter()
            .map(|task| task.late_start)
            .min()
            .expect("summary has children"),
        late_finish: calculated_children
            .iter()
            .map(|task| task.late_finish)
            .max()
            .expect("summary has children"),
        total_float_minutes,
        duration_minutes: early_finish
            .checked_sub(early_start)
            .ok_or(ScheduleError::ScheduleOutOfRange)?,
        constraint_violated: calculated_children
            .iter()
            .any(|task| task.constraint_violated),
        has_incomplete,
        incomplete_total_float,
        percent_complete,
        weighted_percent_numerator,
        positive_leaf_duration,
        milestone_leaf_count,
        milestone_complete_count,
    };
    offsets[index] = Some(calculated.clone());
    Ok(calculated)
}

// Recursively resolves civil instants; the wide argument list threads the
// validated leaf context without hiding it behind a bundle struct.
#[allow(clippy::too_many_arguments)]
fn derive_task_instants(
    index: usize,
    hierarchy: &TaskHierarchy<'_>,
    graph: &LeafGraph<'_>,
    constraints: &ConstraintSet,
    progress: &ProgressSet,
    calendar: &CalendarMath,
    offsets: &[Option<OffsetTask>],
    instants: &mut [Option<TaskInstants>],
) -> Result<TaskInstants, ScheduleError> {
    if let Some(calculated) = &instants[index] {
        return Ok(calculated.clone());
    }
    let offset = offsets[index].as_ref().expect("validated task calculation");
    let calculated = if hierarchy.children[index].is_empty() {
        let leaf_index = graph
            .original_indices
            .iter()
            .position(|&original_index| original_index == index)
            .expect("validated leaf index");
        let milestone = offset.duration_minutes == 0;
        match progress.status[leaf_index] {
            ProgressStatus::Complete => {
                // Anchor early and late instants on the stored actual instants
                // rather than re-deriving from ambiguous, unclamped offsets.
                let early_finish =
                    progress.actual_finish_instant[leaf_index].expect("complete actual finish");
                let early_start = if milestone {
                    early_finish
                } else {
                    progress.actual_start_instant[leaf_index].expect("complete actual start")
                };
                TaskInstants {
                    early_start,
                    early_finish,
                    late_start: early_start,
                    late_finish: early_finish,
                }
            }
            ProgressStatus::InProgress => TaskInstants {
                early_start: calendar.start_instant(offset.early_start)?,
                early_finish: calendar.task_finish_instant(offset.early_finish, false)?,
                late_start: calendar.task_start_instant(offset.late_start, false)?,
                late_finish: calendar.task_finish_instant(offset.late_finish, false)?,
            },
            ProgressStatus::NotStarted => {
                let constrained_milestone_start = milestone
                    && constraints.normalized_start_dates[leaf_index].is_some()
                    && offset.early_start == constraints.start_no_earlier_than[leaf_index].max(0);
                let (early_start, early_finish) = if constrained_milestone_start {
                    let instant = calendar.working_day_start(
                        constraints.normalized_start_dates[leaf_index]
                            .expect("constrained milestone start date"),
                    )?;
                    (instant, instant)
                } else {
                    (
                        calendar.task_start_instant(offset.early_start, milestone)?,
                        calendar.task_finish_instant(offset.early_finish, milestone)?,
                    )
                };
                let constrained_milestone_finish = milestone
                    && constraints.normalized_finish_dates[leaf_index].is_some()
                    && offset.late_finish == constraints.finish_no_later_than[leaf_index];
                let (late_start, late_finish) = if constrained_milestone_finish {
                    let instant = calendar.working_day_finish(
                        constraints.normalized_finish_dates[leaf_index]
                            .expect("constrained milestone finish date"),
                    )?;
                    (instant, instant)
                } else {
                    (
                        calendar.task_start_instant(offset.late_start, milestone)?,
                        calendar.task_finish_instant(offset.late_finish, milestone)?,
                    )
                };
                TaskInstants {
                    early_start,
                    early_finish,
                    late_start,
                    late_finish,
                }
            }
        }
    } else {
        let calculated_children = hierarchy.children[index]
            .iter()
            .map(|&child| {
                derive_task_instants(
                    child,
                    hierarchy,
                    graph,
                    constraints,
                    progress,
                    calendar,
                    offsets,
                    instants,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        TaskInstants {
            early_start: calculated_children
                .iter()
                .map(|task| task.early_start)
                .min()
                .expect("summary has children"),
            early_finish: calculated_children
                .iter()
                .map(|task| task.early_finish)
                .max()
                .expect("summary has children"),
            late_start: calculated_children
                .iter()
                .map(|task| task.late_start)
                .min()
                .expect("summary has children"),
            late_finish: calculated_children
                .iter()
                .map(|task| task.late_finish)
                .max()
                .expect("summary has children"),
        }
    };
    instants[index] = Some(calculated.clone());
    Ok(calculated)
}

struct ConstraintSet {
    start_no_earlier_than: Vec<i64>,
    finish_no_later_than: Vec<i64>,
    normalized_start_dates: Vec<Option<NaiveDate>>,
    normalized_finish_dates: Vec<Option<NaiveDate>>,
}

impl ConstraintSet {
    fn new(
        constraints: &[TaskConstraint],
        hierarchy: &TaskHierarchy<'_>,
        graph: &LeafGraph<'_>,
        calendar: &CalendarMath,
    ) -> Result<Self, ScheduleError> {
        let mut leaf_indices = HashMap::new();
        for (index, task) in graph.tasks.iter().enumerate() {
            leaf_indices.insert(task.id.as_str(), index);
        }
        let mut start_no_earlier_than = vec![0; graph.tasks.len()];
        let mut finish_no_later_than = vec![i64::MAX; graph.tasks.len()];
        let mut normalized_start_dates = vec![None; graph.tasks.len()];
        let mut normalized_finish_dates = vec![None; graph.tasks.len()];
        let mut seen = HashSet::new();
        for constraint in constraints {
            if constraint.task_id.trim().is_empty() {
                return Err(invalid_task(
                    "task_constraint_id_required",
                    "task constraints require a task ID",
                ));
            }
            if !seen.insert(constraint.task_id.as_str()) {
                return Err(invalid_task(
                    "task_constraint_duplicate",
                    format!("task {} has more than one constraint", constraint.task_id),
                ));
            }
            let original_index = hierarchy
                .task_indices
                .get(constraint.task_id.as_str())
                .copied()
                .ok_or_else(|| {
                    invalid_task(
                        "task_constraint_task_missing",
                        format!("constrained task {} does not exist", constraint.task_id),
                    )
                })?;
            if hierarchy.is_summary(original_index) {
                return Err(invalid_task(
                    "task_constraint_summary",
                    format!(
                        "summary task {} cannot have a constraint",
                        constraint.task_id
                    ),
                ));
            }
            if constraint.start_no_earlier_than.is_none()
                && constraint.finish_no_later_than.is_none()
            {
                return Err(invalid_task(
                    "task_constraint_empty",
                    format!("task {} has an empty constraint", constraint.task_id),
                ));
            }
            let index = leaf_indices[constraint.task_id.as_str()];
            if let Some(date) = constraint.start_no_earlier_than {
                let normalized = calendar
                    .working_date_on_or_after(date)?
                    .max(calendar.first_working_date);
                start_no_earlier_than[index] =
                    calendar.start_offset_for_working_date(normalized)?;
                normalized_start_dates[index] = Some(normalized);
            }
            if let Some(date) = constraint.finish_no_later_than {
                let normalized = calendar.working_date_on_or_before(date)?;
                finish_no_later_than[index] =
                    calendar.finish_offset_for_working_date(normalized)?;
                normalized_finish_dates[index] = Some(normalized);
            }
        }
        Ok(Self {
            start_no_earlier_than,
            finish_no_later_than,
            normalized_start_dates,
            normalized_finish_dates,
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProgressStatus {
    NotStarted,
    InProgress,
    Complete,
}

/// Validated and normalized per-leaf progress plus the job data-date instant.
struct ProgressSet {
    data_date_offset: Option<i64>,
    data_date_instant: Option<NaiveDateTime>,
    status: Vec<ProgressStatus>,
    percent: Vec<u8>,
    remaining_duration: Vec<i64>,
    actual_start_offset: Vec<Option<i64>>,
    actual_finish_offset: Vec<Option<i64>>,
    actual_start_instant: Vec<Option<NaiveDateTime>>,
    actual_finish_instant: Vec<Option<NaiveDateTime>>,
}

impl ProgressSet {
    fn new(
        progress: &ScheduleProgress,
        hierarchy: &TaskHierarchy<'_>,
        graph: &LeafGraph<'_>,
        calendar: &CalendarMath,
    ) -> Result<Self, ScheduleError> {
        let leaf_count = graph.tasks.len();
        let mut leaf_indices = HashMap::new();
        for (index, task) in graph.tasks.iter().enumerate() {
            leaf_indices.insert(task.id.as_str(), index);
        }

        // Progress entries require an explicit data date; a data date with no
        // entries is a valid empty status update.
        let data_date = progress.data_date;
        if !progress.entries.is_empty() && data_date.is_none() {
            return Err(invalid_progress(
                "progress_data_date_required",
                "progress entries require a job data date",
            ));
        }
        let (data_date_offset, data_date_instant) = match data_date {
            Some(date) => {
                let normalized = calendar
                    .working_date_on_or_after(date)?
                    .max(calendar.first_working_date);
                (
                    Some(calendar.start_offset_for_working_date(normalized)?),
                    Some(calendar.working_day_start(normalized)?),
                )
            }
            None => (None, None),
        };

        let mut status = vec![ProgressStatus::NotStarted; leaf_count];
        let mut percent = vec![0_u8; leaf_count];
        let mut actual_start_offset = vec![None; leaf_count];
        let mut actual_finish_offset = vec![None; leaf_count];
        let mut actual_start_instant = vec![None; leaf_count];
        let mut actual_finish_instant = vec![None; leaf_count];
        let mut seen = HashSet::new();

        for entry in &progress.entries {
            if entry.task_id.trim().is_empty() {
                return Err(invalid_progress(
                    "progress_task_id_required",
                    "progress entries require a task ID",
                ));
            }
            if !seen.insert(entry.task_id.as_str()) {
                return Err(invalid_progress(
                    "progress_duplicate",
                    format!("task {} has more than one progress entry", entry.task_id),
                ));
            }
            let original_index = hierarchy
                .task_indices
                .get(entry.task_id.as_str())
                .copied()
                .ok_or_else(|| {
                    invalid_progress(
                        "progress_task_missing",
                        format!("progress task {} does not exist", entry.task_id),
                    )
                })?;
            if hierarchy.is_summary(original_index) {
                return Err(invalid_progress(
                    "progress_summary",
                    format!("summary task {} cannot report progress", entry.task_id),
                ));
            }
            if entry.percent_complete > 100 {
                return Err(invalid_progress(
                    "progress_percent_out_of_range",
                    format!("task {} percent complete exceeds 100", entry.task_id),
                ));
            }
            let index = leaf_indices[entry.task_id.as_str()];
            let milestone = graph.durations[index] == 0;
            let data_date = data_date.expect("entries validated against a data date");

            // Percent/actual-date combination rules.
            match entry.percent_complete {
                0 => {
                    if entry.actual_start.is_some() || entry.actual_finish.is_some() {
                        return Err(invalid_progress(
                            "progress_actuals_forbidden",
                            format!("task {} at 0 percent cannot carry actuals", entry.task_id),
                        ));
                    }
                    status[index] = ProgressStatus::NotStarted;
                }
                100 => {
                    if entry.actual_start.is_none() {
                        return Err(invalid_progress(
                            "progress_actual_start_required",
                            format!("complete task {} requires an actual start", entry.task_id),
                        ));
                    }
                    if entry.actual_finish.is_none() {
                        return Err(invalid_progress(
                            "progress_actual_finish_required",
                            format!("complete task {} requires an actual finish", entry.task_id),
                        ));
                    }
                    status[index] = ProgressStatus::Complete;
                }
                _ => {
                    if milestone {
                        return Err(invalid_progress(
                            "progress_milestone_percent",
                            format!("milestone {} accepts only 0 or 100 percent", entry.task_id),
                        ));
                    }
                    if entry.actual_start.is_none() {
                        return Err(invalid_progress(
                            "progress_actual_start_required",
                            format!(
                                "in-progress task {} requires an actual start",
                                entry.task_id
                            ),
                        ));
                    }
                    if entry.actual_finish.is_some() {
                        return Err(invalid_progress(
                            "progress_actual_finish_forbidden",
                            format!(
                                "in-progress task {} cannot carry an actual finish",
                                entry.task_id
                            ),
                        ));
                    }
                    status[index] = ProgressStatus::InProgress;
                }
            }

            // Civil-date ordering: actuals precede one another and the data date.
            if let (Some(start), Some(finish)) = (entry.actual_start, entry.actual_finish) {
                if start > finish {
                    return Err(invalid_progress(
                        "progress_actual_order",
                        format!(
                            "task {} actual start is after its actual finish",
                            entry.task_id
                        ),
                    ));
                }
                if milestone && start != finish {
                    return Err(invalid_progress(
                        "progress_milestone_actuals_unequal",
                        format!(
                            "milestone {} requires equal actual start and finish dates",
                            entry.task_id
                        ),
                    ));
                }
            }
            for actual in [entry.actual_start, entry.actual_finish]
                .into_iter()
                .flatten()
            {
                if actual > data_date {
                    return Err(invalid_progress(
                        "progress_actual_after_data_date",
                        format!(
                            "task {} reports an actual after the data date",
                            entry.task_id
                        ),
                    ));
                }
            }

            // Normalize actuals onto working-day boundaries.
            percent[index] = entry.percent_complete;
            if let Some(start) = entry.actual_start {
                let normalized = calendar.working_date_on_or_after(start)?;
                actual_start_offset[index] =
                    Some(calendar.start_offset_for_working_date(normalized)?);
                actual_start_instant[index] = Some(calendar.working_day_start(normalized)?);
            }
            if let Some(finish) = entry.actual_finish {
                let normalized = calendar.working_date_on_or_before(finish)?;
                actual_finish_offset[index] =
                    Some(calendar.finish_offset_for_working_date(normalized)?);
                actual_finish_instant[index] = Some(calendar.working_day_finish(normalized)?);
            }
            // Compare normalized civil instants: a start that rolls forward past
            // a finish that rolls back (e.g. weekend actuals) is an inversion the
            // raw-date check cannot see.
            if let (Some(start), Some(finish)) =
                (actual_start_instant[index], actual_finish_instant[index])
            {
                if start > finish {
                    return Err(invalid_progress(
                        "progress_normalized_order",
                        format!(
                            "task {} normalizes to an actual start after its finish",
                            entry.task_id
                        ),
                    ));
                }
            }
        }

        // Remaining duration = duration - floor(duration * percent / 100).
        let mut remaining_duration = vec![0_i64; leaf_count];
        for index in 0..leaf_count {
            let duration = graph.durations[index];
            let completed_portion =
                (i128::from(duration) * i128::from(percent[index]) / 100) as i64;
            remaining_duration[index] = duration - completed_portion;
        }

        Ok(Self {
            data_date_offset,
            data_date_instant,
            status,
            percent,
            remaining_duration,
            actual_start_offset,
            actual_finish_offset,
            actual_start_instant,
            actual_finish_instant,
        })
    }
}

/// One directed dependency edge in the leaf graph. `other` is the leaf index at
/// the far end (successor in a successor list, predecessor in a predecessor
/// list); `dep_type` and `lag` carry the typed, signed relationship.
#[derive(Clone, Copy)]
struct Edge {
    other: usize,
    dep_type: DependencyType,
    lag: i64,
}

struct LeafGraph<'a> {
    tasks: Vec<&'a ScheduleTask>,
    original_indices: Vec<usize>,
    durations: Vec<i64>,
    predecessors: Vec<Vec<Edge>>,
    successors: Vec<Vec<Edge>>,
    topological_order: Vec<usize>,
}

impl<'a> LeafGraph<'a> {
    fn new(
        hierarchy: &'a TaskHierarchy<'a>,
        dependencies: &[FinishStartDependency],
    ) -> Result<Self, ScheduleError> {
        let mut task_indices = HashMap::new();
        let mut leaf_tasks = Vec::new();
        let mut original_indices = Vec::new();
        let mut durations = Vec::new();
        for &original_index in &hierarchy.leaf_indices {
            let task = &hierarchy.tasks[original_index];
            let duration = task.duration_minutes.expect("validated leaf duration");
            let index = leaf_tasks.len();
            task_indices.insert(task.id.as_str(), index);
            leaf_tasks.push(task);
            original_indices.push(original_index);
            durations.push(duration);
        }

        let mut predecessors = vec![Vec::new(); leaf_tasks.len()];
        let mut successors = vec![Vec::new(); leaf_tasks.len()];
        let mut seen = HashSet::new();
        for dependency in dependencies {
            // Signed lag is legal on every type; the forward pass clamps floored
            // dates at schedule start rather than rejecting negative lag.
            let predecessor_original = hierarchy
                .task_indices
                .get(dependency.predecessor_task_id.as_str())
                .copied()
                .ok_or_else(|| {
                    invalid_dependency(
                        "dependency_task_missing",
                        format!(
                            "predecessor task {} does not exist",
                            dependency.predecessor_task_id
                        ),
                    )
                })?;
            let successor_original = hierarchy
                .task_indices
                .get(dependency.successor_task_id.as_str())
                .copied()
                .ok_or_else(|| {
                    invalid_dependency(
                        "dependency_task_missing",
                        format!(
                            "successor task {} does not exist",
                            dependency.successor_task_id
                        ),
                    )
                })?;
            if hierarchy.is_summary(predecessor_original)
                || hierarchy.is_summary(successor_original)
            {
                return Err(invalid_dependency(
                    "dependency_summary_endpoint",
                    "summary tasks cannot be dependency endpoints",
                ));
            }
            let predecessor = task_indices[dependency.predecessor_task_id.as_str()];
            let successor = task_indices[dependency.successor_task_id.as_str()];
            if predecessor == successor {
                return Err(invalid_dependency(
                    "dependency_self_link",
                    "a task cannot depend on itself",
                ));
            }
            // Identity includes the type, so different-type links between the
            // same pair are legal while an exact repeat is rejected.
            if !seen.insert((predecessor, successor, dependency.dependency_type)) {
                return Err(invalid_dependency(
                    "dependency_duplicate",
                    "that dependency already exists",
                ));
            }
            successors[predecessor].push(Edge {
                other: successor,
                dep_type: dependency.dependency_type,
                lag: dependency.lag_minutes,
            });
            predecessors[successor].push(Edge {
                other: predecessor,
                dep_type: dependency.dependency_type,
                lag: dependency.lag_minutes,
            });
        }
        for edges in successors.iter_mut().chain(predecessors.iter_mut()) {
            edges.sort_unstable_by(|left, right| {
                leaf_tasks[left.other]
                    .id
                    .cmp(&leaf_tasks[right.other].id)
                    .then_with(|| left.dep_type.cmp(&right.dep_type))
            });
        }

        let topological_order = topological_order(&leaf_tasks, &successors, &predecessors)?;
        Ok(Self {
            tasks: leaf_tasks,
            original_indices,
            durations,
            predecessors,
            successors,
            topological_order,
        })
    }

    // The offset arrays are threaded positionally rather than bundled to keep
    // the driving-path math close to the forward/backward passes.
    #[allow(clippy::too_many_arguments)]
    fn critical_path(
        &self,
        remaining_start: &[i64],
        early_start: &[i64],
        early_finish: &[i64],
        total_float: &[i64],
        schedule_finish: i64,
        directly_violated: &[usize],
        completed: &[bool],
    ) -> Vec<usize> {
        // A predecessor drives the current leaf when its type-appropriate anchor
        // plus lag exactly meets the current leaf's type-appropriate target.
        let drives = |edge: &Edge, current: usize| -> bool {
            let anchor = match edge.dep_type {
                DependencyType::FinishStart | DependencyType::FinishFinish => {
                    early_finish[edge.other]
                }
                DependencyType::StartStart | DependencyType::StartFinish => early_start[edge.other],
            };
            let target = match edge.dep_type {
                DependencyType::FinishStart | DependencyType::StartStart => {
                    remaining_start[current]
                }
                DependencyType::FinishFinish | DependencyType::StartFinish => early_finish[current],
            };
            anchor.checked_add(edge.lag) == Some(target)
        };
        // The primary driving path traces incomplete leaves only.
        if directly_violated
            .iter()
            .any(|&index| !completed[index] && total_float[index] < 0)
        {
            let mut current = directly_violated
                .iter()
                .copied()
                .filter(|&index| !completed[index] && total_float[index] < 0)
                .min_by(|&left, &right| {
                    total_float[left]
                        .cmp(&total_float[right])
                        .then_with(|| self.tasks[left].id.cmp(&self.tasks[right].id))
                })
                .expect("non-empty directly violated task list");
            let mut path = vec![current];
            while let Some(predecessor) = self.predecessors[current].iter().find_map(|edge| {
                (!completed[edge.other]
                    && total_float[edge.other] == total_float[current]
                    && drives(edge, current))
                .then_some(edge.other)
            }) {
                path.push(predecessor);
                current = predecessor;
            }
            path.reverse();
            return path;
        }
        // A terminal driving leaf is an incomplete leaf that finishes the project
        // with zero float. Under SS/FF/SF such a leaf can still have incomplete
        // successors, so there is no "successors complete" filter; instead a
        // candidate that drives a deeper candidate is excluded so the deepest
        // wins, then lexical order breaks ties. The drives relation is acyclic,
        // so a sink candidate always exists when any candidate does.
        let is_terminal_candidate = |index: usize| {
            !completed[index] && total_float[index] == 0 && early_finish[index] == schedule_finish
        };
        let Some(mut current) = (0..self.tasks.len())
            .filter(|&index| is_terminal_candidate(index))
            .filter(|&index| {
                !self.successors[index].iter().any(|edge| {
                    is_terminal_candidate(edge.other)
                        && drives(
                            &Edge {
                                other: index,
                                dep_type: edge.dep_type,
                                lag: edge.lag,
                            },
                            edge.other,
                        )
                })
            })
            .min_by(|&left, &right| self.tasks[left].id.cmp(&self.tasks[right].id))
        else {
            return Vec::new();
        };
        let mut path = vec![current];
        while let Some(predecessor) = self.predecessors[current].iter().find_map(|edge| {
            (!completed[edge.other] && total_float[edge.other] == 0 && drives(edge, current))
                .then_some(edge.other)
        }) {
            path.push(predecessor);
            current = predecessor;
        }
        path.reverse();
        path
    }
}

fn topological_order(
    tasks: &[&ScheduleTask],
    successors: &[Vec<Edge>],
    predecessors: &[Vec<Edge>],
) -> Result<Vec<usize>, ScheduleError> {
    // Any directed cycle is rejected regardless of link type: each parallel edge
    // counts toward the in-degree, so the multigraph resolves like a simple one.
    let mut remaining_predecessors = predecessors.iter().map(Vec::len).collect::<Vec<_>>();
    let mut ready = remaining_predecessors
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 0)
        .map(|(index, _)| (tasks[index].id.clone(), index))
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(tasks.len());
    while let Some((_, index)) = ready.pop_first() {
        order.push(index);
        for edge in &successors[index] {
            remaining_predecessors[edge.other] -= 1;
            if remaining_predecessors[edge.other] == 0 {
                ready.insert((tasks[edge.other].id.clone(), edge.other));
            }
        }
    }
    if order.len() != tasks.len() {
        let mut task_ids = remaining_predecessors
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(index, _)| tasks[index].id.clone())
            .collect::<Vec<_>>();
        task_ids.sort();
        return Err(ScheduleError::DependencyCycle { task_ids });
    }
    Ok(order)
}

/// The maximum number of dated exceptions one calendar may carry. Bounds every
/// day scan and every exception-correction loop.
const MAX_CALENDAR_EXCEPTIONS: usize = 4000;

struct CalendarMath {
    working_weekdays: HashSet<Weekday>,
    workday_start_minute: i64,
    workday_duration_minutes: i64,
    first_working_date: NaiveDate,
    /// Every dated non-working exception, for the iterative working-day predicate.
    exceptions: HashSet<NaiveDate>,
    /// Sorted closed-form weekly offsets of the exceptions that fall on a working
    /// weekday at or after `first_working_date`. Used to correct the closed-form
    /// offset math in both directions. Exceptions on non-working weekdays or
    /// before the first working date never shift any working-day offset.
    working_exception_offsets: Vec<i64>,
}

impl CalendarMath {
    fn new(calendar: &WorkingCalendar, schedule_start: NaiveDate) -> Result<Self, ScheduleError> {
        let working_weekdays = calendar
            .working_weekdays
            .iter()
            .copied()
            .map(CalendarWeekday::chrono)
            .collect::<HashSet<_>>();
        if working_weekdays.is_empty() {
            return Err(invalid_calendar(
                "calendar_no_working_days",
                "the calendar must contain at least one working weekday",
            ));
        }
        if calendar.workday_duration_minutes == 0 {
            return Err(invalid_calendar(
                "calendar_empty_workday",
                "the workday duration must be greater than zero",
            ));
        }
        if u32::from(calendar.workday_start_minute) + u32::from(calendar.workday_duration_minutes)
            > 24 * 60
        {
            return Err(invalid_calendar(
                "calendar_workday_out_of_range",
                "the working interval must fit within one local day",
            ));
        }
        // Validate the exception list: bounded count, in-range years, and no
        // duplicates. Canonical date text is handled at the parse boundary.
        if calendar.exceptions.len() > MAX_CALENDAR_EXCEPTIONS {
            return Err(invalid_calendar(
                "calendar_too_many_exceptions",
                "a job may define at most 4000 calendar exceptions",
            ));
        }
        let mut exceptions = HashSet::with_capacity(calendar.exceptions.len());
        for &exception in &calendar.exceptions {
            if !(2000..=2100).contains(&exception.year()) {
                return Err(invalid_calendar(
                    "calendar_exception_out_of_range",
                    "calendar exceptions must fall within years 2000-2100",
                ));
            }
            if !exceptions.insert(exception) {
                return Err(invalid_calendar(
                    "calendar_duplicate_exception",
                    "calendar exceptions must be unique",
                ));
            }
        }
        let first_working_date = next_working_date(schedule_start, &working_weekdays, &exceptions)?;
        let mut math = Self {
            working_weekdays,
            workday_start_minute: i64::from(calendar.workday_start_minute),
            workday_duration_minutes: i64::from(calendar.workday_duration_minutes),
            first_working_date,
            exceptions,
            working_exception_offsets: Vec::new(),
        };
        // Precompute the sorted weekly offsets of every exception that lands on a
        // working weekday, keyed relative to the first working date (offset 0).
        // Pre-start exceptions get negative offsets so backward normalization and
        // negative-offset traversal stay consistent with the forward direction.
        let mut offsets = Vec::new();
        for &exception in &math.exceptions {
            if math.working_weekdays.contains(&exception.weekday()) {
                offsets.push(math.weekly_offset_for_date(exception)?);
            }
        }
        offsets.sort_unstable();
        math.working_exception_offsets = offsets;
        Ok(math)
    }

    fn start_instant(&self, offset_minutes: i64) -> Result<NaiveDateTime, ScheduleError> {
        let working_days = offset_minutes.div_euclid(self.workday_duration_minutes);
        let minute_in_day = offset_minutes.rem_euclid(self.workday_duration_minutes);
        let date = self.working_date_after(working_days)?;
        date.and_hms_opt(0, 0, 0)
            .ok_or(ScheduleError::ScheduleOutOfRange)?
            .checked_add_signed(Duration::minutes(self.workday_start_minute + minute_in_day))
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    fn task_start_instant(
        &self,
        offset_minutes: i64,
        milestone: bool,
    ) -> Result<NaiveDateTime, ScheduleError> {
        if milestone {
            self.event_instant(offset_minutes)
        } else {
            self.start_instant(offset_minutes)
        }
    }

    fn task_finish_instant(
        &self,
        offset_minutes: i64,
        milestone: bool,
    ) -> Result<NaiveDateTime, ScheduleError> {
        if milestone || offset_minutes != 0 {
            self.event_instant(offset_minutes)
        } else {
            let date = self.working_date_after(-1)?;
            date.and_hms_opt(0, 0, 0)
                .ok_or(ScheduleError::ScheduleOutOfRange)?
                .checked_add_signed(Duration::minutes(
                    self.workday_start_minute + self.workday_duration_minutes,
                ))
                .ok_or(ScheduleError::ScheduleOutOfRange)
        }
    }

    fn event_instant(&self, offset_minutes: i64) -> Result<NaiveDateTime, ScheduleError> {
        if offset_minutes == 0 || offset_minutes % self.workday_duration_minutes != 0 {
            return self.start_instant(offset_minutes);
        }
        let prior_working_day = offset_minutes.div_euclid(self.workday_duration_minutes) - 1;
        let date = self.working_date_after(prior_working_day)?;
        date.and_hms_opt(0, 0, 0)
            .ok_or(ScheduleError::ScheduleOutOfRange)?
            .checked_add_signed(Duration::minutes(
                self.workday_start_minute + self.workday_duration_minutes,
            ))
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    /// Maps a 0-based working-day offset to its civil date, honoring exceptions.
    /// `first_working_date` is offset 0. Uses the weekly closed form as a lower
    /// bound then corrects over the bounded exception list (both are exact for a
    /// calendar with no working exceptions, so output is byte-identical there).
    fn working_date_after(&self, working_days: i64) -> Result<NaiveDate, ScheduleError> {
        if self.working_exception_offsets.is_empty() {
            return self.weekly_working_date_after(working_days);
        }
        // The correction list may hold negative offsets (pre-start exceptions), so
        // the number of exceptions in [0, weekly)/[weekly, 0) is measured against
        // the zero boundary.
        let below_zero = self.exception_count_below(0);
        if working_days >= 0 {
            // Each working exception in [0, answer) shifts the answer one weekly
            // slot later; the fixpoint is monotone and bounded by the count.
            let mut preceding = 0_i64;
            loop {
                let weekly_offset = working_days
                    .checked_add(preceding)
                    .ok_or(ScheduleError::ScheduleOutOfRange)?;
                let count = self.exception_count_below(weekly_offset) - below_zero;
                if count == preceding {
                    if self
                        .working_exception_offsets
                        .binary_search(&weekly_offset)
                        .is_ok()
                    {
                        preceding += 1;
                        continue;
                    }
                    return self.weekly_working_date_after(weekly_offset);
                }
                preceding = count;
            }
        } else {
            // Mirror image for dates before the first working date: each pre-start
            // exception in [weekly, 0) pushes the weekly slot one further back.
            let mut trailing = 0_i64;
            loop {
                let weekly_offset = working_days
                    .checked_sub(trailing)
                    .ok_or(ScheduleError::ScheduleOutOfRange)?;
                let count = below_zero - self.exception_count_below(weekly_offset);
                if count == trailing {
                    if self
                        .working_exception_offsets
                        .binary_search(&weekly_offset)
                        .is_ok()
                    {
                        trailing += 1;
                        continue;
                    }
                    return self.weekly_working_date_after(weekly_offset);
                }
                trailing = count;
            }
        }
    }

    /// Count of working-exception offsets strictly less than `bound`.
    fn exception_count_below(&self, bound: i64) -> i64 {
        self.working_exception_offsets
            .partition_point(|&offset| offset < bound) as i64
    }

    fn weekly_working_date_after(&self, working_days: i64) -> Result<NaiveDate, ScheduleError> {
        let days_per_week = i64::try_from(self.working_weekdays.len())
            .map_err(|_| ScheduleError::ScheduleOutOfRange)?;
        let full_weeks = working_days.div_euclid(days_per_week);
        let calendar_days = full_weeks
            .checked_mul(7)
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
        let calendar_delta =
            Duration::try_days(calendar_days).ok_or(ScheduleError::ScheduleOutOfRange)?;
        let mut date = self
            .first_working_date
            .checked_add_signed(calendar_delta)
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
        let rank = working_days.rem_euclid(days_per_week);
        let mut seen = 0;
        loop {
            if self.working_weekdays.contains(&date.weekday()) {
                if seen == rank {
                    return Ok(date);
                }
                seen += 1;
            }
            date = date.succ_opt().ok_or(ScheduleError::ScheduleOutOfRange)?;
        }
    }

    fn working_date_on_or_after(&self, date: NaiveDate) -> Result<NaiveDate, ScheduleError> {
        next_working_date(date, &self.working_weekdays, &self.exceptions)
    }

    fn start_offset_for_working_date(&self, date: NaiveDate) -> Result<i64, ScheduleError> {
        self.working_offset_for_date(date)?
            .checked_mul(self.workday_duration_minutes)
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    fn working_date_on_or_before(&self, date: NaiveDate) -> Result<NaiveDate, ScheduleError> {
        previous_working_date(date, &self.working_weekdays, &self.exceptions)
    }

    fn working_day_start(&self, date: NaiveDate) -> Result<NaiveDateTime, ScheduleError> {
        date.and_hms_opt(0, 0, 0)
            .ok_or(ScheduleError::ScheduleOutOfRange)?
            .checked_add_signed(Duration::minutes(self.workday_start_minute))
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    fn working_day_finish(&self, date: NaiveDate) -> Result<NaiveDateTime, ScheduleError> {
        date.and_hms_opt(0, 0, 0)
            .ok_or(ScheduleError::ScheduleOutOfRange)?
            .checked_add_signed(Duration::minutes(
                self.workday_start_minute + self.workday_duration_minutes,
            ))
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    fn finish_offset_for_working_date(&self, date: NaiveDate) -> Result<i64, ScheduleError> {
        self.working_offset_for_date(date)?
            .checked_add(1)
            .and_then(|days| days.checked_mul(self.workday_duration_minutes))
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    /// Working-day offset of a working date, honoring exceptions. `date` is always
    /// a working date, so no exception equals its offset. For dates at or after
    /// the first working date, subtract the working exceptions in `[0, weekly)`;
    /// for earlier dates, add back the pre-start exceptions in `[weekly, 0)` so
    /// the negative regime agrees with `working_date_after`.
    fn working_offset_for_date(&self, date: NaiveDate) -> Result<i64, ScheduleError> {
        let weekly = self.weekly_offset_for_date(date)?;
        let below_zero = self.exception_count_below(0);
        if weekly >= 0 {
            let skipped = self.exception_count_below(weekly) - below_zero;
            weekly
                .checked_sub(skipped)
                .ok_or(ScheduleError::ScheduleOutOfRange)
        } else {
            let regained = below_zero - self.exception_count_below(weekly);
            weekly
                .checked_add(regained)
                .ok_or(ScheduleError::ScheduleOutOfRange)
        }
    }

    fn weekly_offset_for_date(&self, date: NaiveDate) -> Result<i64, ScheduleError> {
        let first_week_start = self
            .first_working_date
            .checked_sub_signed(Duration::days(i64::from(
                self.first_working_date.weekday().num_days_from_monday(),
            )))
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
        let date_week_start = date
            .checked_sub_signed(Duration::days(i64::from(
                date.weekday().num_days_from_monday(),
            )))
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
        let weeks = date_week_start
            .signed_duration_since(first_week_start)
            .num_days()
            .checked_div(7)
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
        let weekday_rank = (0..date.weekday().num_days_from_monday())
            .filter(|day| self.working_weekdays.contains(&weekday_from_monday(*day)))
            .count();
        let first_rank = (0..self.first_working_date.weekday().num_days_from_monday())
            .filter(|day| self.working_weekdays.contains(&weekday_from_monday(*day)))
            .count();
        weeks
            .checked_mul(
                i64::try_from(self.working_weekdays.len())
                    .map_err(|_| ScheduleError::ScheduleOutOfRange)?,
            )
            .and_then(|offset| offset.checked_add(i64::try_from(weekday_rank).ok()?))
            .and_then(|offset| offset.checked_sub(i64::try_from(first_rank).ok()?))
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }
}

/// The centralized working-day predicate: a working weekday that is not a dated
/// non-working exception.
fn is_working_date(
    date: NaiveDate,
    working_weekdays: &HashSet<Weekday>,
    exceptions: &HashSet<NaiveDate>,
) -> bool {
    working_weekdays.contains(&date.weekday()) && !exceptions.contains(&date)
}

fn next_working_date(
    mut date: NaiveDate,
    working_weekdays: &HashSet<Weekday>,
    exceptions: &HashSet<NaiveDate>,
) -> Result<NaiveDate, ScheduleError> {
    while !is_working_date(date, working_weekdays, exceptions) {
        date = date.succ_opt().ok_or(ScheduleError::ScheduleOutOfRange)?;
    }
    Ok(date)
}

fn previous_working_date(
    mut date: NaiveDate,
    working_weekdays: &HashSet<Weekday>,
    exceptions: &HashSet<NaiveDate>,
) -> Result<NaiveDate, ScheduleError> {
    while !is_working_date(date, working_weekdays, exceptions) {
        date = date.pred_opt().ok_or(ScheduleError::ScheduleOutOfRange)?;
    }
    Ok(date)
}

fn weekday_from_monday(day: u32) -> Weekday {
    match day {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        6 => Weekday::Sun,
        _ => unreachable!("weekday index is bounded by chrono"),
    }
}

fn invalid_calendar(code: &'static str, message: impl Into<String>) -> ScheduleError {
    ScheduleError::InvalidCalendar {
        code,
        message: message.into(),
    }
}

fn invalid_task(code: &'static str, message: impl Into<String>) -> ScheduleError {
    ScheduleError::InvalidTask {
        code,
        message: message.into(),
    }
}

fn invalid_dependency(code: &'static str, message: impl Into<String>) -> ScheduleError {
    ScheduleError::InvalidDependency {
        code,
        message: message.into(),
    }
}

fn invalid_progress(code: &'static str, message: impl Into<String>) -> ScheduleError {
    ScheduleError::InvalidProgress {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod calendar_math_tests {
    use super::*;

    fn weekdays_mon_fri() -> Vec<CalendarWeekday> {
        vec![
            CalendarWeekday::Monday,
            CalendarWeekday::Tuesday,
            CalendarWeekday::Wednesday,
            CalendarWeekday::Thursday,
            CalendarWeekday::Friday,
        ]
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    /// Reference Nth working date by a plain day scan, independent of the
    /// closed-form offset math. Supports negative indices (walk backward).
    fn reference_date(math: &CalendarMath, index: i64) -> NaiveDate {
        let working = |date: NaiveDate| {
            math.working_weekdays.contains(&date.weekday()) && !math.exceptions.contains(&date)
        };
        let mut date = math.first_working_date;
        if index >= 0 {
            let mut seen = 0;
            while seen < index {
                date = date.succ_opt().unwrap();
                if working(date) {
                    seen += 1;
                }
            }
        } else {
            let mut seen = 0;
            while seen > index {
                date = date.pred_opt().unwrap();
                if working(date) {
                    seen -= 1;
                }
            }
        }
        date
    }

    fn calendar(exceptions: &[NaiveDate]) -> WorkingCalendar {
        WorkingCalendar {
            working_weekdays: weekdays_mon_fri(),
            workday_start_minute: 8 * 60,
            workday_duration_minutes: 8 * 60,
            exceptions: exceptions.to_vec(),
        }
    }

    #[test]
    fn offset_math_round_trips_across_positive_and_negative_regimes() {
        // Start Wednesday with exceptions both before and after the first working
        // date, including the two days immediately before the start.
        let start = ymd(2026, 1, 14); // Wednesday
        let exceptions = [
            ymd(2026, 1, 5),  // Monday, pre-start
            ymd(2026, 1, 6),  // Tuesday, pre-start
            ymd(2026, 1, 13), // Tuesday, immediately before start
            ymd(2026, 1, 16), // Friday, post-start
            ymd(2026, 1, 20), // Tuesday, post-start
            ymd(2026, 1, 21), // Wednesday, adjacent run
            ymd(2026, 3, 7),  // Saturday, non-working weekday (no-op)
        ];
        let math = CalendarMath::new(&calendar(&exceptions), start).expect("calendar");

        for index in -20_i64..=60 {
            let date = math.working_date_after(index).expect("date");
            // The mapped date is a genuine working date.
            assert!(
                math.working_weekdays.contains(&date.weekday()) && !math.exceptions.contains(&date),
                "offset {index} landed on a non-working date {date}"
            );
            // It matches an independent day scan.
            assert_eq!(
                date,
                reference_date(&math, index),
                "offset {index} date mismatch"
            );
            // The inverse agrees exactly in both directions.
            assert_eq!(
                math.working_offset_for_date(date).expect("offset"),
                index,
                "offset {index} did not round-trip"
            );
        }
    }

    #[test]
    fn no_exception_calendar_matches_the_weekly_closed_form() {
        let start = ymd(2026, 1, 5);
        let math = CalendarMath::new(&calendar(&[]), start).expect("calendar");
        for index in -10_i64..=40 {
            assert_eq!(
                math.working_date_after(index).expect("date"),
                math.weekly_working_date_after(index).expect("weekly"),
            );
        }
    }
}
