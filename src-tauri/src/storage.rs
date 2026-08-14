use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::domain::{Job, JobStatus, Task};
use crate::error::ApplicationError;

pub(crate) struct SqliteJobStore {
    database_path: PathBuf,
}

impl SqliteJobStore {
    pub(crate) fn open(database_path: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        let database_path = database_path.as_ref().to_path_buf();
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { database_path };
        store.migrate()?;
        Ok(store)
    }

    pub(crate) fn insert_job(&self, job: &Job) -> Result<(), ApplicationError> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO jobs (
                id, name, status, timezone, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                job.id,
                job.name,
                job.status.as_database_value(),
                job.timezone,
                job.created_at,
                job.updated_at,
                job.version,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn list_jobs(&self) -> Result<Vec<Job>, ApplicationError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, status, timezone, created_at, updated_at, version
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
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(
                |(id, name, status, timezone, created_at, updated_at, version)| {
                    let status = JobStatus::from_database_value(&status).ok_or_else(|| {
                        ApplicationError::InvalidStoredData(format!(
                            "job {id} has unsupported status {status}"
                        ))
                    })?;
                    Ok(Job {
                        id,
                        name,
                        status,
                        timezone,
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
    ) -> Result<i64, ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
            if parent_job_id != task.job_id {
                return Err(ApplicationError::ValidationFailed {
                    code: "task_parent_cross_job",
                    message: "a task parent must belong to the same job".into(),
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
                id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                task.id,
                task.job_id,
                task.parent_task_id,
                task.sort_key,
                task.name,
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
        transaction.commit()?;
        Ok(job_version)
    }

    pub(crate) fn list_tasks(&self, job_id: &str) -> Result<(i64, Vec<Task>), ApplicationError> {
        let connection = self.connection()?;
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
                id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version, path
             ) AS (
                SELECT id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version,
                       printf('%020d:%s', sort_key, id)
                FROM tasks
                WHERE job_id = ?1 AND parent_task_id IS NULL
                UNION ALL
                SELECT child.id, child.job_id, child.parent_task_id, child.sort_key, child.name,
                       child.created_at, child.updated_at, child.version,
                       parent.path || '/' || printf('%020d:%s', child.sort_key, child.id)
                FROM tasks child
                JOIN task_tree parent ON child.parent_task_id = parent.id
                WHERE child.job_id = ?1
             )
             SELECT id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version
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
        Ok((job_version, tasks))
    }

    pub(crate) fn update_task(
        &self,
        task_id: &str,
        name: &str,
        expected_version: i64,
        updated_at: &str,
    ) -> Result<(Task, i64), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut task = transaction
            .query_row(
                "SELECT id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version
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
        transaction.commit()?;
        Ok((task, job_version))
    }

    pub(crate) fn reorder_task(
        &self,
        task_id: &str,
        new_parent_task_id: Option<&str>,
        new_sibling_index: i64,
        expected_version: i64,
        expected_job_version: i64,
        updated_at: &str,
    ) -> Result<(String, i64, Vec<Task>), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = find_task(&transaction, task_id)?.ok_or_else(|| ApplicationError::NotFound {
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
        let current_job_version = transaction.query_row(
            "SELECT version FROM jobs WHERE id = ?1",
            [&task.job_id],
            |row| row.get::<_, i64>(0),
        )?;
        if current_job_version != expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: task.job_id.clone(),
                expected: expected_job_version,
                current: current_job_version,
            });
        }

        validate_new_parent(&transaction, &task, new_parent_task_id)?;
        let destination_index =
            usize::try_from(new_sibling_index).map_err(|_| ApplicationError::InvalidInput {
                field: "newSiblingIndex",
                message: "is too large".into(),
            })?;
        let mut desired: Vec<(Task, Option<String>, i64)> = Vec::new();
        if task.parent_task_id.as_deref() == new_parent_task_id {
            let mut siblings =
                load_siblings(&transaction, &task.job_id, task.parent_task_id.as_deref())?;
            siblings.retain(|sibling| sibling.id != task.id);
            validate_destination_index(destination_index, siblings.len())?;
            siblings.insert(destination_index, task.clone());
            for (index, sibling) in siblings.into_iter().enumerate() {
                desired.push((sibling, task.parent_task_id.clone(), index as i64));
            }
        } else {
            let mut source =
                load_siblings(&transaction, &task.job_id, task.parent_task_id.as_deref())?;
            source.retain(|sibling| sibling.id != task.id);
            for (index, sibling) in source.into_iter().enumerate() {
                desired.push((sibling, task.parent_task_id.clone(), index as i64));
            }

            let mut destination = load_siblings(&transaction, &task.job_id, new_parent_task_id)?;
            validate_destination_index(destination_index, destination.len())?;
            destination.insert(destination_index, task.clone());
            for (index, sibling) in destination.into_iter().enumerate() {
                desired.push((sibling, new_parent_task_id.map(str::to_owned), index as i64));
            }
        }

        let has_changes = desired.iter().any(|(original, parent, sort_key)| {
            original.parent_task_id != *parent || original.sort_key != *sort_key
        });
        let mut job_version = current_job_version;
        if has_changes {
            let maximum_sort_key = desired
                .iter()
                .map(|(task, _, _)| task.sort_key)
                .max()
                .unwrap_or(0);
            let desired_count =
                i64::try_from(desired.len()).map_err(|_| ApplicationError::ValidationFailed {
                    code: "task_order_invalid",
                    message: "too many tasks to reorder".into(),
                })?;
            let offset = maximum_sort_key
                .checked_add(desired_count)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| ApplicationError::ValidationFailed {
                    code: "task_order_invalid",
                    message: "task order is too large to reorder".into(),
                })?;
            let mut seen = HashSet::new();
            for (original, _, _) in &desired {
                if !seen.insert(original.id.as_str()) {
                    return Err(ApplicationError::InvalidStoredData(format!(
                        "task {} appears twice in its hierarchy",
                        original.id
                    )));
                }
                transaction.execute(
                    "UPDATE tasks SET sort_key = sort_key + ?1 WHERE id = ?2",
                    params![offset, original.id],
                )?;
            }
            for (original, parent_task_id, sort_key) in &desired {
                let changed =
                    original.parent_task_id != *parent_task_id || original.sort_key != *sort_key;
                if changed {
                    transaction.execute(
                        "UPDATE tasks
                         SET parent_task_id = ?1, sort_key = ?2, updated_at = ?3, version = version + 1
                         WHERE id = ?4",
                        params![parent_task_id, sort_key, updated_at, original.id],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE tasks SET sort_key = ?1 WHERE id = ?2",
                        params![sort_key, original.id],
                    )?;
                }
            }
            job_version += 1;
            transaction.execute(
                "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
                params![updated_at, job_version, task.job_id],
            )?;
        }
        transaction.commit()?;
        let (_, tasks) = self.list_tasks(&task.job_id)?;
        Ok((task.job_id, job_version, tasks))
    }

    fn connection(&self) -> Result<Connection, ApplicationError> {
        let connection = Connection::open(&self.database_path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn migrate(&self) -> Result<(), ApplicationError> {
        let connection = self.connection()?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timezone TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL CHECK (version > 0)
             );
             CREATE TABLE IF NOT EXISTS tasks (
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
             CREATE INDEX IF NOT EXISTS tasks_job_parent_order
                ON tasks(job_id, parent_task_id, sort_key, id);
             CREATE UNIQUE INDEX IF NOT EXISTS tasks_root_sibling_order
                ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
             CREATE UNIQUE INDEX IF NOT EXISTS tasks_child_sibling_order
                ON tasks(job_id, parent_task_id, sort_key)
                WHERE parent_task_id IS NOT NULL;
             INSERT OR IGNORE INTO schema_migrations (version, applied_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             INSERT OR IGNORE INTO schema_migrations (version, applied_at)
             VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        Ok(())
    }
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        job_id: row.get(1)?,
        parent_task_id: row.get(2)?,
        sort_key: row.get(3)?,
        name: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        version: row.get(7)?,
    })
}

fn find_task(
    transaction: &Transaction<'_>,
    task_id: &str,
) -> Result<Option<Task>, ApplicationError> {
    transaction
        .query_row(
            "SELECT id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version
             FROM tasks WHERE id = ?1",
            [task_id],
            task_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn load_siblings(
    transaction: &Transaction<'_>,
    job_id: &str,
    parent_task_id: Option<&str>,
) -> Result<Vec<Task>, ApplicationError> {
    let mut statement = transaction.prepare(
        "SELECT id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version
         FROM tasks
         WHERE job_id = ?1 AND parent_task_id IS ?2
         ORDER BY sort_key, id",
    )?;
    let tasks = statement
        .query_map(params![job_id, parent_task_id], task_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tasks)
}

fn validate_new_parent(
    transaction: &Transaction<'_>,
    task: &Task,
    new_parent_task_id: Option<&str>,
) -> Result<(), ApplicationError> {
    let mut cursor = new_parent_task_id.map(str::to_owned);
    let mut visited = HashSet::new();
    while let Some(candidate_id) = cursor {
        if candidate_id == task.id {
            return Err(ApplicationError::ValidationFailed {
                code: "task_parent_cycle",
                message: "a task cannot be placed below itself or one of its descendants".into(),
            });
        }
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
        if candidate.job_id != task.job_id {
            return Err(ApplicationError::ValidationFailed {
                code: "task_parent_cross_job",
                message: "a task parent must belong to the same job".into(),
            });
        }
        cursor = candidate.parent_task_id;
    }
    Ok(())
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
