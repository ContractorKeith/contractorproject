// Integration coverage for issue #38: durable audited data-date and task
// progress persistence, validated through the pure scheduler before commit.

use contractorproject_lib::application::{ApplicationError, Job, TaskConstraintKind, TaskMutation};
use contractorproject_lib::application::{
    ApplicationService, CommandActor, CommandContext, CreateBackupRequest, CreateJobRequest,
    CreateTaskRequest, UpdateJobDataDateRequest, UpdateScheduleRequest,
    UpdateTaskConstraintRequest, UpdateTaskDurationRequest, UpdateTaskProgressRequest,
    VerifyRestoreRequest,
};
use contractorproject_lib::scheduling::{CalendarWeekday, WorkingCalendar};
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_COMMAND_ID: AtomicU64 = AtomicU64::new(1);

fn command_context() -> CommandContext {
    CommandContext {
        command_id: format!(
            "progress-command-{}",
            NEXT_COMMAND_ID.fetch_add(1, Ordering::Relaxed)
        ),
        actor: CommandActor::Agent,
        client_name: "progress-test".into(),
    }
}

fn working_calendar() -> WorkingCalendar {
    WorkingCalendar {
        working_weekdays: vec![
            CalendarWeekday::Monday,
            CalendarWeekday::Tuesday,
            CalendarWeekday::Wednesday,
            CalendarWeekday::Thursday,
            CalendarWeekday::Friday,
        ],
        workday_start_minute: 480,
        workday_duration_minutes: 480,
    }
}

fn seven_day_calendar() -> WorkingCalendar {
    WorkingCalendar {
        working_weekdays: vec![
            CalendarWeekday::Monday,
            CalendarWeekday::Tuesday,
            CalendarWeekday::Wednesday,
            CalendarWeekday::Thursday,
            CalendarWeekday::Friday,
            CalendarWeekday::Saturday,
            CalendarWeekday::Sunday,
        ],
        workday_start_minute: 480,
        workday_duration_minutes: 480,
    }
}

fn job_calendar_and_version(path: &std::path::Path, job_id: &str) -> (String, i64) {
    Connection::open(path)
        .expect("open job database")
        .query_row(
            "SELECT calendar_json, version FROM jobs WHERE id = ?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read job calendar and version")
}

fn command_log_count(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .expect("open audit database")
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count command log")
}

/// A single leaf task's persisted progress columns and version.
fn task_progress_row(
    path: &std::path::Path,
    task_id: &str,
) -> (Option<i64>, Option<String>, Option<String>, i64) {
    Connection::open(path)
        .expect("open task database")
        .query_row(
            "SELECT percent_complete, actual_start, actual_finish, version FROM tasks WHERE id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read task progress row")
}

fn job_row(path: &std::path::Path, job_id: &str) -> (Option<String>, i64) {
    Connection::open(path)
        .expect("open job database")
        .query_row(
            "SELECT data_date, version FROM jobs WHERE id = ?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read job row")
}

/// Creates a job with a Monday schedule start and one 480-minute leaf task.
fn seed_scheduled_leaf(path: &std::path::Path) -> (ApplicationService, Job, TaskMutation) {
    let service = ApplicationService::open(path).expect("open service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Progress job".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("schedule");
    let task = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Leaf".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");
    let duration = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: task.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: task.task.version,
                expected_job_version: task.job_version,
            },
        )
        .expect("duration");
    let job = job_after(&service, &job.id);
    (service, job, duration)
}

fn job_after(service: &ApplicationService, job_id: &str) -> Job {
    service
        .list_jobs()
        .expect("list jobs")
        .into_iter()
        .find(|job| job.id == job_id)
        .expect("job present")
}

#[test]
fn migration_v6_adds_progress_columns_on_fresh_and_existing_v5_database() {
    // Fresh database opens directly at v6 with the new nullable columns.
    let fresh = tempfile::tempdir().expect("temp");
    let fresh_path = fresh.path().join("contractorproject.sqlite3");
    let _service = ApplicationService::open(&fresh_path).expect("open fresh");
    let connection = Connection::open(&fresh_path).expect("inspect fresh");
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    assert_eq!(version, 6);
    let job_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('jobs') WHERE name = 'data_date'",
            [],
            |row| row.get(0),
        )
        .expect("job data_date column");
    assert_eq!(job_columns, 1);
    let task_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name IN ('percent_complete', 'actual_start', 'actual_finish')",
            [],
            |row| row.get(0),
        )
        .expect("task progress columns");
    assert_eq!(task_columns, 3);

    // An existing exact-v5 database migrates forward while preserving its rows.
    let existing = tempfile::tempdir().expect("temp");
    let existing_path = existing.path().join("contractorproject.sqlite3");
    write_exact_v5_database(&existing_path, "job-v5");
    let service = ApplicationService::open(&existing_path).expect("migrate v5");
    let migrated_version: i64 = Connection::open(&existing_path)
        .expect("open migrated")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migrated version");
    assert_eq!(migrated_version, 6);
    let hierarchy = service.list_tasks("job-v5").expect("preserved tasks");
    assert_eq!(hierarchy.tasks.len(), 1);
    let leaf = &hierarchy.tasks[0];
    assert_eq!(leaf.percent_complete, None);
    assert_eq!(leaf.actual_start, None);
    assert_eq!(leaf.actual_finish, None);
    // The pre-migration backup retained the original v5 snapshot untouched.
    let backup_path =
        existing_path.with_file_name("contractorproject.sqlite3.pre-migration-v6.bak");
    let backup_version: i64 =
        Connection::open_with_flags(&backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open pre-migration backup")
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("backup version");
    assert_eq!(backup_version, 5);
}

#[test]
fn data_date_is_versioned_audited_and_persisted() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, _leaf) = seed_scheduled_leaf(&path);

    let set = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("set data date");
    assert_eq!(set.data_date.as_deref(), Some("2026-08-18"));
    assert_eq!(set.version, job.version + 1);
    let summary: String = Connection::open(&path)
        .expect("open audit database")
        .query_row(
            "SELECT summary FROM command_log ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("data date audit summary");
    assert_eq!(summary, "updated job data date");

    // A stale job version is rejected without drift.
    let (_, version_before) = job_row(&path, &job.id);
    let audit_before = command_log_count(&path);
    let conflict = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-19".into()),
                expected_job_version: job.version,
            },
        )
        .expect_err("stale data date version rejected");
    assert_eq!(conflict.kind(), "version_conflict");
    assert_eq!(job_row(&path, &job.id).1, version_before);
    assert_eq!(command_log_count(&path), audit_before);

    // Clearing persists across a restart.
    let cleared = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: None,
                expected_job_version: set.version,
            },
        )
        .expect("clear data date");
    assert_eq!(cleared.data_date, None);
    drop(service);
    let reopened = ApplicationService::open(&path).expect("reopen");
    assert_eq!(job_after(&reopened, &job.id).data_date, None);
}

#[test]
fn task_progress_round_trips_and_survives_restart_with_identical_projection() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");

    let projection_before = service.get_schedule(&job.id).expect("projection");
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("set progress");
    assert_eq!(progress.task.percent_complete, Some(50));
    assert_eq!(progress.task.actual_start.as_deref(), Some("2026-08-17"));
    assert_eq!(progress.task.actual_finish, None);
    assert_eq!(progress.task.version, leaf.task.version + 1);
    assert_eq!(progress.job_version, job.version + 1);
    let summary: String = Connection::open(&path)
        .expect("open audit database")
        .query_row(
            "SELECT summary FROM command_log ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("progress audit summary");
    assert_eq!(summary, "updated task progress");

    drop(service);
    let reopened = ApplicationService::open(&path).expect("reopen");
    let row = task_progress_row(&path, &leaf.task.id);
    assert_eq!(row.0, Some(50));
    assert_eq!(row.1.as_deref(), Some("2026-08-17"));
    assert_eq!(row.2, None);
    let projection_after = reopened.get_schedule(&job.id).expect("projection after");
    assert_eq!(projection_after.rows, projection_before.rows);
    assert_eq!(
        projection_after.schedule_finish,
        projection_before.schedule_finish
    );

    // Clearing nulls every progress column back to unstatused.
    let cleared = reopened
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: true,
                percent_complete: None,
                actual_start: None,
                actual_finish: None,
                expected_version: progress.task.version,
                expected_job_version: progress.job_version,
            },
        )
        .expect("clear progress");
    assert_eq!(cleared.task.percent_complete, None);
    let row = task_progress_row(&path, &leaf.task.id);
    assert_eq!((row.0, row.1, row.2), (None, None, None));
}

#[test]
fn scheduler_progress_rejections_surface_atomically_at_the_boundary() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);

    // Case 1: progress without a data date is rejected by the scheduler.
    let before_row = task_progress_row(&path, &leaf.task.id);
    let before_audit = command_log_count(&path);
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect_err("progress requires a data date");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(task_progress_row(&path, &leaf.task.id), before_row);
    assert_eq!(command_log_count(&path), before_audit);

    // Set a data date so the remaining rejections exercise cross-field rules.
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");

    // Case 2: an actual after the data date is rejected.
    let before_row = task_progress_row(&path, &leaf.task.id);
    let before_audit = command_log_count(&path);
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-19".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect_err("actual after data date");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(task_progress_row(&path, &leaf.task.id), before_row);
    assert_eq!(command_log_count(&path), before_audit);

    // Case 3: an unknown task is rejected before any write.
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: "missing".into(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: 1,
                expected_job_version: job.version,
            },
        )
        .expect_err("unknown task");
    assert_eq!(error.kind(), "not_found");
    assert_eq!(command_log_count(&path), before_audit);
}

#[test]
fn milestone_and_summary_progress_are_rejected_without_drift() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Rollups".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("schedule");
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");

    // A milestone (zero duration) rejects a partial percent from the scheduler.
    let milestone = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Milestone".into(),
                expected_job_version: job.version,
            },
        )
        .expect("milestone task");
    let milestone = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: milestone.task.id.clone(),
                duration_minutes: Some(0),
                expected_version: milestone.task.version,
                expected_job_version: milestone.job_version,
            },
        )
        .expect("milestone duration");
    let before_audit = command_log_count(&path);
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: milestone.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: milestone.task.version,
                expected_job_version: milestone.job_version,
            },
        )
        .expect_err("milestone partial percent");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(task_progress_row(&path, &milestone.task.id).0, None);
    assert_eq!(command_log_count(&path), before_audit);

    // A summary task (has children) is rejected before schedule validation.
    let parent = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Parent".into(),
                expected_job_version: milestone.job_version,
            },
        )
        .expect("parent task");
    let child = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(parent.task.id.clone()),
                name: "Child".into(),
                expected_job_version: parent.job_version,
            },
        )
        .expect("child task");
    let before_audit = command_log_count(&path);
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: parent.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: parent.task.version,
                expected_job_version: child.job_version,
            },
        )
        .expect_err("summary progress rejected");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(error.to_string(), "progress applies only to leaf tasks");
    assert_eq!(command_log_count(&path), before_audit);
}

#[test]
fn duplicate_progress_command_is_rejected_without_rerunning() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    let context = CommandContext {
        command_id: "progress-duplicate".into(),
        actor: CommandActor::Agent,
        client_name: "progress-test".into(),
    };
    let first = service
        .update_task_progress(
            context.clone(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("first progress");
    let audit_after_first = command_log_count(&path);
    let error = service
        .update_task_progress(
            context,
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(75),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: first.task.version,
                expected_job_version: first.job_version,
            },
        )
        .expect_err("duplicate command rejected");
    assert_eq!(error.kind(), "duplicate_command");
    assert_eq!(task_progress_row(&path, &leaf.task.id).0, Some(50));
    assert_eq!(command_log_count(&path), audit_after_first);
}

#[test]
fn stale_task_version_rejects_progress_without_drift() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    let before_row = task_progress_row(&path, &leaf.task.id);
    let before_audit = command_log_count(&path);
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version + 5,
                expected_job_version: job.version,
            },
        )
        .expect_err("stale task version");
    assert_eq!(error.kind(), "version_conflict");
    assert_eq!(task_progress_row(&path, &leaf.task.id), before_row);
    assert_eq!(command_log_count(&path), before_audit);
}

#[test]
fn clearing_the_data_date_with_persisted_progress_is_rejected_atomically() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("progress");

    // A constraint edit still commits while valid progress is persisted,
    // proving the shared validation helper loads progress without over-rejecting.
    let constrained = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: leaf.task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-17".into()),
                expected_version: progress.task.version,
                expected_job_version: progress.job_version,
            },
        )
        .expect("constraint edit with progress present");

    // Clearing the data date is rejected because the persisted entry needs one.
    let (data_date_before, job_version_before) = job_row(&path, &job.id);
    assert_eq!(data_date_before.as_deref(), Some("2026-08-18"));
    let audit_before = command_log_count(&path);
    let error = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: None,
                expected_job_version: constrained.job_version,
            },
        )
        .expect_err("cannot clear data date with progress present");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(job_row(&path, &job.id).0.as_deref(), Some("2026-08-18"));
    assert_eq!(job_row(&path, &job.id).1, job_version_before);
    assert_eq!(command_log_count(&path), audit_before);
}

#[test]
fn backup_and_restore_verification_accept_v6_and_exact_v5_snapshots() {
    // A live v6 database carrying a data date and progress verifies as a backup.
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("progress");
    let v6_backup = temp.path().join("v6.backup.sqlite3");
    let backup = service
        .create_verified_backup(CreateBackupRequest {
            destination: v6_backup.to_string_lossy().into_owned(),
        })
        .expect("verify v6 backup");
    assert!(backup.verified);

    // An exact-v5 snapshot is still accepted by the restore preflight and the
    // owned target migrates forward to v6.
    let v5_dir = tempfile::tempdir().expect("temp");
    let v5_path = v5_dir.path().join("v5.backup.sqlite3");
    write_exact_v5_database(&v5_path, "job-v5");
    let target = v5_dir.path().join("restored");
    let result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: v5_path.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect("verify exact v5 restore");
    assert!(result.verified);
    let restored_version: i64 = Connection::open(target.join("contractorproject.sqlite3"))
        .expect("open restored")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("restored version");
    assert_eq!(restored_version, 6);
}

#[test]
fn milestone_conversion_rejects_incompatible_progress_without_drift() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    // A partial-percent leaf cannot be converted to a milestone.
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("partial progress");
    let before_task = task_progress_row(&path, &leaf.task.id);
    let before_duration: Option<i64> = Connection::open(&path)
        .expect("open")
        .query_row(
            "SELECT duration_minutes FROM tasks WHERE id = ?1",
            [&leaf.task.id],
            |row| row.get(0),
        )
        .expect("duration");
    let (_, before_job_version) = job_row(&path, &job.id);
    let before_audit = command_log_count(&path);
    let error = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: leaf.task.id.clone(),
                duration_minutes: Some(0),
                expected_version: progress.task.version,
                expected_job_version: progress.job_version,
            },
        )
        .expect_err("milestone conversion rejects partial progress");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(
        error.to_string(),
        "clear or complete task progress before converting to a milestone"
    );
    assert_eq!(task_progress_row(&path, &leaf.task.id), before_task);
    let after_duration: Option<i64> = Connection::open(&path)
        .expect("open")
        .query_row(
            "SELECT duration_minutes FROM tasks WHERE id = ?1",
            [&leaf.task.id],
            |row| row.get(0),
        )
        .expect("duration");
    assert_eq!(after_duration, before_duration);
    assert_eq!(job_row(&path, &job.id).1, before_job_version);
    assert_eq!(command_log_count(&path), before_audit);
}

#[test]
fn milestone_conversion_allows_a_complete_leaf_with_equal_actuals() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    // A leaf completed on a single working day has equal actual start and finish.
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(100),
                actual_start: Some("2026-08-17".into()),
                actual_finish: Some("2026-08-17".into()),
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("complete progress");
    let converted = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: leaf.task.id.clone(),
                duration_minutes: Some(0),
                expected_version: progress.task.version,
                expected_job_version: progress.job_version,
            },
        )
        .expect("convert complete leaf to milestone");
    assert_eq!(converted.task.duration_minutes, Some(0));
    assert_eq!(converted.task.percent_complete, Some(100));
    let row = task_progress_row(&path, &leaf.task.id);
    assert_eq!(row.0, Some(100));
    assert_eq!(row.1.as_deref(), Some("2026-08-17"));
    assert_eq!(row.2.as_deref(), Some("2026-08-17"));
}

#[test]
fn calendar_narrowing_under_persisted_weekend_actuals_is_rejected_atomically() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Weekend work".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    // A seven-day calendar makes Saturday/Sunday valid working days.
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: seven_day_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("seven-day schedule");
    let task = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Weekend leaf".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");
    let duration = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: task.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: task.task.version,
                expected_job_version: task.job_version,
            },
        )
        .expect("duration");
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-17".into()),
                expected_job_version: duration.job_version,
            },
        )
        .expect("data date");
    // A leaf completed across the weekend has Saturday/Sunday actuals.
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: task.task.id.clone(),
                clear: false,
                percent_complete: Some(100),
                actual_start: Some("2026-08-15".into()),
                actual_finish: Some("2026-08-16".into()),
                expected_version: duration.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("weekend completion");

    // Narrowing to Monday-Friday inverts the normalized actual window and the
    // scheduler rejects it; the calendar, version, and audit log are untouched.
    let (calendar_before, version_before) = job_calendar_and_version(&path, &job.id);
    let audit_before = command_log_count(&path);
    let error = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: progress.job_version,
            },
        )
        .expect_err("weekday narrowing rejected");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(
        job_calendar_and_version(&path, &job.id),
        (calendar_before, version_before)
    );
    assert_eq!(command_log_count(&path), audit_before);
}

#[test]
fn a_still_valid_calendar_change_commits_with_progress_present() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("progress");
    // An earlier start time keeps every actual and the data date valid.
    let updated = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: WorkingCalendar {
                    working_weekdays: working_calendar().working_weekdays,
                    workday_start_minute: 420,
                    workday_duration_minutes: 480,
                },
                expected_job_version: progress.job_version,
            },
        )
        .expect("valid calendar change commits");
    assert_eq!(updated.version, progress.job_version + 1);
    assert_eq!(updated.data_date.as_deref(), Some("2026-08-18"));
    let row = task_progress_row(&path, &leaf.task.id);
    assert_eq!(row.0, Some(50));
    assert_eq!(row.1.as_deref(), Some("2026-08-17"));
}

#[test]
fn clearing_the_schedule_start_with_persisted_progress_is_rejected_atomically() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job, leaf) = seed_scheduled_leaf(&path);
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect("data date");
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("progress");
    let (calendar_before, version_before) = job_calendar_and_version(&path, &job.id);
    let audit_before = command_log_count(&path);
    let error = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: None,
                calendar: working_calendar(),
                expected_job_version: progress.job_version,
            },
        )
        .expect_err("cannot clear schedule start with progress present");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(
        error.to_string(),
        "clear task progress and the data date before clearing the schedule start"
    );
    let schedule_start: Option<String> = Connection::open(&path)
        .expect("open")
        .query_row(
            "SELECT schedule_start FROM jobs WHERE id = ?1",
            [&job.id],
            |row| row.get(0),
        )
        .expect("schedule start");
    assert_eq!(schedule_start.as_deref(), Some("2026-08-17"));
    assert_eq!(
        job_calendar_and_version(&path, &job.id),
        (calendar_before, version_before)
    );
    assert_eq!(command_log_count(&path), audit_before);
}

#[test]
fn progress_on_a_duration_less_leaf_reports_duration_required() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "No duration".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("schedule");
    // A freshly created leaf carries no duration yet.
    let leaf = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Unsized".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");
    let audit_before = command_log_count(&path);
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(50),
                actual_start: Some("2026-08-17".into()),
                actual_finish: None,
                expected_version: leaf.task.version,
                expected_job_version: leaf.job_version,
            },
        )
        .expect_err("duration required");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(
        error.to_string(),
        "set a duration before reporting progress"
    );
    assert_eq!(command_log_count(&path), audit_before);
}

#[test]
fn calendar_narrowing_is_rejected_even_with_a_pre_existing_duration_less_task() {
    // Abuse case: a valid statused seven-day job with weekend actuals, plus a
    // fresh duration-less WBS row. Narrowing to Monday-Friday must still be
    // rejected for the progress-class failure it causes, not silently accepted
    // because an unrelated structural defect makes the current state invalid.
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Weekend abuse".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: seven_day_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("seven-day schedule");
    let leaf = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Weekend leaf".into(),
                expected_job_version: job.version,
            },
        )
        .expect("leaf");
    let duration = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: leaf.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: leaf.task.version,
                expected_job_version: leaf.job_version,
            },
        )
        .expect("duration");
    let job = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-17".into()),
                expected_job_version: duration.job_version,
            },
        )
        .expect("data date");
    let progress = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: leaf.task.id.clone(),
                clear: false,
                percent_complete: Some(100),
                actual_start: Some("2026-08-15".into()),
                actual_finish: Some("2026-08-16".into()),
                expected_version: duration.task.version,
                expected_job_version: job.version,
            },
        )
        .expect("weekend completion");
    // A fresh duration-less WBS row makes the current stored schedule invalid.
    let unsized_row = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Unsized".into(),
                expected_job_version: progress.job_version,
            },
        )
        .expect("unsized task");

    let (calendar_before, version_before) = job_calendar_and_version(&path, &job.id);
    let audit_before = command_log_count(&path);
    let error = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: unsized_row.job_version,
            },
        )
        .expect_err("progress-class failure blocks the narrowing");
    assert_eq!(error.kind(), "validation_failed");
    match &error {
        ApplicationError::ValidationFailed { code, .. } => {
            assert_eq!(*code, "progress_normalized_order")
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(
        job_calendar_and_version(&path, &job.id),
        (calendar_before, version_before)
    );
    assert_eq!(command_log_count(&path), audit_before);
}

#[test]
fn schedule_edits_commit_over_pre_existing_incomplete_tasks() {
    // The ordinary setup flow adds WBS rows before their durations, then keeps
    // adjusting the schedule. A duration-less leaf must not block that edit.
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Mid-setup".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("initial schedule");
    // A freshly added leaf carries no duration yet.
    let task = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Unsized".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");
    // Changing the calendar and schedule start still commits over that state.
    let updated = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-18".into()),
                calendar: seven_day_calendar(),
                expected_job_version: task.job_version,
            },
        )
        .expect("schedule change commits over incomplete task");
    assert_eq!(updated.version, task.job_version + 1);
    assert_eq!(updated.schedule_start.as_deref(), Some("2026-08-18"));
}

#[test]
fn data_date_on_a_start_less_job_reports_schedule_start_required() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "No start".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let audit_before = command_log_count(&path);
    let error = service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job.id.clone(),
                data_date: Some("2026-08-18".into()),
                expected_job_version: job.version,
            },
        )
        .expect_err("data date needs a schedule start");
    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(
        error.to_string(),
        "set a schedule start before updating the schedule"
    );
    assert_eq!(command_log_count(&path), audit_before);
}

/// Writes an exact-v5 schema (migrations 1..5) with one leaf task, matching the
/// DDL the migration path produces so the backup verifier accepts it as-is.
fn write_exact_v5_database(path: &std::path::Path, job_id: &str) {
    let connection = Connection::open(path).expect("create v5");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES (1, '2026-08-16T00:00:00.000Z'), (2, '2026-08-16T00:00:00.000Z'), (3, '2026-08-16T00:00:00.000Z'), (4, '2026-08-16T00:00:00.000Z'), (5, '2026-08-16T00:00:00.000Z');
             CREATE TABLE jobs (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL, timezone TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), schedule_start TEXT, calendar_json TEXT NOT NULL DEFAULT '[]');
             CREATE TABLE tasks (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, parent_task_id TEXT, sort_key INTEGER NOT NULL CHECK (sort_key >= 0), name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), duration_minutes INTEGER CHECK (duration_minutes >= 0), start_no_earlier_than TEXT, finish_no_later_than TEXT, UNIQUE (job_id, id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, parent_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT);
             CREATE INDEX tasks_job_parent_order ON tasks(job_id, parent_task_id, sort_key, id);
             CREATE UNIQUE INDEX tasks_root_sibling_order ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
             CREATE UNIQUE INDEX tasks_child_sibling_order ON tasks(job_id, parent_task_id, sort_key) WHERE parent_task_id IS NOT NULL;
             CREATE TABLE command_log (command_id TEXT NOT NULL PRIMARY KEY CHECK (length(command_id) BETWEEN 1 AND 128), actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')), client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120), created_at TEXT NOT NULL, summary TEXT NOT NULL CHECK (length(summary) <= 240));
             CREATE TABLE task_dependencies (job_id TEXT NOT NULL, predecessor_task_id TEXT NOT NULL, successor_task_id TEXT NOT NULL, lag_minutes INTEGER NOT NULL CHECK (lag_minutes >= 0), PRIMARY KEY (predecessor_task_id, successor_task_id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, predecessor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, FOREIGN KEY (job_id, successor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, CHECK (predecessor_task_id <> successor_task_id));
             CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);",
        )
        .expect("write exact v5 schema");
    connection
        .execute(
            "INSERT INTO jobs VALUES (?1, 'Existing v5', 'draft', 'UTC', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 2, '2026-08-17', '{\"workingWeekdays\":[\"monday\",\"tuesday\",\"wednesday\",\"thursday\",\"friday\"],\"workdayStartMinute\":480,\"workdayDurationMinutes\":480}')",
            [job_id],
        )
        .expect("seed v5 job");
    connection
        .execute(
            "INSERT INTO tasks VALUES ('leaf-v5', ?1, NULL, 0, 'Leaf', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, 480, NULL, NULL)",
            [job_id],
        )
        .expect("seed v5 task");
    connection
        .execute(
            "INSERT INTO command_log VALUES ('v5-audit', 'agent', 'test', '2026-08-16T00:00:00.000Z', 'updated task duration')",
            [],
        )
        .expect("seed v5 audit");
}
