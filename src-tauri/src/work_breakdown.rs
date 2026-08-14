use crate::domain::Task;
use crate::error::ApplicationError;

pub(crate) struct TaskPlacement {
    pub(crate) task: Task,
    pub(crate) parent_task_id: Option<String>,
    pub(crate) sort_key: i64,
}

impl TaskPlacement {
    pub(crate) fn changed(&self) -> bool {
        self.task.parent_task_id != self.parent_task_id || self.task.sort_key != self.sort_key
    }
}

pub(crate) fn validate_parent_job(
    job_id: &str,
    parent_job_id: &str,
    field: &'static str,
) -> Result<(), ApplicationError> {
    if parent_job_id != job_id {
        return Err(ApplicationError::ValidationFailed {
            code: "task_parent_cross_job",
            field,
            message: "a task parent must belong to the same job".into(),
        });
    }
    Ok(())
}

pub(crate) fn validate_parent_chain(
    task: &Task,
    ancestors: &[Task],
) -> Result<(), ApplicationError> {
    for ancestor in ancestors {
        validate_parent_job(&task.job_id, &ancestor.job_id, "newParentTaskId")?;
        if ancestor.id == task.id {
            return Err(ApplicationError::ValidationFailed {
                code: "task_parent_cycle",
                field: "newParentTaskId",
                message: "a task cannot be placed below itself or one of its descendants".into(),
            });
        }
    }
    Ok(())
}

pub(crate) fn plan_reorder(
    task: Task,
    mut source_siblings: Vec<Task>,
    mut destination_siblings: Vec<Task>,
    new_parent_task_id: Option<String>,
    destination_index: usize,
) -> Result<Vec<TaskPlacement>, ApplicationError> {
    source_siblings.retain(|sibling| sibling.id != task.id);
    let same_parent = task.parent_task_id == new_parent_task_id;
    if same_parent {
        validate_destination_index(destination_index, source_siblings.len())?;
        source_siblings.insert(destination_index, task.clone());
        return placements(source_siblings, task.parent_task_id);
    }

    validate_destination_index(destination_index, destination_siblings.len())?;
    destination_siblings.insert(destination_index, task.clone());
    let mut planned = placements(source_siblings, task.parent_task_id)?;
    planned.extend(placements(destination_siblings, new_parent_task_id)?);
    Ok(planned)
}

fn placements(
    siblings: Vec<Task>,
    parent_task_id: Option<String>,
) -> Result<Vec<TaskPlacement>, ApplicationError> {
    siblings
        .into_iter()
        .enumerate()
        .map(|(index, task)| {
            let sort_key =
                i64::try_from(index).map_err(|_| ApplicationError::ValidationFailed {
                    code: "task_order_invalid",
                    field: "newSiblingIndex",
                    message: "too many tasks to reorder".into(),
                })?;
            Ok(TaskPlacement {
                task,
                parent_task_id: parent_task_id.clone(),
                sort_key,
            })
        })
        .collect()
}

fn validate_destination_index(index: usize, sibling_count: usize) -> Result<(), ApplicationError> {
    if index > sibling_count {
        return Err(ApplicationError::InvalidInput {
            field: "newSiblingIndex",
            message: format!("must be between 0 and {sibling_count}"),
        });
    }
    Ok(())
}
