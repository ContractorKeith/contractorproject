use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::application::{
    AddDependencyRequest, CommandContext, RemoveDependencyRequest, ReorderTaskRequest,
    UpdateScheduleRequest, UpdateTaskDurationRequest, MAX_AUDIT_SUMMARY_CHARACTERS,
    MAX_CLIENT_NAME_CHARACTERS, MAX_COMMAND_ID_CHARACTERS,
};
use crate::domain::{FinishStartDependency, Job, JobStatus, Task};
use crate::error::ApplicationError;
use crate::work_breakdown::{plan_reorder, validate_parent_chain, validate_parent_job};

pub(crate) struct SqliteStore {
    database_path: PathBuf,
}

impl SqliteStore {
    pub(crate) fn open(database_path: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        let database_path = database_path.as_ref().to_path_buf();
        let database_existed = database_path.exists();
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { database_path };
        store.migrate(database_existed)?;
        Ok(store)
    }

    pub(crate) fn insert_job(
        &self,
        job: &Job,
        context: &CommandContext,
    ) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        transaction.execute(
            "INSERT INTO jobs (
                id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                job.id,
                job.name,
                job.status.as_database_value(),
                job.timezone,
                job.schedule_start,
                serde_json::to_string(&job.calendar).map_err(|error| ApplicationError::InvalidStoredData(error.to_string()))?,
                job.created_at,
                job.updated_at,
                job.version,
            ],
        )?;
        write_audit_record(&transaction, context, &job.created_at, "created job")?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn list_jobs(&self) -> Result<Vec<Job>, ApplicationError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version
             FROM jobs
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(
                |(
                    id,
                    name,
                    status,
                    timezone,
                    schedule_start,
                    calendar_json,
                    created_at,
                    updated_at,
                    version,
                )| {
                    let status = JobStatus::from_database_value(&status).ok_or_else(|| {
                        ApplicationError::InvalidStoredData(format!(
                            "job {id} has unsupported status {status}"
                        ))
                    })?;
                    Ok(Job {
                        id: id.clone(),
                        name,
                        status,
                        timezone,
                        schedule_start,
                        calendar: serde_json::from_str(&calendar_json).map_err(|_| {
                            ApplicationError::InvalidStoredData(format!(
                                "job {id} has invalid calendar"
                            ))
                        })?,
                        created_at,
                        updated_at,
                        version,
                    })
                },
            )
            .collect()
    }

    pub(crate) fn create_task(
        &self,
        task: &mut Task,
        expected_job_version: i64,
        context: &CommandContext,
    ) -> Result<i64, ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let current_job_version = transaction
            .query_row(
                "SELECT version FROM jobs WHERE id = ?1",
                [&task.job_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| ApplicationError::NotFound {
                resource: "job",
                id: task.job_id.clone(),
            })?;
        if current_job_version != expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: task.job_id.clone(),
                expected: expected_job_version,
                current: current_job_version,
            });
        }

        if let Some(parent_id) = task.parent_task_id.as_deref() {
            let parent_job_id = transaction
                .query_row(
                    "SELECT job_id FROM tasks WHERE id = ?1",
                    [parent_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| ApplicationError::NotFound {
                    resource: "task",
                    id: parent_id.into(),
                })?;
            validate_parent_job(&task.job_id, &parent_job_id, "parentTaskId")?;
            let parent = find_task(&transaction, parent_id)?.expect("parent was found above");
            if parent.duration_minutes.is_some() || task_has_dependencies(&transaction, parent_id)?
            {
                return Err(ApplicationError::ValidationFailed {
                    code: "summary_conversion_requires_cleanup",
                    field: "parentTaskId",
                    message: "clear the parent duration and remove its dependencies before adding a child".into(),
                });
            }
        }

        task.sort_key = transaction.query_row(
            "SELECT COALESCE(MAX(sort_key) + 1, 0)
             FROM tasks
             WHERE job_id = ?1 AND parent_task_id IS ?2",
            params![task.job_id, task.parent_task_id],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO tasks (
                id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                task.id,
                task.job_id,
                task.parent_task_id,
                task.sort_key,
                task.name,
                task.duration_minutes,
                task.created_at,
                task.updated_at,
                task.version,
            ],
        )?;
        let job_version = current_job_version + 1;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![task.updated_at, job_version, task.job_id],
        )?;
        write_audit_record(&transaction, context, &task.created_at, "created task")?;
        transaction.commit()?;
        Ok(job_version)
    }

    pub(crate) fn list_tasks(
        &self,
        job_id: &str,
    ) -> Result<(i64, Vec<Task>, Vec<FinishStartDependency>), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let hierarchy = read_task_hierarchy(&transaction, job_id)?;
        transaction.commit()?;
        Ok(hierarchy)
    }

    pub(crate) fn update_task(
        &self,
        task_id: &str,
        name: &str,
        expected_version: i64,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<(Task, i64), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let mut task = transaction
            .query_row(
                "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version
                 FROM tasks WHERE id = ?1",
                [task_id],
                task_from_row,
            )
            .optional()?
            .ok_or_else(|| ApplicationError::NotFound {
                resource: "task",
                id: task_id.into(),
            })?;
        if task.version != expected_version {
            return Err(ApplicationError::VersionConflict {
                resource: "task",
                id: task_id.into(),
                expected: expected_version,
                current: task.version,
            });
        }

        task.name = name.into();
        task.updated_at = updated_at.into();
        task.version += 1;
        transaction.execute(
            "UPDATE tasks SET name = ?1, updated_at = ?2, version = ?3 WHERE id = ?4",
            params![task.name, task.updated_at, task.version, task.id],
        )?;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = version + 1 WHERE id = ?2",
            params![updated_at, task.job_id],
        )?;
        let job_version = transaction.query_row(
            "SELECT version FROM jobs WHERE id = ?1",
            [&task.job_id],
            |row| row.get(0),
        )?;
        write_audit_record(&transaction, context, updated_at, "updated task")?;
        transaction.commit()?;
        Ok((task, job_version))
    }

    pub(crate) fn reorder_task(
        &self,
        request: &ReorderTaskRequest,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<(String, i64, Vec<Task>, Vec<FinishStartDependency>), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let task = find_task(&transaction, &request.task_id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                resource: "task",
                id: request.task_id.clone(),
            }
        })?;
        if task.version != request.expected_version {
            return Err(ApplicationError::VersionConflict {
                resource: "task",
                id: request.task_id.clone(),
                expected: request.expected_version,
                current: task.version,
            });
        }
        let current_job_version = transaction.query_row(
            "SELECT version FROM jobs WHERE id = ?1",
            [&task.job_id],
            |row| row.get::<_, i64>(0),
        )?;
        if current_job_version != request.expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: task.job_id.clone(),
                expected: request.expected_job_version,
                current: current_job_version,
            });
        }

        let ancestors = load_ancestor_chain(&transaction, request.new_parent_task_id.as_deref())?;
        validate_parent_chain(&task, &ancestors)?;
        if let Some(parent_id) = request.new_parent_task_id.as_deref() {
            let parent = find_task(&transaction, parent_id)?.expect("ancestor was found");
            if parent.duration_minutes.is_some() || task_has_dependencies(&transaction, parent_id)?
            {
                return Err(ApplicationError::ValidationFailed {
                    code: "summary_conversion_requires_cleanup",
                    field: "newParentTaskId",
                    message: "clear the parent duration and remove its dependencies before moving a child under it".into(),
                });
            }
        }
        let destination_index = usize::try_from(request.new_sibling_index).map_err(|_| {
            ApplicationError::InvalidInput {
                field: "newSiblingIndex",
                message: "is too large".into(),
            }
        })?;
        let source_siblings =
            load_siblings(&transaction, &task.job_id, task.parent_task_id.as_deref())?;
        let destination_siblings = if task.parent_task_id == request.new_parent_task_id {
            Vec::new()
        } else {
            load_siblings(
                &transaction,
                &task.job_id,
                request.new_parent_task_id.as_deref(),
            )?
        };
        let desired = plan_reorder(
            task.clone(),
            source_siblings,
            destination_siblings,
            request.new_parent_task_id.clone(),
            destination_index,
        )?;

        let has_changes = desired.iter().any(|placement| placement.changed());
        let mut job_version = current_job_version;
        if has_changes {
            let maximum_sort_key = desired
                .iter()
                .map(|placement| placement.task.sort_key)
                .max()
                .unwrap_or(0);
            let desired_count =
                i64::try_from(desired.len()).map_err(|_| ApplicationError::ValidationFailed {
                    code: "task_order_invalid",
                    field: "newSiblingIndex",
                    message: "too many tasks to reorder".into(),
                })?;
            let offset = maximum_sort_key
                .checked_add(desired_count)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| ApplicationError::ValidationFailed {
                    code: "task_order_invalid",
                    field: "newSiblingIndex",
                    message: "task order is too large to reorder".into(),
                })?;
            let mut seen = HashSet::new();
            for placement in &desired {
                if !seen.insert(placement.task.id.as_str()) {
                    return Err(ApplicationError::InvalidStoredData(format!(
                        "task {} appears twice in its hierarchy",
                        placement.task.id
                    )));
                }
                transaction.execute(
                    "UPDATE tasks SET sort_key = sort_key + ?1 WHERE id = ?2",
                    params![offset, placement.task.id],
                )?;
            }
            for placement in &desired {
                if placement.changed() {
                    transaction.execute(
                        "UPDATE tasks
                         SET parent_task_id = ?1, sort_key = ?2, updated_at = ?3, version = version + 1
                         WHERE id = ?4",
                        params![
                            placement.parent_task_id,
                            placement.sort_key,
                            updated_at,
                            placement.task.id
                        ],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE tasks SET sort_key = ?1 WHERE id = ?2",
                        params![placement.sort_key, placement.task.id],
                    )?;
                }
            }
            job_version += 1;
            transaction.execute(
                "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
                params![updated_at, job_version, task.job_id],
            )?;
        }
        let (snapshot_version, tasks, dependencies) =
            read_task_hierarchy(&transaction, &task.job_id)?;
        debug_assert_eq!(snapshot_version, job_version);
        if has_changes {
            write_audit_record(
                &transaction,
                context,
                updated_at,
                "reordered task hierarchy",
            )?;
        }
        transaction.commit()?;
        Ok((task.job_id, snapshot_version, tasks, dependencies))
    }

    pub(crate) fn update_schedule(
        &self,
        request: &UpdateScheduleRequest,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<Job, ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let current = transaction
            .query_row(
                "SELECT version FROM jobs WHERE id = ?1",
                [&request.job_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| ApplicationError::NotFound {
                resource: "job",
                id: request.job_id.clone(),
            })?;
        if current != request.expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: request.job_id.clone(),
                expected: request.expected_job_version,
                current,
            });
        }
        let calendar = serde_json::to_string(&request.calendar)
            .map_err(|error| ApplicationError::InvalidStoredData(error.to_string()))?;
        transaction.execute("UPDATE jobs SET schedule_start = ?1, calendar_json = ?2, updated_at = ?3, version = ?4 WHERE id = ?5", params![request.schedule_start, calendar, updated_at, current + 1, request.job_id])?;
        write_audit_record(
            &transaction,
            context,
            updated_at,
            "updated schedule settings",
        )?;
        let job = read_job(&transaction, &request.job_id)?;
        transaction.commit()?;
        Ok(job)
    }

    pub(crate) fn update_task_duration(
        &self,
        request: &UpdateTaskDurationRequest,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<(Task, i64), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let mut task = find_task(&transaction, &request.task_id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                resource: "task",
                id: request.task_id.clone(),
            }
        })?;
        if task.version != request.expected_version {
            return Err(ApplicationError::VersionConflict {
                resource: "task",
                id: task.id.clone(),
                expected: request.expected_version,
                current: task.version,
            });
        }
        let job_version = transaction.query_row(
            "SELECT version FROM jobs WHERE id = ?1",
            [&task.job_id],
            |row| row.get::<_, i64>(0),
        )?;
        if job_version != request.expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: task.job_id.clone(),
                expected: request.expected_job_version,
                current: job_version,
            });
        }
        let children: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE parent_task_id = ?1)",
            [&task.id],
            |row| row.get(0),
        )?;
        if children && request.duration_minutes.is_some() {
            return Err(ApplicationError::ValidationFailed {
                code: "summary_duration",
                field: "durationMinutes",
                message: "summary tasks derive duration; clear the duration before adding children"
                    .into(),
            });
        }
        if request.duration_minutes.is_none() && task_has_dependencies(&transaction, &task.id)? {
            return Err(ApplicationError::ValidationFailed {
                code: "dependency_endpoint",
                field: "durationMinutes",
                message: "remove dependencies before clearing a leaf duration".into(),
            });
        }
        task.duration_minutes = request.duration_minutes;
        task.updated_at = updated_at.into();
        task.version += 1;
        transaction.execute(
            "UPDATE tasks SET duration_minutes = ?1, updated_at = ?2, version = ?3 WHERE id = ?4",
            params![
                task.duration_minutes,
                task.updated_at,
                task.version,
                task.id
            ],
        )?;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![updated_at, job_version + 1, task.job_id],
        )?;
        write_audit_record(&transaction, context, updated_at, "updated task duration")?;
        transaction.commit()?;
        Ok((task, job_version + 1))
    }

    pub(crate) fn add_dependency(
        &self,
        request: &AddDependencyRequest,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<(i64, Vec<Task>, Vec<FinishStartDependency>), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let current = transaction
            .query_row(
                "SELECT version FROM jobs WHERE id = ?1",
                [&request.job_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| ApplicationError::NotFound {
                resource: "job",
                id: request.job_id.clone(),
            })?;
        if current != request.expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: request.job_id.clone(),
                expected: request.expected_job_version,
                current,
            });
        }
        if request.predecessor_task_id == request.successor_task_id {
            return Err(ApplicationError::ValidationFailed {
                code: "dependency_self",
                field: "successorTaskId",
                message: "a task cannot depend on itself".into(),
            });
        }
        let predecessor =
            find_task(&transaction, &request.predecessor_task_id)?.ok_or_else(|| {
                ApplicationError::NotFound {
                    resource: "task",
                    id: request.predecessor_task_id.clone(),
                }
            })?;
        let successor = find_task(&transaction, &request.successor_task_id)?.ok_or_else(|| {
            ApplicationError::NotFound {
                resource: "task",
                id: request.successor_task_id.clone(),
            }
        })?;
        if predecessor.job_id != request.job_id || successor.job_id != request.job_id {
            return Err(ApplicationError::ValidationFailed {
                code: "dependency_job",
                field: "jobId",
                message: "dependency endpoints must belong to this job".into(),
            });
        }
        for (task, field) in [
            (&predecessor, "predecessorTaskId"),
            (&successor, "successorTaskId"),
        ] {
            let is_summary: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE parent_task_id = ?1)",
                [&task.id],
                |row| row.get(0),
            )?;
            if is_summary || task.duration_minutes.is_none() {
                return Err(ApplicationError::ValidationFailed {
                    code: "dependency_endpoint",
                    field,
                    message: "dependencies require scheduled leaf tasks".into(),
                });
            }
        }
        let duplicate: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM task_dependencies WHERE predecessor_task_id = ?1 AND successor_task_id = ?2)", params![request.predecessor_task_id, request.successor_task_id], |row| row.get(0))?;
        if duplicate {
            return Err(ApplicationError::ValidationFailed {
                code: "dependency_duplicate",
                field: "successorTaskId",
                message: "that finish-to-start dependency already exists".into(),
            });
        }
        let reaches_predecessor: bool = transaction.query_row("WITH RECURSIVE reach(id) AS (SELECT successor_task_id FROM task_dependencies WHERE predecessor_task_id = ?1 UNION SELECT d.successor_task_id FROM task_dependencies d JOIN reach r ON d.predecessor_task_id = r.id) SELECT EXISTS(SELECT 1 FROM reach WHERE id = ?2)", params![request.successor_task_id, request.predecessor_task_id], |row| row.get(0))?;
        if reaches_predecessor {
            return Err(ApplicationError::ValidationFailed {
                code: "dependency_cycle",
                field: "successorTaskId",
                message: "finish-to-start dependencies cannot form a cycle".into(),
            });
        }
        transaction.execute("INSERT INTO task_dependencies (job_id, predecessor_task_id, successor_task_id, lag_minutes) VALUES (?1, ?2, ?3, ?4)", params![request.job_id, request.predecessor_task_id, request.successor_task_id, request.lag_minutes])?;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![updated_at, current + 1, request.job_id],
        )?;
        write_audit_record(
            &transaction,
            context,
            updated_at,
            "added finish-to-start dependency",
        )?;
        let (_, tasks, dependencies) = read_task_hierarchy(&transaction, &request.job_id)?;
        transaction.commit()?;
        Ok((current + 1, tasks, dependencies))
    }

    pub(crate) fn remove_dependency(
        &self,
        request: &RemoveDependencyRequest,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<(i64, Vec<Task>, Vec<FinishStartDependency>), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let current = transaction
            .query_row(
                "SELECT version FROM jobs WHERE id = ?1",
                [&request.job_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| ApplicationError::NotFound {
                resource: "job",
                id: request.job_id.clone(),
            })?;
        if current != request.expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: request.job_id.clone(),
                expected: request.expected_job_version,
                current,
            });
        }
        let deleted = transaction.execute("DELETE FROM task_dependencies WHERE job_id = ?1 AND predecessor_task_id = ?2 AND successor_task_id = ?3", params![request.job_id, request.predecessor_task_id, request.successor_task_id])?;
        if deleted == 0 {
            return Err(ApplicationError::NotFound {
                resource: "dependency",
                id: format!(
                    "{}→{}",
                    request.predecessor_task_id, request.successor_task_id
                ),
            });
        }
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![updated_at, current + 1, request.job_id],
        )?;
        write_audit_record(
            &transaction,
            context,
            updated_at,
            "removed finish-to-start dependency",
        )?;
        let (_, tasks, dependencies) = read_task_hierarchy(&transaction, &request.job_id)?;
        transaction.commit()?;
        Ok((current + 1, tasks, dependencies))
    }

    fn connection(&self) -> Result<Connection, ApplicationError> {
        let connection = Connection::open(&self.database_path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn migrate(&self, database_existed: bool) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        connection.execute_batch("PRAGMA journal_mode = WAL;")?;
        let has_migration_table: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name = 'schema_migrations'
             )",
            [],
            |row| row.get(0),
        )?;
        if database_existed && !has_migration_table {
            self.back_up_before_migration(&connection, 1)?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );",
        )?;

        if !migration_applied(&connection, 1)? {
            if database_existed {
                self.back_up_before_migration(&connection, 1)?;
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE jobs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    timezone TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version INTEGER NOT NULL CHECK (version > 0)
                 );
                 INSERT INTO schema_migrations (version, applied_at)
                 VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
            transaction.commit()?;
        }

        if !migration_applied(&connection, 2)? {
            if database_existed {
                self.back_up_before_migration(&connection, 2)?;
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE tasks (
                    id TEXT PRIMARY KEY,
                    job_id TEXT NOT NULL,
                    parent_task_id TEXT,
                    sort_key INTEGER NOT NULL CHECK (sort_key >= 0),
                    name TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version INTEGER NOT NULL CHECK (version > 0),
                    UNIQUE (job_id, id),
                    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT,
                    FOREIGN KEY (job_id, parent_task_id)
                        REFERENCES tasks(job_id, id) ON DELETE RESTRICT
                 );
                 CREATE INDEX tasks_job_parent_order
                    ON tasks(job_id, parent_task_id, sort_key, id);
                 CREATE UNIQUE INDEX tasks_root_sibling_order
                    ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
                 CREATE UNIQUE INDEX tasks_child_sibling_order
                    ON tasks(job_id, parent_task_id, sort_key)
                    WHERE parent_task_id IS NOT NULL;
                 INSERT INTO schema_migrations (version, applied_at)
                 VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
            transaction.commit()?;
        }
        if !migration_applied(&connection, 3)? {
            if database_existed {
                self.back_up_before_migration(&connection, 3)?;
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE command_log (
                    command_id TEXT NOT NULL PRIMARY KEY
                        CHECK (length(command_id) BETWEEN 1 AND 128),
                    actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')),
                    client_name TEXT NOT NULL
                        CHECK (length(client_name) BETWEEN 1 AND 120),
                    created_at TEXT NOT NULL,
                    summary TEXT NOT NULL CHECK (length(summary) <= 240)
                 );
                 INSERT INTO schema_migrations (version, applied_at)
                 VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
            transaction.commit()?;
        }
        if !migration_applied(&connection, 4)? {
            if database_existed {
                self.back_up_before_migration(&connection, 4)?;
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let calendar = serde_json::to_string(&crate::application::default_calendar())
                .map_err(|error| ApplicationError::InvalidStoredData(error.to_string()))?;
            transaction.execute("ALTER TABLE jobs ADD COLUMN schedule_start TEXT", [])?;
            transaction.execute(
                "ALTER TABLE jobs ADD COLUMN calendar_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
            transaction.execute("UPDATE jobs SET calendar_json = ?1", [&calendar])?;
            transaction.execute("ALTER TABLE tasks ADD COLUMN duration_minutes INTEGER CHECK (duration_minutes >= 0)", [])?;
            transaction.execute_batch(
                "CREATE TABLE task_dependencies (
                    job_id TEXT NOT NULL,
                    predecessor_task_id TEXT NOT NULL,
                    successor_task_id TEXT NOT NULL,
                    lag_minutes INTEGER NOT NULL CHECK (lag_minutes >= 0),
                    PRIMARY KEY (predecessor_task_id, successor_task_id),
                    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT,
                    FOREIGN KEY (job_id, predecessor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT,
                    FOREIGN KEY (job_id, successor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT,
                    CHECK (predecessor_task_id <> successor_task_id)
                 );
                 CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);
                 INSERT INTO schema_migrations (version, applied_at)
                 VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
            transaction.commit()?;
        }
        Ok(())
    }

    fn back_up_before_migration(
        &self,
        connection: &Connection,
        target_version: i64,
    ) -> Result<(), ApplicationError> {
        let file_name = self
            .database_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                ApplicationError::InvalidStoredData("database path has no file name".into())
            })?;
        let backup_path = self
            .database_path
            .with_file_name(format!("{file_name}.pre-migration-v{target_version}.bak"));
        if !backup_path.exists() {
            connection.backup("main", backup_path, None)?;
        }
        Ok(())
    }
}

fn ensure_command_is_new(
    transaction: &Transaction<'_>,
    context: &CommandContext,
) -> Result<(), ApplicationError> {
    let already_applied: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM command_log WHERE command_id = ?1)",
        [&context.command_id],
        |row| row.get(0),
    )?;
    if already_applied {
        return Err(ApplicationError::DuplicateCommand {
            command_id: context.command_id.clone(),
        });
    }
    Ok(())
}

fn write_audit_record(
    transaction: &Transaction<'_>,
    context: &CommandContext,
    created_at: &str,
    summary: &str,
) -> Result<(), ApplicationError> {
    debug_assert!(context.command_id.chars().count() <= MAX_COMMAND_ID_CHARACTERS);
    debug_assert!(context.client_name.chars().count() <= MAX_CLIENT_NAME_CHARACTERS);
    debug_assert!(summary.chars().count() <= MAX_AUDIT_SUMMARY_CHARACTERS);
    transaction.execute(
        "INSERT INTO command_log (command_id, actor, client_name, created_at, summary)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            context.command_id,
            context.actor.as_database_value(),
            context.client_name,
            created_at,
            summary,
        ],
    )?;
    Ok(())
}

fn migration_applied(connection: &Connection, version: i64) -> Result<bool, ApplicationError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
        [version],
        |row| row.get(0),
    )?)
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        job_id: row.get(1)?,
        parent_task_id: row.get(2)?,
        sort_key: row.get(3)?,
        name: row.get(4)?,
        duration_minutes: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        version: row.get(8)?,
    })
}

fn read_job(connection: &Connection, job_id: &str) -> Result<Job, ApplicationError> {
    let (id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version) = connection.query_row(
        "SELECT id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version FROM jobs WHERE id = ?1",
        [job_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?, row.get::<_, String>(7)?, row.get::<_, i64>(8)?)),
    ).optional()?.ok_or_else(|| ApplicationError::NotFound { resource: "job", id: job_id.into() })?;
    Ok(Job {
        id: id.clone(),
        name,
        status: JobStatus::from_database_value(&status).ok_or_else(|| {
            ApplicationError::InvalidStoredData(format!("job {id} has unsupported status {status}"))
        })?,
        timezone,
        schedule_start,
        calendar: serde_json::from_str(&calendar_json).map_err(|_| {
            ApplicationError::InvalidStoredData(format!("job {id} has invalid calendar"))
        })?,
        created_at,
        updated_at,
        version,
    })
}

fn read_task_hierarchy(
    connection: &Connection,
    job_id: &str,
) -> Result<(i64, Vec<Task>, Vec<FinishStartDependency>), ApplicationError> {
    let job_version = connection
        .query_row("SELECT version FROM jobs WHERE id = ?1", [job_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .ok_or_else(|| ApplicationError::NotFound {
            resource: "job",
            id: job_id.into(),
        })?;
    let task_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM tasks WHERE job_id = ?1",
        [job_id],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(
        "WITH RECURSIVE task_tree(
            id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version, path
         ) AS (
            SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version,
                   printf('%020d:%s', sort_key, id)
            FROM tasks
            WHERE job_id = ?1 AND parent_task_id IS NULL
            UNION ALL
            SELECT child.id, child.job_id, child.parent_task_id, child.sort_key, child.name,
                   child.duration_minutes, child.created_at, child.updated_at, child.version,
                   parent.path || '/' || printf('%020d:%s', child.sort_key, child.id)
            FROM tasks child
            JOIN task_tree parent ON child.parent_task_id = parent.id
            WHERE child.job_id = ?1
         )
         SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version
         FROM task_tree
         ORDER BY path",
    )?;
    let tasks = statement
        .query_map([job_id], task_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if i64::try_from(tasks.len()).ok() != Some(task_count) {
        return Err(ApplicationError::InvalidStoredData(format!(
            "job {job_id} has an invalid task hierarchy"
        )));
    }
    let mut statement = connection.prepare("SELECT predecessor_task_id, successor_task_id, lag_minutes FROM task_dependencies WHERE job_id = ?1 ORDER BY predecessor_task_id, successor_task_id")?;
    let dependencies = statement
        .query_map([job_id], |row| {
            Ok(FinishStartDependency {
                predecessor_task_id: row.get(0)?,
                successor_task_id: row.get(1)?,
                lag_minutes: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((job_version, tasks, dependencies))
}

fn find_task(
    transaction: &Transaction<'_>,
    task_id: &str,
) -> Result<Option<Task>, ApplicationError> {
    transaction
        .query_row(
            "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version
             FROM tasks WHERE id = ?1",
            [task_id],
            task_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn task_has_dependencies(
    transaction: &Transaction<'_>,
    task_id: &str,
) -> Result<bool, ApplicationError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM task_dependencies WHERE predecessor_task_id = ?1 OR successor_task_id = ?1)",
            [task_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn load_siblings(
    transaction: &Transaction<'_>,
    job_id: &str,
    parent_task_id: Option<&str>,
) -> Result<Vec<Task>, ApplicationError> {
    let mut statement = transaction.prepare(
        "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version
         FROM tasks
         WHERE job_id = ?1 AND parent_task_id IS ?2
         ORDER BY sort_key, id",
    )?;
    let tasks = statement
        .query_map(params![job_id, parent_task_id], task_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tasks)
}

fn load_ancestor_chain(
    transaction: &Transaction<'_>,
    new_parent_task_id: Option<&str>,
) -> Result<Vec<Task>, ApplicationError> {
    let mut cursor = new_parent_task_id.map(str::to_owned);
    let mut visited = HashSet::new();
    let mut ancestors = Vec::new();
    while let Some(candidate_id) = cursor.take() {
        if !visited.insert(candidate_id.clone()) {
            return Err(ApplicationError::InvalidStoredData(format!(
                "task {candidate_id} is part of a parent cycle"
            )));
        }
        let candidate =
            find_task(transaction, &candidate_id)?.ok_or_else(|| ApplicationError::NotFound {
                resource: "task",
                id: candidate_id.clone(),
            })?;
        cursor = candidate.parent_task_id.clone();
        ancestors.push(candidate);
    }
    Ok(ancestors)
}
