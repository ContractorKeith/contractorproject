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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleTask {
    pub id: String,
    pub parent_task_id: Option<String>,
    /// `None` identifies a summary whose schedule is derived from its children.
    pub duration_minutes: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishStartDependency {
    pub predecessor_task_id: String,
    pub successor_task_id: String,
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
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ScheduleError {
    #[error("{message}")]
    InvalidCalendar { code: &'static str, message: String },
    #[error("{message}")]
    InvalidTask { code: &'static str, message: String },
    #[error("{message}")]
    InvalidDependency { code: &'static str, message: String },
    #[error("finish-to-start dependencies contain a cycle")]
    DependencyCycle { task_ids: Vec<String> },
    #[error("the calculated schedule exceeds the supported date range")]
    ScheduleOutOfRange,
}

impl ScheduleError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCalendar { code, .. }
            | Self::InvalidTask { code, .. }
            | Self::InvalidDependency { code, .. } => code,
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
    let calendar = CalendarMath::new(&input.calendar, input.schedule_start)?;
    let hierarchy = TaskHierarchy::new(&input.tasks)?;
    let graph = LeafGraph::new(&hierarchy, &input.dependencies)?;
    let constraints = ConstraintSet::new(constraints, &hierarchy, &graph, &calendar)?;
    let task_count = graph.tasks.len();
    let mut early_start = constraints
        .start_no_earlier_than
        .iter()
        .map(|&offset| offset.max(0))
        .collect::<Vec<_>>();
    let mut early_finish = vec![0_i64; task_count];

    for &task_index in &graph.topological_order {
        early_finish[task_index] = early_start[task_index]
            .checked_add(graph.durations[task_index])
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
        for &(successor, lag) in &graph.successors[task_index] {
            let constrained_start = early_finish[task_index]
                .checked_add(lag)
                .ok_or(ScheduleError::ScheduleOutOfRange)?;
            early_start[successor] = early_start[successor].max(constrained_start);
        }
    }

    let schedule_finish_offset = early_finish.iter().copied().max().unwrap_or(0);
    let mut late_finish = vec![schedule_finish_offset; task_count];
    let mut late_start = vec![0_i64; task_count];
    for &task_index in graph.topological_order.iter().rev() {
        late_finish[task_index] =
            late_finish[task_index].min(constraints.finish_no_later_than[task_index]);
        if !graph.successors[task_index].is_empty() {
            let mut earliest_successor_start = i64::MAX;
            for &(successor, lag) in &graph.successors[task_index] {
                earliest_successor_start = earliest_successor_start.min(
                    late_start[successor]
                        .checked_sub(lag)
                        .ok_or(ScheduleError::ScheduleOutOfRange)?,
                );
            }
            late_finish[task_index] = late_finish[task_index].min(earliest_successor_start);
        }
        late_start[task_index] = late_finish[task_index]
            .checked_sub(graph.durations[task_index])
            .ok_or(ScheduleError::ScheduleOutOfRange)?;
    }

    let total_float: Vec<i64> = early_start
        .iter()
        .zip(&late_start)
        .map(|(early, late)| {
            late.checked_sub(*early)
                .ok_or(ScheduleError::ScheduleOutOfRange)
        })
        .collect::<Result<_, _>>()?;
    let mut directly_violated = Vec::new();
    for index in 0..task_count {
        let Some(deadline_date) = constraints.normalized_finish_dates[index] else {
            continue;
        };
        let milestone = graph.durations[index] == 0;
        let constrained_milestone_start = milestone
            && constraints.normalized_start_dates[index].is_some()
            && early_start[index] == constraints.start_no_earlier_than[index].max(0);
        let actual_finish = if constrained_milestone_start {
            calendar.working_day_start(
                constraints.normalized_start_dates[index]
                    .expect("constrained milestone start date"),
            )?
        } else {
            calendar.task_finish_instant(early_finish[index], milestone)?
        };
        if actual_finish > calendar.working_day_finish(deadline_date)? {
            directly_violated.push(index);
        }
    }
    let critical_path_indices = graph.critical_path(
        &early_start,
        &early_finish,
        &total_float,
        schedule_finish_offset,
        &directly_violated,
    );
    let critical_path = critical_path_indices
        .iter()
        .map(|&index| graph.tasks[index].id.clone())
        .collect();

    let mut offsets = vec![None; input.tasks.len()];
    for (index, &original_index) in graph.original_indices.iter().enumerate() {
        offsets[original_index] = Some(OffsetTask {
            early_start: early_start[index],
            early_finish: early_finish[index],
            late_start: late_start[index],
            late_finish: late_finish[index],
            total_float_minutes: total_float[index],
            duration_minutes: graph.durations[index],
            constraint_violated: directly_violated.contains(&index),
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
            &calendar,
            &offsets,
            &mut instants,
        )?;
    }

    let mut tasks = Vec::with_capacity(input.tasks.len());
    for (index, task) in input.tasks.iter().enumerate() {
        let offset = offsets[index].as_ref().expect("validated task calculation");
        let task_instants = instants[index].as_ref().expect("validated task instants");
        let summary = !hierarchy.children[index].is_empty();
        let zero_span = offset.duration_minutes == 0;
        tasks.push(ScheduledTask {
            id: task.id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            duration_minutes: offset.duration_minutes,
            early_start: task_instants.early_start,
            early_finish: task_instants.early_finish,
            late_start: task_instants.late_start,
            late_finish: task_instants.late_finish,
            total_float_minutes: offset.total_float_minutes,
            critical: offset.total_float_minutes <= 0,
            constraint_violated: offset.constraint_violated,
            milestone: !summary && zero_span,
            summary,
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
        tasks,
    })
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
        total_float_minutes: calculated_children
            .iter()
            .map(|task| task.total_float_minutes)
            .min()
            .expect("summary has children"),
        duration_minutes: early_finish
            .checked_sub(early_start)
            .ok_or(ScheduleError::ScheduleOutOfRange)?,
        constraint_violated: calculated_children
            .iter()
            .any(|task| task.constraint_violated),
    };
    offsets[index] = Some(calculated.clone());
    Ok(calculated)
}

fn derive_task_instants(
    index: usize,
    hierarchy: &TaskHierarchy<'_>,
    graph: &LeafGraph<'_>,
    constraints: &ConstraintSet,
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
    } else {
        let calculated_children = hierarchy.children[index]
            .iter()
            .map(|&child| {
                derive_task_instants(
                    child,
                    hierarchy,
                    graph,
                    constraints,
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

struct LeafGraph<'a> {
    tasks: Vec<&'a ScheduleTask>,
    original_indices: Vec<usize>,
    durations: Vec<i64>,
    predecessors: Vec<Vec<(usize, i64)>>,
    successors: Vec<Vec<(usize, i64)>>,
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
            if dependency.lag_minutes < 0 {
                return Err(invalid_dependency(
                    "dependency_lag_negative",
                    "negative lag is not supported in the FS slice",
                ));
            }
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
            if !seen.insert((predecessor, successor)) {
                return Err(invalid_dependency(
                    "dependency_duplicate",
                    "the finish-to-start dependency already exists",
                ));
            }
            successors[predecessor].push((successor, dependency.lag_minutes));
            predecessors[successor].push((predecessor, dependency.lag_minutes));
        }
        for edges in successors.iter_mut().chain(predecessors.iter_mut()) {
            edges.sort_unstable_by(|&(left, _), &(right, _)| {
                leaf_tasks[left].id.cmp(&leaf_tasks[right].id)
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

    fn critical_path(
        &self,
        early_start: &[i64],
        early_finish: &[i64],
        total_float: &[i64],
        schedule_finish: i64,
        directly_violated: &[usize],
    ) -> Vec<usize> {
        if directly_violated
            .iter()
            .any(|&index| total_float[index] < 0)
        {
            let mut current = directly_violated
                .iter()
                .copied()
                .filter(|&index| total_float[index] < 0)
                .min_by(|&left, &right| {
                    total_float[left]
                        .cmp(&total_float[right])
                        .then_with(|| self.tasks[left].id.cmp(&self.tasks[right].id))
                })
                .expect("non-empty directly violated task list");
            let mut path = vec![current];
            while let Some(predecessor) =
                self.predecessors[current].iter().find_map(|&(index, lag)| {
                    (total_float[index] == total_float[current]
                        && early_finish[index].checked_add(lag) == Some(early_start[current]))
                    .then_some(index)
                })
            {
                path.push(predecessor);
                current = predecessor;
            }
            path.reverse();
            return path;
        }
        let Some(mut current) = (0..self.tasks.len())
            .filter(|&index| {
                self.successors[index].is_empty()
                    && total_float[index] == 0
                    && early_finish[index] == schedule_finish
            })
            .min_by(|&left, &right| self.tasks[left].id.cmp(&self.tasks[right].id))
        else {
            return Vec::new();
        };
        let mut path = vec![current];
        while let Some(predecessor) = self.predecessors[current].iter().find_map(|&(index, lag)| {
            (total_float[index] == 0
                && early_finish[index].checked_add(lag) == Some(early_start[current]))
            .then_some(index)
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
    successors: &[Vec<(usize, i64)>],
    predecessors: &[Vec<(usize, i64)>],
) -> Result<Vec<usize>, ScheduleError> {
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
        for &(successor, _) in &successors[index] {
            remaining_predecessors[successor] -= 1;
            if remaining_predecessors[successor] == 0 {
                ready.insert((tasks[successor].id.clone(), successor));
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

struct CalendarMath {
    working_weekdays: HashSet<Weekday>,
    workday_start_minute: i64,
    workday_duration_minutes: i64,
    first_working_date: NaiveDate,
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
        let first_working_date = next_working_date(schedule_start, &working_weekdays)?;
        Ok(Self {
            working_weekdays,
            workday_start_minute: i64::from(calendar.workday_start_minute),
            workday_duration_minutes: i64::from(calendar.workday_duration_minutes),
            first_working_date,
        })
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

    fn working_date_after(&self, working_days: i64) -> Result<NaiveDate, ScheduleError> {
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
        next_working_date(date, &self.working_weekdays)
    }

    fn start_offset_for_working_date(&self, date: NaiveDate) -> Result<i64, ScheduleError> {
        self.working_offset_for_date(date)?
            .checked_mul(self.workday_duration_minutes)
            .ok_or(ScheduleError::ScheduleOutOfRange)
    }

    fn working_date_on_or_before(&self, date: NaiveDate) -> Result<NaiveDate, ScheduleError> {
        previous_working_date(date, &self.working_weekdays)
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

    fn working_offset_for_date(&self, date: NaiveDate) -> Result<i64, ScheduleError> {
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

fn next_working_date(
    mut date: NaiveDate,
    working_weekdays: &HashSet<Weekday>,
) -> Result<NaiveDate, ScheduleError> {
    while !working_weekdays.contains(&date.weekday()) {
        date = date.succ_opt().ok_or(ScheduleError::ScheduleOutOfRange)?;
    }
    Ok(date)
}

fn previous_working_date(
    mut date: NaiveDate,
    working_weekdays: &HashSet<Weekday>,
) -> Result<NaiveDate, ScheduleError> {
    while !working_weekdays.contains(&date.weekday()) {
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
