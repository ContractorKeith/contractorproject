use std::collections::HashSet;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{NaiveDate, SecondsFormat, Utc};
use rusqlite::backup::Backup;
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use uuid::Uuid;

use crate::application::{
    AddDependencyRequest, BackupResult, CommandContext, RemoveDependencyRequest,
    ReorderTaskRequest, RestoreVerificationResult, TaskConstraintKind, UpdateJobDataDateRequest,
    UpdateScheduleRequest, UpdateTaskConstraintRequest, UpdateTaskDurationRequest,
    UpdateTaskProgressRequest, MAX_AUDIT_SUMMARY_CHARACTERS, MAX_CLIENT_NAME_CHARACTERS,
    MAX_COMMAND_ID_CHARACTERS,
};
use crate::domain::{FinishStartDependency, Job, JobStatus, Task};
use crate::error::ApplicationError;
use crate::scheduling::{
    calculate_schedule_with_progress, FinishStartDependency as ScheduleDependency, ScheduleInput,
    ScheduleProgress, ScheduleTask, TaskConstraint, TaskProgress,
};
use crate::work_breakdown::{plan_reorder, validate_parent_chain, validate_parent_job};

pub(crate) struct SqliteStore {
    database_path: PathBuf,
}

pub(crate) struct PublishedRestore {
    pub(crate) target_database_path: PathBuf,
    staging_directory: PathBuf,
}

#[derive(Clone, Copy)]
enum JobStatusTransition {
    Archive,
    Restore,
}

impl JobStatusTransition {
    fn expected_status(self) -> JobStatus {
        match self {
            Self::Archive => JobStatus::Draft,
            Self::Restore => JobStatus::Archived,
        }
    }

    fn next_status(self) -> JobStatus {
        match self {
            Self::Archive => JobStatus::Archived,
            Self::Restore => JobStatus::Draft,
        }
    }

    fn audit_summary(self) -> &'static str {
        match self {
            Self::Archive => "archived job",
            Self::Restore => "restored job",
        }
    }
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
        self.list_jobs_matching("WHERE status != 'archived'", [])
    }

    pub(crate) fn list_jobs_by_status(
        &self,
        status: JobStatus,
    ) -> Result<Vec<Job>, ApplicationError> {
        self.list_jobs_matching("WHERE status = ?1", [status.as_database_value()])
    }

    fn list_jobs_matching<P>(
        &self,
        predicate: &str,
        params: P,
    ) -> Result<Vec<Job>, ApplicationError>
    where
        P: rusqlite::Params,
    {
        let connection = self.connection()?;
        let mut statement = connection.prepare(&format!(
            "SELECT id, name, status, timezone, schedule_start, calendar_json, data_date, created_at, updated_at, version
             FROM jobs {predicate}
             ORDER BY created_at DESC, id DESC"
        ))?;
        let rows = statement
            .query_map(params, |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
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
                    data_date,
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
                        data_date,
                        created_at,
                        updated_at,
                        version,
                    })
                },
            )
            .collect()
    }

    pub(crate) fn archive_job(
        &self,
        job_id: &str,
        expected_job_version: i64,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<Job, ApplicationError> {
        self.transition_job_status(
            job_id,
            expected_job_version,
            JobStatusTransition::Archive,
            updated_at,
            context,
        )
    }

    pub(crate) fn restore_job(
        &self,
        job_id: &str,
        expected_job_version: i64,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<Job, ApplicationError> {
        self.transition_job_status(
            job_id,
            expected_job_version,
            JobStatusTransition::Restore,
            updated_at,
            context,
        )
    }

    fn transition_job_status(
        &self,
        job_id: &str,
        expected_job_version: i64,
        transition_kind: JobStatusTransition,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<Job, ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let job = read_job(&transaction, job_id)?;
        if job.version != expected_job_version {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: job_id.into(),
                expected: expected_job_version,
                current: job.version,
            });
        }
        if job.status != transition_kind.expected_status() {
            return Err(ApplicationError::ValidationFailed {
                code: "job_status_transition",
                field: "status",
                message: "job is not in a state that supports this action".into(),
            });
        }
        let version = job.version + 1;
        transaction.execute(
            "UPDATE jobs SET status = ?1, updated_at = ?2, version = ?3 WHERE id = ?4",
            params![
                transition_kind.next_status().as_database_value(),
                updated_at,
                version,
                job_id
            ],
        )?;
        write_audit_record(
            &transaction,
            context,
            updated_at,
            transition_kind.audit_summary(),
        )?;
        transaction.commit()?;
        Ok(Job {
            status: transition_kind.next_status(),
            updated_at: updated_at.into(),
            version,
            ..job
        })
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
        let current_job_version =
            require_draft_job(&transaction, &task.job_id, Some(expected_job_version))?;

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
            if parent.duration_minutes.is_some()
                || parent.start_no_earlier_than.is_some()
                || parent.finish_no_later_than.is_some()
                || parent.percent_complete.is_some()
                || parent.actual_start.is_some()
                || parent.actual_finish.is_some()
                || task_has_dependencies(&transaction, parent_id)?
            {
                return Err(ApplicationError::ValidationFailed {
                    code: "summary_conversion_requires_cleanup",
                    field: "parentTaskId",
                    message: "clear the parent duration, constraints, progress, and dependencies before adding a child".into(),
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
                id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.id,
                task.job_id,
                task.parent_task_id,
                task.sort_key,
                task.name,
                task.duration_minutes,
                task.start_no_earlier_than,
                task.finish_no_later_than,
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

    pub(crate) fn schedule_inputs(
        &self,
        job_id: &str,
    ) -> Result<(Job, Vec<Task>, Vec<FinishStartDependency>), ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let job = read_job(&transaction, job_id)?;
        let (_, tasks, dependencies) = read_task_hierarchy(&transaction, job_id)?;
        transaction.commit()?;
        Ok((job, tasks, dependencies))
    }

    pub(crate) fn create_verified_backup(
        &self,
        destination: &str,
    ) -> Result<BackupResult, ApplicationError> {
        let destination = PathBuf::from(destination);
        self.create_verified_backup_with(&destination, |path| verify_backup(path).map(|_| ()))
    }

    pub(crate) fn restore_verified_backup_into_fresh_app_data(
        &self,
        backup_path: &str,
        target_app_data_dir: &str,
    ) -> Result<PublishedRestore, ApplicationError> {
        let backup_path = PathBuf::from(backup_path);
        let target_app_data_dir = PathBuf::from(target_app_data_dir);

        // This preflight uses a read-only connection and happens before any
        // target or staging directory exists, so untrusted input is never
        // migrated or activated.
        verify_backup(&backup_path).map_err(|_| ApplicationError::RestoreVerificationFailed)?;
        reserve_fresh_restore_target(&target_app_data_dir)?;
        let staging = match create_restore_staging_directory(&target_app_data_dir) {
            Ok(staging) => staging,
            Err(error) => {
                cleanup_empty_restore_reservation(&target_app_data_dir)?;
                return Err(error);
            }
        };
        let target_database_path = target_app_data_dir.join("contractorproject.sqlite3");
        let staged_database_path = staging.join("contractorproject.sqlite3");

        let restore_result = (|| {
            copy_verified_snapshot(&backup_path, &staged_database_path)?;
            verify_backup(&staged_database_path)
                .map_err(|_| ApplicationError::RestoreVerificationFailed)?;
            publish_restored_database(&staged_database_path, &target_database_path)?;
            Ok(PublishedRestore {
                target_database_path,
                staging_directory: staging.clone(),
            })
        })();

        match restore_result {
            Ok(published) => Ok(published),
            Err(error) => {
                cleanup_unpublished_restore(&staging, &target_app_data_dir)?;
                Err(error)
            }
        }
    }

    pub(crate) fn verify_restored_database(
        &self,
    ) -> Result<RestoreVerificationResult, ApplicationError> {
        let snapshot = verify_backup(&self.database_path)
            .map_err(|_| ApplicationError::RestoreVerificationFailed)?;
        Ok(RestoreVerificationResult {
            verified: true,
            job_count: snapshot.job_count,
            task_count: snapshot.task_count,
            dependency_count: snapshot.dependency_count,
            command_log_count: snapshot.command_log_count,
        })
    }

    pub(crate) fn finalize_published_restore(
        published: &PublishedRestore,
    ) -> Result<(), ApplicationError> {
        cleanup_owned_restore_staging(&published.staging_directory)
    }

    pub(crate) fn rollback_published_restore(
        published: &PublishedRestore,
    ) -> Result<(), ApplicationError> {
        if !same_regular_file(
            &published
                .staging_directory
                .join("contractorproject.sqlite3"),
            &published.target_database_path,
        ) {
            return Err(ApplicationError::RestoreFailed);
        }
        std::fs::remove_file(&published.target_database_path)
            .map_err(|_| ApplicationError::RestoreFailed)?;
        let target = published
            .target_database_path
            .parent()
            .ok_or(ApplicationError::RestoreFailed)?;
        let target_result = cleanup_empty_restore_reservation(target);
        let staging_result = cleanup_owned_restore_staging(&published.staging_directory);
        if target_result.is_err() || staging_result.is_err() {
            return Err(ApplicationError::RestoreFailed);
        }
        Ok(())
    }

    fn create_verified_backup_with(
        &self,
        destination: &Path,
        verify: impl FnOnce(&Path) -> Result<(), ApplicationError>,
    ) -> Result<BackupResult, ApplicationError> {
        let incomplete_destination = create_incomplete_backup_path(destination)?;
        let backup_result = (|| {
            let source = self.connection()?;
            let mut target = Connection::open(&incomplete_destination)
                .map_err(|_| ApplicationError::BackupFailed)?;
            // This file is private, unverified, and never published in place.
            // Avoid a rollback-journal sync in File Provider-managed folders;
            // durability is established by sync_all after the backup completes.
            target
                .execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")
                .map_err(|_| ApplicationError::BackupFailed)?;
            {
                let backup = Backup::new(&source, &mut target)
                    .map_err(|_| ApplicationError::BackupFailed)?;
                backup
                    .run_to_completion(100, Duration::from_millis(1), None)
                    .map_err(|_| ApplicationError::BackupFailed)?;
            }
            drop(target);
            OpenOptions::new()
                .write(true)
                .open(&incomplete_destination)
                .and_then(|file| file.sync_all())
                .map_err(|_| ApplicationError::BackupFailed)?;
            verify(&incomplete_destination)?;
            publish_incomplete_backup(&incomplete_destination, destination)?;
            let byte_size = std::fs::metadata(destination)
                .map_err(|_| ApplicationError::BackupFailed)?
                .len();
            Ok(BackupResult {
                destination: destination.to_string_lossy().into_owned(),
                created_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                byte_size,
                verified: true,
            })
        })();

        match backup_result {
            Ok(result) => {
                cleanup_owned_incomplete_backup(&incomplete_destination)?;
                Ok(result)
            }
            Err(error) => {
                let _ = cleanup_owned_incomplete_backup(&incomplete_destination);
                Err(error)
            }
        }
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
                "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version
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
        let current_job_version = require_draft_job(&transaction, &task.job_id, None)?;

        task.name = name.into();
        task.updated_at = updated_at.into();
        task.version += 1;
        transaction.execute(
            "UPDATE tasks SET name = ?1, updated_at = ?2, version = ?3 WHERE id = ?4",
            params![task.name, task.updated_at, task.version, task.id],
        )?;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![updated_at, current_job_version + 1, task.job_id],
        )?;
        write_audit_record(&transaction, context, updated_at, "updated task")?;
        transaction.commit()?;
        Ok((task, current_job_version + 1))
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
        let current_job_version = require_draft_job(
            &transaction,
            &task.job_id,
            Some(request.expected_job_version),
        )?;

        let ancestors = load_ancestor_chain(&transaction, request.new_parent_task_id.as_deref())?;
        validate_parent_chain(&task, &ancestors)?;
        if let Some(parent_id) = request.new_parent_task_id.as_deref() {
            let parent = find_task(&transaction, parent_id)?.expect("ancestor was found");
            if parent.duration_minutes.is_some()
                || parent.start_no_earlier_than.is_some()
                || parent.finish_no_later_than.is_some()
                || task_has_dependencies(&transaction, parent_id)?
            {
                return Err(ApplicationError::ValidationFailed {
                    code: "summary_conversion_requires_cleanup",
                    field: "newParentTaskId",
                    message: "clear the parent duration, constraints, and dependencies before moving a child under it".into(),
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
        let current = require_draft_job(
            &transaction,
            &request.job_id,
            Some(request.expected_job_version),
        )?;
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
        let job_version = require_draft_job(
            &transaction,
            &task.job_id,
            Some(request.expected_job_version),
        )?;
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
        if request.duration_minutes.is_none()
            && (task.start_no_earlier_than.is_some() || task.finish_no_later_than.is_some())
        {
            return Err(ApplicationError::ValidationFailed {
                code: "summary_constraint",
                field: "durationMinutes",
                message: "clear task constraints before clearing a leaf duration".into(),
            });
        }
        if request.duration_minutes.is_none()
            && (task.percent_complete.is_some()
                || task.actual_start.is_some()
                || task.actual_finish.is_some())
        {
            return Err(ApplicationError::ValidationFailed {
                code: "summary_progress",
                field: "durationMinutes",
                message: "clear task progress before clearing a leaf duration".into(),
            });
        }
        // Converting to a milestone (zero duration) must not strand persisted
        // progress the scheduler would reject: a milestone accepts only 0 or 100
        // percent, and a complete milestone requires equal actual dates.
        if request.duration_minutes == Some(0) {
            let partial =
                matches!(task.percent_complete, Some(percent) if (1..=99).contains(&percent));
            let complete_unequal =
                task.percent_complete == Some(100) && task.actual_start != task.actual_finish;
            if partial || complete_unequal {
                return Err(ApplicationError::ValidationFailed {
                    code: "milestone_progress",
                    field: "durationMinutes",
                    message: "clear or complete task progress before converting to a milestone"
                        .into(),
                });
            }
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

    pub(crate) fn update_task_constraint(
        &self,
        request: &UpdateTaskConstraintRequest,
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
        let job_version = require_draft_job(
            &transaction,
            &task.job_id,
            Some(request.expected_job_version),
        )?;
        let is_summary: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE parent_task_id = ?1)",
            [&task.id],
            |row| row.get(0),
        )?;
        if is_summary {
            return Err(ApplicationError::ValidationFailed {
                code: "summary_constraint",
                field: "taskId",
                message: "constraints apply only to leaf tasks".into(),
            });
        }
        match request.kind {
            TaskConstraintKind::StartNoEarlierThan => {
                task.start_no_earlier_than = request.value.clone()
            }
            TaskConstraintKind::FinishNoLaterThan => {
                task.finish_no_later_than = request.value.clone()
            }
        }
        validate_proposed_schedule(&transaction, &task.job_id, &ProposedEdit::Task(&task))?;
        task.updated_at = updated_at.into();
        task.version += 1;
        transaction.execute(
            "UPDATE tasks SET start_no_earlier_than = ?1, finish_no_later_than = ?2, updated_at = ?3, version = ?4 WHERE id = ?5",
            params![task.start_no_earlier_than, task.finish_no_later_than, task.updated_at, task.version, task.id],
        )?;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![updated_at, job_version + 1, task.job_id],
        )?;
        write_audit_record(&transaction, context, updated_at, "updated task constraint")?;
        transaction.commit()?;
        Ok((task, job_version + 1))
    }

    /// Sets or clears the job data date. The whole proposed schedule (including
    /// persisted progress) is validated before commit, so clearing a data date
    /// while any task still carries progress is rejected by the scheduler.
    pub(crate) fn update_job_data_date(
        &self,
        request: &UpdateJobDataDateRequest,
        updated_at: &str,
        context: &CommandContext,
    ) -> Result<Job, ApplicationError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_command_is_new(&transaction, context)?;
        let job_version = require_draft_job(
            &transaction,
            &request.job_id,
            Some(request.expected_job_version),
        )?;
        validate_proposed_schedule(
            &transaction,
            &request.job_id,
            &ProposedEdit::DataDate(request.data_date.as_deref()),
        )?;
        transaction.execute(
            "UPDATE jobs SET data_date = ?1, updated_at = ?2, version = ?3 WHERE id = ?4",
            params![
                request.data_date,
                updated_at,
                job_version + 1,
                request.job_id
            ],
        )?;
        write_audit_record(&transaction, context, updated_at, "updated job data date")?;
        let job = read_job(&transaction, &request.job_id)?;
        transaction.commit()?;
        Ok(job)
    }

    /// Sets or clears leaf task progress. Summary tasks are rejected and the
    /// complete proposed schedule is validated before commit; a clear nulls all
    /// three progress columns back to unstatused.
    pub(crate) fn update_task_progress(
        &self,
        request: &UpdateTaskProgressRequest,
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
        let job_version = require_draft_job(
            &transaction,
            &task.job_id,
            Some(request.expected_job_version),
        )?;
        let is_summary: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE parent_task_id = ?1)",
            [&task.id],
            |row| row.get(0),
        )?;
        if is_summary {
            return Err(ApplicationError::ValidationFailed {
                code: "summary_progress",
                field: "taskId",
                message: "progress applies only to leaf tasks".into(),
            });
        }
        // A clear nulls every progress column; otherwise the request values are
        // proposed as-is and validated as a whole schedule before commit.
        let values = (!request.clear).then(|| ProposedProgress {
            percent_complete: request.percent_complete,
            actual_start: request.actual_start.clone(),
            actual_finish: request.actual_finish.clone(),
        });
        validate_proposed_schedule(
            &transaction,
            &task.job_id,
            &ProposedEdit::Progress {
                task_id: &task.id,
                values: values.as_ref().map(|values| ProposedProgress {
                    percent_complete: values.percent_complete,
                    actual_start: values.actual_start.clone(),
                    actual_finish: values.actual_finish.clone(),
                }),
            },
        )?;
        match values {
            Some(values) => {
                task.percent_complete = values.percent_complete;
                task.actual_start = values.actual_start;
                task.actual_finish = values.actual_finish;
            }
            None => {
                task.percent_complete = None;
                task.actual_start = None;
                task.actual_finish = None;
            }
        }
        task.updated_at = updated_at.into();
        task.version += 1;
        transaction.execute(
            "UPDATE tasks SET percent_complete = ?1, actual_start = ?2, actual_finish = ?3, updated_at = ?4, version = ?5 WHERE id = ?6",
            params![
                task.percent_complete,
                task.actual_start,
                task.actual_finish,
                task.updated_at,
                task.version,
                task.id
            ],
        )?;
        transaction.execute(
            "UPDATE jobs SET updated_at = ?1, version = ?2 WHERE id = ?3",
            params![updated_at, job_version + 1, task.job_id],
        )?;
        write_audit_record(&transaction, context, updated_at, "updated task progress")?;
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
        let current = require_draft_job(
            &transaction,
            &request.job_id,
            Some(request.expected_job_version),
        )?;
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
        let current = require_draft_job(
            &transaction,
            &request.job_id,
            Some(request.expected_job_version),
        )?;
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
        if !migration_applied(&connection, 5)? {
            if database_existed {
                self.back_up_before_migration(&connection, 5)?;
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "ALTER TABLE tasks ADD COLUMN start_no_earlier_than TEXT;
                 ALTER TABLE tasks ADD COLUMN finish_no_later_than TEXT;
                 INSERT INTO schema_migrations (version, applied_at)
                 VALUES (5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
            transaction.commit()?;
        }
        if !migration_applied(&connection, 6)? {
            if database_existed {
                self.back_up_before_migration(&connection, 6)?;
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Nullable data-date/progress columns. NULL percent means unstatused (0
            // with no actuals); canonical YYYY-MM-DD text is validated on write.
            transaction.execute_batch(
                "ALTER TABLE jobs ADD COLUMN data_date TEXT;
                 ALTER TABLE tasks ADD COLUMN percent_complete INTEGER;
                 ALTER TABLE tasks ADD COLUMN actual_start TEXT;
                 ALTER TABLE tasks ADD COLUMN actual_finish TEXT;
                 INSERT INTO schema_migrations (version, applied_at)
                 VALUES (6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
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

const REQUIRED_BACKUP_TABLES: [&str; 5] = [
    "schema_migrations",
    "jobs",
    "tasks",
    "command_log",
    "task_dependencies",
];

#[derive(Clone, Copy)]
struct ColumnSpec {
    name: &'static str,
    data_type: &'static str,
    not_null: bool,
    primary_key_position: i64,
}

const SCHEMA_MIGRATION_COLUMNS: [ColumnSpec; 2] = [
    ColumnSpec {
        name: "version",
        data_type: "INTEGER",
        not_null: false,
        primary_key_position: 1,
    },
    ColumnSpec {
        name: "applied_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
];
const JOB_COLUMNS: [ColumnSpec; 9] = [
    ColumnSpec {
        name: "id",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 1,
    },
    ColumnSpec {
        name: "name",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "status",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "timezone",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "created_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "updated_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "version",
        data_type: "INTEGER",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "schedule_start",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "calendar_json",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
];
const JOB_V6_COLUMNS: [ColumnSpec; 10] = [
    JOB_COLUMNS[0],
    JOB_COLUMNS[1],
    JOB_COLUMNS[2],
    JOB_COLUMNS[3],
    JOB_COLUMNS[4],
    JOB_COLUMNS[5],
    JOB_COLUMNS[6],
    JOB_COLUMNS[7],
    JOB_COLUMNS[8],
    ColumnSpec {
        name: "data_date",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
];
const TASK_COLUMNS: [ColumnSpec; 9] = [
    ColumnSpec {
        name: "id",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 1,
    },
    ColumnSpec {
        name: "job_id",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "parent_task_id",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "sort_key",
        data_type: "INTEGER",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "name",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "created_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "updated_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "version",
        data_type: "INTEGER",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "duration_minutes",
        data_type: "INTEGER",
        not_null: false,
        primary_key_position: 0,
    },
];
const TASK_V5_COLUMNS: [ColumnSpec; 11] = [
    ColumnSpec {
        name: "id",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 1,
    },
    ColumnSpec {
        name: "job_id",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "parent_task_id",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "sort_key",
        data_type: "INTEGER",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "name",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "created_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "updated_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "version",
        data_type: "INTEGER",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "duration_minutes",
        data_type: "INTEGER",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "start_no_earlier_than",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "finish_no_later_than",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
];
const TASK_V6_COLUMNS: [ColumnSpec; 14] = [
    TASK_V5_COLUMNS[0],
    TASK_V5_COLUMNS[1],
    TASK_V5_COLUMNS[2],
    TASK_V5_COLUMNS[3],
    TASK_V5_COLUMNS[4],
    TASK_V5_COLUMNS[5],
    TASK_V5_COLUMNS[6],
    TASK_V5_COLUMNS[7],
    TASK_V5_COLUMNS[8],
    TASK_V5_COLUMNS[9],
    TASK_V5_COLUMNS[10],
    ColumnSpec {
        name: "percent_complete",
        data_type: "INTEGER",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "actual_start",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "actual_finish",
        data_type: "TEXT",
        not_null: false,
        primary_key_position: 0,
    },
];
const COMMAND_LOG_COLUMNS: [ColumnSpec; 5] = [
    ColumnSpec {
        name: "command_id",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 1,
    },
    ColumnSpec {
        name: "actor",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "client_name",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "created_at",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "summary",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
];
const DEPENDENCY_COLUMNS: [ColumnSpec; 4] = [
    ColumnSpec {
        name: "job_id",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 0,
    },
    ColumnSpec {
        name: "predecessor_task_id",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 1,
    },
    ColumnSpec {
        name: "successor_task_id",
        data_type: "TEXT",
        not_null: true,
        primary_key_position: 2,
    },
    ColumnSpec {
        name: "lag_minutes",
        data_type: "INTEGER",
        not_null: true,
        primary_key_position: 0,
    },
];

fn create_incomplete_backup_path(destination: &Path) -> Result<PathBuf, ApplicationError> {
    let file_name = destination.file_name().and_then(|name| name.to_str());
    let Some(file_name) = file_name.filter(|name| !name.is_empty()) else {
        return Err(ApplicationError::InvalidInput {
            field: "destination",
            message: "must name a backup file".into(),
        });
    };
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));

    for _ in 0..3 {
        let incomplete = parent.join(format!(".{file_name}.{}.incomplete", Uuid::now_v7()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&incomplete)
        {
            Ok(_) => return Ok(incomplete),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ApplicationError::BackupFailed),
        }
    }
    Err(ApplicationError::BackupFailed)
}

/// Publishes only after validation. A same-directory hard link is an atomic
/// no-clobber create, so an independently created destination is never
/// overwritten or removed by this operation.
fn publish_incomplete_backup(
    incomplete: &Path,
    destination: &Path,
) -> Result<(), ApplicationError> {
    match std::fs::hard_link(incomplete, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(ApplicationError::BackupDestinationExists)
        }
        Err(_) => Err(ApplicationError::BackupFailed),
    }
}

fn cleanup_owned_incomplete_backup(incomplete: &Path) -> Result<(), ApplicationError> {
    for path in owned_incomplete_paths(incomplete) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ApplicationError::BackupFailed),
        }
    }
    Ok(())
}

fn owned_incomplete_paths(incomplete: &Path) -> [PathBuf; 4] {
    let file_name = incomplete.file_name().unwrap_or_default();
    let sidecar = |suffix: &str| {
        let mut name = file_name.to_os_string();
        name.push(suffix);
        incomplete.with_file_name(name)
    };
    [
        incomplete.to_path_buf(),
        sidecar("-journal"),
        sidecar("-wal"),
        sidecar("-shm"),
    ]
}

fn create_restore_staging_directory(target: &Path) -> Result<PathBuf, ApplicationError> {
    let parent = target.parent().ok_or(ApplicationError::RestoreFailed)?;
    if !parent.is_dir() {
        return Err(ApplicationError::RestoreFailed);
    }
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(ApplicationError::RestoreFailed)?;

    for _ in 0..3 {
        let staging = parent.join(format!(".{name}.{}.restore-staging", Uuid::now_v7()));
        match std::fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ApplicationError::RestoreFailed),
        }
    }
    Err(ApplicationError::RestoreFailed)
}

/// Atomically claims a fresh target directory. `symlink_metadata` deliberately
/// treats a dangling symlink as an existing entry, and `create_dir` closes the
/// check/create race without replacing a contender's directory.
fn reserve_fresh_restore_target(target: &Path) -> Result<(), ApplicationError> {
    match std::fs::symlink_metadata(target) {
        Ok(_) => return Err(ApplicationError::RestoreTargetExists),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ApplicationError::RestoreFailed),
    }
    match std::fs::create_dir(target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(ApplicationError::RestoreTargetExists)
        }
        Err(_) => Err(ApplicationError::RestoreFailed),
    }
}

fn cleanup_owned_restore_staging(staging: &Path) -> Result<(), ApplicationError> {
    match std::fs::remove_dir_all(staging) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ApplicationError::RestoreFailed),
    }
}

/// Removes a target reservation only while it is empty. Before publication the
/// operation has no ownership of canonical database names inside that directory.
fn cleanup_empty_restore_reservation(target: &Path) -> Result<(), ApplicationError> {
    let metadata =
        std::fs::symlink_metadata(target).map_err(|_| ApplicationError::RestoreFailed)?;
    if !metadata.file_type().is_dir() {
        return Err(ApplicationError::RestoreFailed);
    }
    if std::fs::read_dir(target)
        .map_err(|_| ApplicationError::RestoreFailed)?
        .next()
        .is_some()
    {
        return Err(ApplicationError::RestoreFailed);
    }
    std::fs::remove_dir(target).map_err(|_| ApplicationError::RestoreFailed)
}

fn cleanup_unpublished_restore(staging: &Path, target: &Path) -> Result<(), ApplicationError> {
    let staging_result = cleanup_owned_restore_staging(staging);
    let target_result = cleanup_empty_restore_reservation(target);
    if staging_result.is_err() || target_result.is_err() {
        return Err(ApplicationError::RestoreFailed);
    }
    Ok(())
}

fn same_regular_file(staged: &Path, target: &Path) -> bool {
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {}
        _ => return false,
    }
    match std::fs::metadata(staged) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        _ => return false,
    }
    same_file::is_same_file(staged, target).unwrap_or(false)
}

/// Hard-linking is atomic and no-clobber: an unexpected file inside the
/// reserved target makes the restore fail rather than replacing it.
fn publish_restored_database(staged: &Path, target: &Path) -> Result<(), ApplicationError> {
    std::fs::hard_link(staged, target).map_err(|_| ApplicationError::RestoreFailed)?;
    OpenOptions::new()
        .write(true)
        .open(target)
        .and_then(|file| file.sync_all())
        .map_err(|_| ApplicationError::RestoreFailed)
}

/// Copies an already-verified, static snapshot through SQLite's online backup
/// API rather than treating the database file as an application-level blob.
fn copy_verified_snapshot(source_path: &Path, destination: &Path) -> Result<(), ApplicationError> {
    let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| ApplicationError::RestoreFailed)?;
    let mut target = Connection::open(destination).map_err(|_| ApplicationError::RestoreFailed)?;
    target
        .execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")
        .map_err(|_| ApplicationError::RestoreFailed)?;
    {
        let backup =
            Backup::new(&source, &mut target).map_err(|_| ApplicationError::RestoreFailed)?;
        backup
            .run_to_completion(100, Duration::from_millis(1), None)
            .map_err(|_| ApplicationError::RestoreFailed)?;
    }
    drop(target);
    OpenOptions::new()
        .write(true)
        .open(destination)
        .and_then(|file| file.sync_all())
        .map_err(|_| ApplicationError::RestoreFailed)
}

#[derive(Clone, Copy, Debug)]
struct VerifiedSnapshot {
    job_count: i64,
    task_count: i64,
    dependency_count: i64,
    command_log_count: i64,
}

/// Opens the completed snapshot read-only. It never runs the application's
/// migration path, so verification cannot change a user-selected backup.
fn verify_backup(destination: &Path) -> Result<VerifiedSnapshot, ApplicationError> {
    let connection = Connection::open_with_flags(destination, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;

    let integrity_rows = connection
        .prepare("PRAGMA integrity_check")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    if integrity_rows.as_slice() != ["ok"] {
        return Err(ApplicationError::BackupVerificationFailed);
    }

    let has_foreign_key_violation: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    if has_foreign_key_violation {
        return Err(ApplicationError::BackupVerificationFailed);
    }

    let schema_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    if !matches!(schema_version, 4..=6) {
        return Err(ApplicationError::BackupVerificationFailed);
    }
    let required_table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN (?1, ?2, ?3, ?4, ?5)",
            params![
                REQUIRED_BACKUP_TABLES[0],
                REQUIRED_BACKUP_TABLES[1],
                REQUIRED_BACKUP_TABLES[2],
                REQUIRED_BACKUP_TABLES[3],
                REQUIRED_BACKUP_TABLES[4],
            ],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    if required_table_count != REQUIRED_BACKUP_TABLES.len() as i64 {
        return Err(ApplicationError::BackupVerificationFailed);
    }
    verify_supported_schema(&connection, schema_version)?;
    if schema_version >= 5 {
        verify_v5_constraint_domain(&connection)?;
    }
    if schema_version == 6 {
        verify_v6_progress_domain(&connection)?;
    }

    // These bounded counts prove the core domain tables are readable without
    // exposing any customer or job content in the result or error surface.
    let job_count = bounded_table_count(&connection, "jobs")?;
    let task_count = bounded_table_count(&connection, "tasks")?;
    let dependency_count = bounded_table_count(&connection, "task_dependencies")?;
    let command_log_count = bounded_table_count(&connection, "command_log")?;
    Ok(VerifiedSnapshot {
        job_count,
        task_count,
        dependency_count,
        command_log_count,
    })
}

fn verify_supported_schema(
    connection: &Connection,
    schema_version: i64,
) -> Result<(), ApplicationError> {
    let migrations = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    let expected_migrations: &[i64] = match schema_version {
        4 => &[1, 2, 3, 4],
        5 => &[1, 2, 3, 4, 5],
        _ => &[1, 2, 3, 4, 5, 6],
    };
    if migrations != expected_migrations {
        return Err(ApplicationError::BackupVerificationFailed);
    }

    verify_table_columns(connection, "schema_migrations", &SCHEMA_MIGRATION_COLUMNS)?;
    verify_table_columns(
        connection,
        "jobs",
        if schema_version == 6 {
            &JOB_V6_COLUMNS[..]
        } else {
            &JOB_COLUMNS[..]
        },
    )?;
    verify_table_columns(
        connection,
        "tasks",
        match schema_version {
            4 => &TASK_COLUMNS[..],
            5 => &TASK_V5_COLUMNS[..],
            _ => &TASK_V6_COLUMNS[..],
        },
    )?;
    verify_table_columns(connection, "command_log", &COMMAND_LOG_COLUMNS)?;
    verify_table_columns(connection, "task_dependencies", &DEPENDENCY_COLUMNS)?;
    verify_foreign_keys(
        connection,
        "tasks",
        [
            ("jobs", "job_id", "id"),
            ("tasks", "job_id", "job_id"),
            ("tasks", "parent_task_id", "id"),
        ],
    )?;
    verify_foreign_keys(
        connection,
        "task_dependencies",
        [
            ("jobs", "job_id", "id"),
            ("tasks", "job_id", "job_id"),
            ("tasks", "predecessor_task_id", "id"),
            ("tasks", "job_id", "job_id"),
            ("tasks", "successor_task_id", "id"),
        ],
    )?;
    for (name, expected_sql) in [
        (
            "tasks_job_parent_order",
            "createindextasks_job_parent_orderontasksjob_idparent_task_idsort_keyid",
        ),
        (
            "tasks_root_sibling_order",
            "createuniqueindextasks_root_sibling_orderontasksjob_idsort_keywhereparent_task_idisnull",
        ),
        (
            "tasks_child_sibling_order",
            "createuniqueindextasks_child_sibling_orderontasksjob_idparent_task_idsort_keywhereparent_task_idisnotnull",
        ),
        (
            "task_dependencies_job_successor",
            "createindextask_dependencies_job_successorontask_dependenciesjob_idsuccessor_task_id",
        ),
    ] {
        let sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::BackupVerificationFailed)?;
        if !matches_supported_schema_sql(&sql, expected_sql) {
            return Err(ApplicationError::BackupVerificationFailed);
        }
    }
    for (table, expected_sql) in [
        (
            "schema_migrations",
            "createtableschema_migrationsversionintegerprimarykeyapplied_attextnotnull",
        ),
        (
            "jobs",
            if schema_version == 6 {
                "createtablejobsidtextprimarykeynametextnotnullstatustextnotnulltimezonetextnotnullcreated_attextnotnullupdated_attextnotnullversionintegernotnullcheckversion>0schedule_starttextcalendar_jsontextnotnulldefaultdata_datetext"
            } else {
                "createtablejobsidtextprimarykeynametextnotnullstatustextnotnulltimezonetextnotnullcreated_attextnotnullupdated_attextnotnullversionintegernotnullcheckversion>0schedule_starttextcalendar_jsontextnotnulldefault"
            },
        ),
        (
            "tasks",
            match schema_version {
                4 => "createtabletasksidtextprimarykeyjob_idtextnotnullparent_task_idtextsort_keyintegernotnullchecksort_key>=0nametextnotnullcreated_attextnotnullupdated_attextnotnullversionintegernotnullcheckversion>0duration_minutesintegercheckduration_minutes>=0uniquejob_ididforeignkeyjob_idreferencesjobsidondeleterestrictforeignkeyjob_idparent_task_idreferencestasksjob_ididondeleterestrict",
                5 => "createtabletasksidtextprimarykeyjob_idtextnotnullparent_task_idtextsort_keyintegernotnullchecksort_key>=0nametextnotnullcreated_attextnotnullupdated_attextnotnullversionintegernotnullcheckversion>0duration_minutesintegercheckduration_minutes>=0start_no_earlier_thantextfinish_no_later_thantextuniquejob_ididforeignkeyjob_idreferencesjobsidondeleterestrictforeignkeyjob_idparent_task_idreferencestasksjob_ididondeleterestrict",
                _ => "createtabletasksidtextprimarykeyjob_idtextnotnullparent_task_idtextsort_keyintegernotnullchecksort_key>=0nametextnotnullcreated_attextnotnullupdated_attextnotnullversionintegernotnullcheckversion>0duration_minutesintegercheckduration_minutes>=0start_no_earlier_thantextfinish_no_later_thantextpercent_completeintegeractual_starttextactual_finishtextuniquejob_ididforeignkeyjob_idreferencesjobsidondeleterestrictforeignkeyjob_idparent_task_idreferencestasksjob_ididondeleterestrict",
            },
        ),
        (
            "command_log",
            "createtablecommand_logcommand_idtextnotnullprimarykeychecklengthcommand_idbetween1and128actortextnotnullcheckactorinuseragentimportclient_nametextnotnullchecklengthclient_namebetween1and120created_attextnotnullsummarytextnotnullchecklengthsummary<=240",
        ),
        (
            "task_dependencies",
            "createtabletask_dependenciesjob_idtextnotnullpredecessor_task_idtextnotnullsuccessor_task_idtextnotnulllag_minutesintegernotnullchecklag_minutes>=0primarykeypredecessor_task_idsuccessor_task_idforeignkeyjob_idreferencesjobsidondeleterestrictforeignkeyjob_idpredecessor_task_idreferencestasksjob_ididondeleterestrictforeignkeyjob_idsuccessor_task_idreferencestasksjob_ididondeleterestrictcheckpredecessor_task_id<>successor_task_id",
        ),
    ] {
        let sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::BackupVerificationFailed)?;
        if !matches_supported_schema_sql(&sql, expected_sql) {
            return Err(ApplicationError::BackupVerificationFailed);
        }
    }
    let task_query = match schema_version {
        4 => "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, created_at, updated_at, version FROM tasks ORDER BY job_id, parent_task_id, sort_key, id LIMIT 1",
        5 => "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, created_at, updated_at, version FROM tasks ORDER BY job_id, parent_task_id, sort_key, id LIMIT 1",
        _ => "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version FROM tasks ORDER BY job_id, parent_task_id, sort_key, id LIMIT 1",
    };
    let job_query = if schema_version == 6 {
        "SELECT id, name, status, timezone, schedule_start, calendar_json, data_date, created_at, updated_at, version FROM jobs ORDER BY created_at DESC, id DESC LIMIT 1"
    } else {
        "SELECT id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version FROM jobs ORDER BY created_at DESC, id DESC LIMIT 1"
    };
    for query in [
        job_query,
        task_query,
        "SELECT job_id, predecessor_task_id, successor_task_id, lag_minutes FROM task_dependencies ORDER BY job_id, predecessor_task_id, successor_task_id LIMIT 1",
        "SELECT command_id, actor, client_name, created_at, summary FROM command_log ORDER BY command_id LIMIT 1",
    ] {
        connection
            .prepare(query)
            .and_then(|mut statement| statement.query([]).map(|_| ()))
            .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    }
    Ok(())
}

/// Backup preflight is deliberately read-only. For v5, schema shape alone is
/// insufficient: constraints are canonical inputs and must be parseable leaf
/// values before an untrusted snapshot can be copied into an owned target.
fn verify_v5_constraint_domain(connection: &Connection) -> Result<(), ApplicationError> {
    let mut statement = connection
        .prepare(
            "SELECT task.id, task.duration_minutes, task.start_no_earlier_than,
                    task.finish_no_later_than,
                    EXISTS(SELECT 1 FROM tasks child WHERE child.parent_task_id = task.id)
             FROM tasks task
             WHERE task.start_no_earlier_than IS NOT NULL OR task.finish_no_later_than IS NOT NULL",
        )
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    for row in rows {
        let (duration, start_no_earlier_than, finish_no_later_than, has_children) =
            row.map_err(|_| ApplicationError::BackupVerificationFailed)?;
        if duration.is_none() || has_children {
            return Err(ApplicationError::BackupVerificationFailed);
        }
        for value in [start_no_earlier_than, finish_no_later_than]
            .into_iter()
            .flatten()
        {
            parse_canonical_constraint_date(&value)
                .map_err(|_| ApplicationError::BackupVerificationFailed)?;
        }
    }
    Ok(())
}

/// Read-only v6 preflight for the data-date/progress columns. It checks that a
/// statused row is a leaf with a duration, its percent (when present) is in
/// `[0, 100]`, and every stored date parses canonically. Deeper cross-field
/// scheduler semantics stay owned by the pure scheduler.
fn verify_v6_progress_domain(connection: &Connection) -> Result<(), ApplicationError> {
    let data_date: Option<String> = connection
        .query_row(
            "SELECT data_date FROM jobs WHERE data_date IS NOT NULL LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ApplicationError::BackupVerificationFailed)?
        .flatten();
    if let Some(value) = data_date {
        parse_canonical_constraint_date(&value)
            .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    }
    let mut statement = connection
        .prepare(
            "SELECT task.duration_minutes, task.percent_complete, task.actual_start,
                    task.actual_finish,
                    EXISTS(SELECT 1 FROM tasks child WHERE child.parent_task_id = task.id)
             FROM tasks task
             WHERE task.percent_complete IS NOT NULL
                OR task.actual_start IS NOT NULL
                OR task.actual_finish IS NOT NULL",
        )
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    for row in rows {
        let (duration, percent, actual_start, actual_finish, has_children) =
            row.map_err(|_| ApplicationError::BackupVerificationFailed)?;
        if duration.is_none() || has_children {
            return Err(ApplicationError::BackupVerificationFailed);
        }
        if let Some(percent) = percent {
            if !(0..=100).contains(&percent) {
                return Err(ApplicationError::BackupVerificationFailed);
            }
        }
        for value in [actual_start, actual_finish].into_iter().flatten() {
            parse_canonical_constraint_date(&value)
                .map_err(|_| ApplicationError::BackupVerificationFailed)?;
        }
    }
    Ok(())
}

fn verify_table_columns(
    connection: &Connection,
    table: &str,
    expected: &[ColumnSpec],
) -> Result<(), ApplicationError> {
    let actual = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?.to_ascii_uppercase(),
                        row.get::<_, i64>(3)? != 0,
                        row.get::<_, i64>(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    if actual.len() != expected.len()
        || actual.iter().zip(expected).any(|(actual, expected)| {
            actual.0 != expected.name
                || actual.1 != expected.data_type
                || actual.2 != expected.not_null
                || actual.3 != expected.primary_key_position
        })
    {
        return Err(ApplicationError::BackupVerificationFailed);
    }
    Ok(())
}

fn verify_foreign_keys<const N: usize>(
    connection: &Connection,
    table: &str,
    expected: [(&str, &str, &str); N],
) -> Result<(), ApplicationError> {
    let mut actual = connection
        .prepare(&format!("PRAGMA foreign_key_list({table})"))
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    actual.sort();
    let mut expected = expected
        .into_iter()
        .map(|(table, from, to)| (table.to_owned(), from.to_owned(), to.to_owned()))
        .collect::<Vec<_>>();
    expected.sort();
    if actual != expected {
        return Err(ApplicationError::BackupVerificationFailed);
    }
    Ok(())
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || *character == '_'
                || *character == '>'
                || *character == '<'
                || *character == '='
        })
        .flat_map(char::to_lowercase)
        .collect()
}

fn matches_supported_schema_sql(sql: &str, expected: &str) -> bool {
    !sql.contains("--")
        && !sql.contains("/*")
        && !sql.contains("*/")
        && normalize_schema_sql(sql) == expected
}

fn bounded_table_count(connection: &Connection, table: &str) -> Result<i64, ApplicationError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|_| ApplicationError::BackupVerificationFailed)
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

/// Guards every ordinary job write; archive and restore use their dedicated transition path.
fn require_draft_job(
    transaction: &Transaction<'_>,
    job_id: &str,
    expected_version: Option<i64>,
) -> Result<i64, ApplicationError> {
    let (status, version) = transaction
        .query_row(
            "SELECT status, version FROM jobs WHERE id = ?1",
            [job_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
        .ok_or_else(|| ApplicationError::NotFound {
            resource: "job",
            id: job_id.into(),
        })?;
    if let Some(expected) = expected_version {
        if version != expected {
            return Err(ApplicationError::VersionConflict {
                resource: "job",
                id: job_id.into(),
                expected,
                current: version,
            });
        }
    }
    match JobStatus::from_database_value(&status) {
        Some(JobStatus::Draft) => Ok(version),
        Some(JobStatus::Archived) => Err(ApplicationError::ValidationFailed {
            code: "job_archived",
            field: "jobId",
            message: "restore the job before changing its tasks or schedule".into(),
        }),
        None => Err(ApplicationError::InvalidStoredData(format!(
            "job {job_id} has unsupported status {status}"
        ))),
    }
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
        start_no_earlier_than: row.get(6)?,
        finish_no_later_than: row.get(7)?,
        percent_complete: row.get(8)?,
        actual_start: row.get(9)?,
        actual_finish: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        version: row.get(13)?,
    })
}

fn read_job(connection: &Connection, job_id: &str) -> Result<Job, ApplicationError> {
    let (id, name, status, timezone, schedule_start, calendar_json, data_date, created_at, updated_at, version) = connection.query_row(
        "SELECT id, name, status, timezone, schedule_start, calendar_json, data_date, created_at, updated_at, version FROM jobs WHERE id = ?1",
        [job_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?, row.get::<_, i64>(9)?)),
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
        data_date,
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
            id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version, path
         ) AS (
            SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version,
                   printf('%020d:%s', sort_key, id)
            FROM tasks
            WHERE job_id = ?1 AND parent_task_id IS NULL
            UNION ALL
            SELECT child.id, child.job_id, child.parent_task_id, child.sort_key, child.name,
                   child.duration_minutes, child.start_no_earlier_than, child.finish_no_later_than, child.percent_complete, child.actual_start, child.actual_finish, child.created_at, child.updated_at, child.version,
                   parent.path || '/' || printf('%020d:%s', child.sort_key, child.id)
            FROM tasks child
            JOIN task_tree parent ON child.parent_task_id = parent.id
            WHERE child.job_id = ?1
         )
         SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version
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
            "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version
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

/// Progress column values proposed for one task before they are persisted.
struct ProposedProgress {
    percent_complete: Option<i64>,
    actual_start: Option<String>,
    actual_finish: Option<String>,
}

/// The single canonical edit substituted into the proposed schedule. Every write
/// path validates the complete job through this seam so no committed edit can
/// leave a schedule the pure scheduler would reject.
enum ProposedEdit<'a> {
    /// A leaf constraint or duration change; the candidate carries the new values.
    Task(&'a Task),
    /// A job data-date change; `None` clears the data date.
    DataDate(Option<&'a str>),
    /// A leaf progress change; `None` clears the row back to unstatused.
    Progress {
        task_id: &'a str,
        values: Option<ProposedProgress>,
    },
}

/// Builds the complete proposed schedule from persisted constraints, progress,
/// and the job data date, substituting a single candidate edit, and runs the
/// pure scheduler before any canonical row is written. This keeps every
/// scheduling-input write atomic with the full schedule the scheduler accepts.
fn validate_proposed_schedule(
    transaction: &Transaction<'_>,
    job_id: &str,
    edit: &ProposedEdit,
) -> Result<(), ApplicationError> {
    let (schedule_start, calendar_json, stored_data_date): (
        Option<String>,
        String,
        Option<String>,
    ) = transaction.query_row(
        "SELECT schedule_start, calendar_json, data_date FROM jobs WHERE id = ?1",
        [job_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let schedule_start = schedule_start.ok_or(ApplicationError::ValidationFailed {
        code: "schedule_start_required",
        field: "scheduleStart",
        message: "set a schedule start before updating the schedule".into(),
    })?;
    let schedule_start = NaiveDate::parse_from_str(&schedule_start, "%Y-%m-%d").map_err(|_| {
        ApplicationError::InvalidStoredData("job has invalid schedule start".into())
    })?;
    let calendar = serde_json::from_str(&calendar_json)
        .map_err(|_| ApplicationError::InvalidStoredData("job has invalid calendar".into()))?;

    // Resolve the proposed data date: the DataDate edit overrides the stored value.
    let data_date_value = match edit {
        ProposedEdit::DataDate(value) => value.map(str::to_owned),
        _ => stored_data_date,
    };
    let data_date = data_date_value
        .as_deref()
        .map(parse_stored_constraint)
        .transpose()?;

    let mut tasks = Vec::new();
    let mut constraints = Vec::new();
    let mut entries = Vec::new();
    let mut statement = transaction.prepare(
        "SELECT id, parent_task_id, duration_minutes, start_no_earlier_than, finish_no_later_than,
                percent_complete, actual_start, actual_finish
         FROM tasks WHERE job_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([job_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            parent_task_id,
            mut duration_minutes,
            mut snet,
            mut fnlt,
            mut percent,
            mut actual_start,
            mut actual_finish,
        ) = row?;
        match edit {
            ProposedEdit::Task(candidate) if candidate.id == id => {
                duration_minutes = candidate.duration_minutes;
                snet = candidate.start_no_earlier_than.clone();
                fnlt = candidate.finish_no_later_than.clone();
            }
            ProposedEdit::Progress { task_id, values } if *task_id == id => match values {
                Some(values) => {
                    percent = values.percent_complete;
                    actual_start = values.actual_start.clone();
                    actual_finish = values.actual_finish.clone();
                }
                None => {
                    percent = None;
                    actual_start = None;
                    actual_finish = None;
                }
            },
            _ => {}
        }
        tasks.push(ScheduleTask {
            id: id.clone(),
            parent_task_id,
            duration_minutes,
        });
        if snet.is_some() || fnlt.is_some() {
            constraints.push(TaskConstraint {
                task_id: id.clone(),
                start_no_earlier_than: snet.as_deref().map(parse_stored_constraint).transpose()?,
                finish_no_later_than: fnlt.as_deref().map(parse_stored_constraint).transpose()?,
            });
        }
        // Canonical mapping: unstatused rows (percent NULL/0 with no actuals) are
        // omitted, matching the scheduler's treatment of absent progress.
        let has_signal =
            percent.unwrap_or(0) != 0 || actual_start.is_some() || actual_finish.is_some();
        if has_signal {
            let percent_complete = u8::try_from(percent.unwrap_or(0)).map_err(|_| {
                ApplicationError::InvalidStoredData("task has invalid percent complete".into())
            })?;
            entries.push(TaskProgress {
                task_id: id,
                percent_complete,
                actual_start: actual_start
                    .as_deref()
                    .map(parse_stored_constraint)
                    .transpose()?,
                actual_finish: actual_finish
                    .as_deref()
                    .map(parse_stored_constraint)
                    .transpose()?,
            });
        }
    }
    let mut statement = transaction.prepare(
        "SELECT predecessor_task_id, successor_task_id, lag_minutes FROM task_dependencies WHERE job_id = ?1 ORDER BY predecessor_task_id, successor_task_id",
    )?;
    let dependencies = statement
        .query_map([job_id], |row| {
            Ok(ScheduleDependency {
                predecessor_task_id: row.get(0)?,
                successor_task_id: row.get(1)?,
                lag_minutes: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    calculate_schedule_with_progress(
        &ScheduleInput {
            schedule_start,
            calendar,
            tasks,
            dependencies,
        },
        &constraints,
        &ScheduleProgress { data_date, entries },
    )
    .map_err(|error| ApplicationError::ValidationFailed {
        code: error.code(),
        field: "schedule",
        message: error.to_string(),
    })?;
    Ok(())
}

fn parse_stored_constraint(value: &str) -> Result<NaiveDate, ApplicationError> {
    parse_canonical_constraint_date(value)
        .map_err(|_| ApplicationError::InvalidStoredData("task has invalid constraint".into()))
}

fn parse_canonical_constraint_date(value: &str) -> Result<NaiveDate, ()> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| ())?;
    (date.format("%Y-%m-%d").to_string() == value)
        .then_some(date)
        .ok_or(())
}

fn load_siblings(
    transaction: &Transaction<'_>,
    job_id: &str,
    parent_task_id: Option<&str>,
) -> Result<Vec<Task>, ApplicationError> {
    let mut statement = transaction.prepare(
        "SELECT id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, percent_complete, actual_start, actual_finish, created_at, updated_at, version
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

#[cfg(test)]
mod backup_verification_tests {
    use super::{
        cleanup_empty_restore_reservation, cleanup_unpublished_restore,
        create_restore_staging_directory, owned_incomplete_paths, publish_restored_database,
        reserve_fresh_restore_target, verify_backup, SqliteStore,
    };
    use crate::error::ApplicationError;
    use rusqlite::Connection;

    #[test]
    fn malformed_schema_is_rejected_without_migrating_the_backup() {
        let temp = tempfile::tempdir().expect("temporary backup directory");
        let path = temp.path().join("old-schema.sqlite3");
        let connection = Connection::open(&path).expect("create malformed backup");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 INSERT INTO schema_migrations VALUES (3, '2026-08-16T00:00:00.000Z');
                 CREATE TABLE jobs (id TEXT PRIMARY KEY);
                 CREATE TABLE tasks (id TEXT PRIMARY KEY);
                 CREATE TABLE command_log (command_id TEXT PRIMARY KEY);
                 CREATE TABLE task_dependencies (predecessor_task_id TEXT PRIMARY KEY);",
            )
            .expect("create old schema");

        let error = verify_backup(&path).expect_err("reject old schema");
        assert_eq!(error.kind(), "backup_verification_failed");
        assert_eq!(error.to_string(), "backup verification failed");

        let unchanged = Connection::open(&path).expect("inspect unmodified backup");
        let version: i64 = unchanged
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("read original version");
        assert_eq!(version, 3);
    }

    #[test]
    fn missing_required_table_is_rejected_with_a_bounded_error() {
        let temp = tempfile::tempdir().expect("temporary backup directory");
        let path = temp.path().join("foreign-schema.sqlite3");
        let connection = Connection::open(&path).expect("create malformed backup");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 INSERT INTO schema_migrations VALUES (4, '2026-08-16T00:00:00.000Z');
                 CREATE TABLE jobs (id TEXT PRIMARY KEY);
                 CREATE TABLE tasks (id TEXT PRIMARY KEY);
                 CREATE TABLE command_log (command_id TEXT PRIMARY KEY);",
            )
            .expect("create foreign schema");

        let error = verify_backup(&path).expect_err("reject missing table");
        assert_eq!(error.kind(), "backup_verification_failed");
        assert_eq!(error.to_string(), "backup verification failed");
    }

    #[test]
    fn verification_failure_removes_only_the_owned_incomplete_snapshot() {
        let temp = tempfile::tempdir().expect("temporary backup directory");
        let source = temp.path().join("source.sqlite3");
        let destination = temp.path().join("published.sqlite3");
        let store = SqliteStore::open(&source).expect("create source database");

        let error = store
            .create_verified_backup_with(&destination, |incomplete| {
                let owned = owned_incomplete_paths(incomplete);
                assert!(
                    !owned[1].exists(),
                    "the unpublished backup must not retain a rollback journal"
                );
                std::fs::write(&owned[1], []).expect("create owned journal sidecar");
                std::fs::write(&owned[2], []).expect("create owned WAL sidecar");
                std::fs::write(&owned[3], []).expect("create owned SHM sidecar");
                Err(ApplicationError::BackupVerificationFailed)
            })
            .expect_err("verification failure");
        assert_eq!(error.kind(), "backup_verification_failed");
        assert!(!destination.exists());
        assert!(
            std::fs::read_dir(temp.path())
                .expect("list backup directory")
                .all(|entry| !entry
                    .expect("directory entry")
                    .file_name()
                    .to_string_lossy()
                    .contains("published.sqlite3")),
            "owned incomplete snapshot should be removed"
        );
        assert!(source.exists(), "the live source database remains intact");
    }

    #[test]
    fn restore_cleanup_never_removes_an_unexpected_target_entry() {
        let temp = tempfile::tempdir().expect("temporary restore directory");
        let target = temp.path().join("reserved-target");
        std::fs::create_dir(&target).expect("reserve target");
        let sentinel = target.join("contender-file");
        std::fs::write(&sentinel, b"do not remove").expect("write contender file");

        let error = cleanup_empty_restore_reservation(&target).expect_err("reject foreign entry");
        assert_eq!(error.kind(), "restore_failed");
        assert_eq!(
            std::fs::read(&sentinel).expect("sentinel remains"),
            b"do not remove"
        );
        assert!(target.is_dir());
    }

    #[test]
    fn unpublished_restore_never_removes_a_same_name_contender_database() {
        let temp = tempfile::tempdir().expect("temporary restore directory");
        let target = temp.path().join("reserved-target");
        reserve_fresh_restore_target(&target).expect("reserve target");
        let staging = create_restore_staging_directory(&target).expect("create staging");
        let staged_database = staging.join("contractorproject.sqlite3");
        std::fs::write(&staged_database, b"owned staging database")
            .expect("write staging database");
        let contender = target.join("contractorproject.sqlite3");
        std::fs::write(&contender, b"contender database").expect("write contender database");

        let error = publish_restored_database(&staged_database, &contender)
            .expect_err("no-clobber publication rejects contender");
        assert_eq!(error.kind(), "restore_failed");
        let cleanup = cleanup_unpublished_restore(&staging, &target)
            .expect_err("non-empty reservation is not owned cleanup");
        assert_eq!(cleanup.kind(), "restore_failed");
        assert_eq!(
            std::fs::read(&contender).expect("contender remains"),
            b"contender database"
        );
        assert!(!staging.exists(), "owned staging is still removed");
    }

    #[cfg(unix)]
    #[test]
    fn unpublished_restore_never_removes_a_same_name_contender_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temporary restore directory");
        let target = temp.path().join("reserved-target");
        reserve_fresh_restore_target(&target).expect("reserve target");
        let staging = create_restore_staging_directory(&target).expect("create staging");
        let staged_database = staging.join("contractorproject.sqlite3");
        std::fs::write(&staged_database, b"owned staging database")
            .expect("write staging database");
        let sentinel = temp.path().join("contender.sqlite3");
        std::fs::write(&sentinel, b"contender database").expect("write sentinel");
        let contender = target.join("contractorproject.sqlite3");
        symlink(&sentinel, &contender).expect("create contender symlink");

        publish_restored_database(&staged_database, &contender)
            .expect_err("no-clobber publication rejects symlink");
        cleanup_unpublished_restore(&staging, &target)
            .expect_err("non-empty reservation is not owned cleanup");
        assert!(std::fs::symlink_metadata(&contender)
            .expect("contender remains")
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read(&sentinel).expect("sentinel remains"),
            b"contender database"
        );
        assert!(!staging.exists(), "owned staging is still removed");
    }
}
