// Integration coverage for issue #43: durable immutable named baselines with
// audited creation, snapshot immutability, single-default selection, and
// verified backup/restore acceptance of the v7 schema.

use contractorproject_lib::application::{
    ApplicationError, ApplicationService, Baseline, CommandActor, CommandContext,
    CreateBackupRequest, CreateBaselineRequest, CreateJobRequest, CreateTaskRequest,
    SetBaselineComparisonDefaultRequest, UpdateScheduleRequest, UpdateTaskDurationRequest,
    VerifyRestoreRequest,
};
use contractorproject_lib::scheduling::{CalendarWeekday, WorkingCalendar};
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_COMMAND_ID: AtomicU64 = AtomicU64::new(1);

fn command_context() -> CommandContext {
    CommandContext {
        command_id: format!(
            "baseline-command-{}",
            NEXT_COMMAND_ID.fetch_add(1, Ordering::Relaxed)
        ),
        actor: CommandActor::Agent,
        client_name: "baseline-test".into(),
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

/// Creates a job with a Monday schedule start and one 480-minute leaf task.
/// Returns the service, the job id, and the leaf task id at its current version.
fn seed_scheduled_leaf(path: &std::path::Path) -> (ApplicationService, String, String, i64, i64) {
    let service = ApplicationService::open(path).expect("open service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Baseline job".into(),
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
    (
        service,
        job.id,
        duration.task.id,
        duration.task.version,
        duration.job_version,
    )
}

fn current_job_version(path: &std::path::Path, job_id: &str) -> i64 {
    Connection::open(path)
        .expect("open job database")
        .query_row("SELECT version FROM jobs WHERE id = ?1", [job_id], |row| {
            row.get(0)
        })
        .expect("read job version")
}

fn command_log_count(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .expect("open audit database")
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count command log")
}

fn baseline_count(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .expect("open baseline database")
        .query_row("SELECT COUNT(*) FROM baselines", [], |row| row.get(0))
        .expect("count baselines")
}

/// Snapshot rows for a baseline ordered by task id.
fn baseline_rows(path: &std::path::Path, baseline_id: &str) -> Vec<(String, String, String, i64)> {
    let connection = Connection::open(path).expect("open baseline database");
    let mut statement = connection
        .prepare(
            "SELECT task_id, start, finish, duration_minutes
             FROM baseline_tasks WHERE baseline_id = ?1 ORDER BY task_id",
        )
        .expect("prepare baseline rows");
    statement
        .query_map([baseline_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .expect("query baseline rows")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect baseline rows")
}

#[test]
fn migration_v7_adds_baseline_tables_on_fresh_and_existing_v6_database() {
    // A fresh database opens directly at v7 with both baseline tables.
    let fresh = tempfile::tempdir().expect("temp");
    let fresh_path = fresh.path().join("contractorproject.sqlite3");
    let _service = ApplicationService::open(&fresh_path).expect("open fresh");
    let connection = Connection::open(&fresh_path).expect("inspect fresh");
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    assert_eq!(version, 7);
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('baselines', 'baseline_tasks')",
            [],
            |row| row.get(0),
        )
        .expect("baseline tables");
    assert_eq!(table_count, 2);
    let default_index: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'baselines_one_default_per_job'",
            [],
            |row| row.get(0),
        )
        .expect("default index");
    assert_eq!(default_index, 1);

    // An existing exact-v6 database migrates forward while preserving its rows.
    let existing = tempfile::tempdir().expect("temp");
    let existing_path = existing.path().join("contractorproject.sqlite3");
    write_exact_v6_database(&existing_path, "job-v6");
    let service = ApplicationService::open(&existing_path).expect("migrate v6");
    let migrated_version: i64 = Connection::open(&existing_path)
        .expect("open migrated")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migrated version");
    assert_eq!(migrated_version, 7);
    assert_eq!(service.list_tasks("job-v6").expect("tasks").tasks.len(), 1);
    assert!(service
        .list_baselines("job-v6")
        .expect("baselines")
        .is_empty());
    // The pre-migration backup retained the original v6 snapshot untouched.
    let backup_path =
        existing_path.with_file_name("contractorproject.sqlite3.pre-migration-v7.bak");
    let backup_version: i64 =
        Connection::open_with_flags(&backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open pre-migration backup")
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("backup version");
    assert_eq!(backup_version, 6);
}

#[test]
fn create_baseline_snapshots_leaves_and_survives_restart() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, task_id, _task_version, job_version) = seed_scheduled_leaf(&path);

    let baseline = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Original plan".into(),
                expected_job_version: job_version,
            },
        )
        .expect("create baseline");
    assert!(baseline.is_comparison_default);
    assert_eq!(baseline.name, "Original plan");
    // Creation bumps the job version.
    assert_eq!(current_job_version(&path, &job_id), job_version + 1);

    let rows = baseline_rows(&path, &baseline.id);
    assert_eq!(
        rows,
        vec![(
            task_id.clone(),
            "2026-08-17T08:00:00".to_string(),
            "2026-08-17T16:00:00".to_string(),
            480,
        )]
    );

    // Reopen the store; the immutable snapshot is intact.
    drop(service);
    let reopened = ApplicationService::open(&path).expect("reopen");
    let listed = reopened.list_baselines(&job_id).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, baseline.id);
    assert!(listed[0].is_comparison_default);
    assert_eq!(baseline_rows(&path, &baseline.id), rows);
}

#[test]
fn first_baseline_auto_defaults_and_second_does_not_steal_it() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);

    let first = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "First".into(),
                expected_job_version: job_version,
            },
        )
        .expect("first baseline");
    assert!(first.is_comparison_default);

    let second = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Second".into(),
                expected_job_version: job_version + 1,
            },
        )
        .expect("second baseline");
    assert!(!second.is_comparison_default);

    let listed = service.list_baselines(&job_id).expect("list");
    let defaults: Vec<&Baseline> = listed
        .iter()
        .filter(|baseline| baseline.is_comparison_default)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, first.id);
}

#[test]
fn setting_the_comparison_default_clears_the_previous_one() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);

    let first = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "First".into(),
                expected_job_version: job_version,
            },
        )
        .expect("first");
    let second = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Second".into(),
                expected_job_version: job_version + 1,
            },
        )
        .expect("second");

    let updated = service
        .set_baseline_comparison_default(
            command_context(),
            SetBaselineComparisonDefaultRequest {
                job_id: job_id.clone(),
                baseline_id: second.id.clone(),
                expected_job_version: job_version + 2,
            },
        )
        .expect("switch default");
    assert!(updated.is_comparison_default);
    assert_eq!(current_job_version(&path, &job_id), job_version + 3);

    let listed = service.list_baselines(&job_id).expect("list");
    let by_id = |id: &str| listed.iter().find(|b| b.id == id).expect("present");
    assert!(!by_id(&first.id).is_comparison_default);
    assert!(by_id(&second.id).is_comparison_default);

    // Re-selecting the already-default baseline is a legal audited no-op-shaped
    // command that still bumps the job version.
    let repeat = service
        .set_baseline_comparison_default(
            command_context(),
            SetBaselineComparisonDefaultRequest {
                job_id: job_id.clone(),
                baseline_id: second.id.clone(),
                expected_job_version: job_version + 3,
            },
        )
        .expect("re-select default");
    assert!(repeat.is_comparison_default);
    assert_eq!(current_job_version(&path, &job_id), job_version + 4);
}

#[test]
fn duplicate_baseline_name_is_rejected_atomically() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);

    service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Plan A".into(),
                expected_job_version: job_version,
            },
        )
        .expect("first");
    let version_after = current_job_version(&path, &job_id);
    let audits_after = command_log_count(&path);

    let error = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Plan A".into(),
                expected_job_version: version_after,
            },
        )
        .expect_err("duplicate name rejected");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "baseline_name_exists",
            ..
        }
    ));
    // No second baseline, no version bump, no audit row.
    assert_eq!(baseline_count(&path), 1);
    assert_eq!(current_job_version(&path, &job_id), version_after);
    assert_eq!(command_log_count(&path), audits_after);
}

#[test]
fn blank_baseline_name_is_rejected_without_writing() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);
    let audits_before = command_log_count(&path);

    let error = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "   ".into(),
                expected_job_version: job_version,
            },
        )
        .expect_err("blank name rejected");
    assert!(matches!(
        error,
        ApplicationError::InvalidInput { field: "name", .. }
    ));
    assert_eq!(baseline_count(&path), 0);
    assert_eq!(current_job_version(&path, &job_id), job_version);
    assert_eq!(command_log_count(&path), audits_before);
}

#[test]
fn uncalculable_schedule_rejects_baseline_creation_atomically() {
    // A job without a schedule start cannot calculate, so no baseline is written.
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Unscheduled".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    service
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
    let version_before = current_job_version(&path, &job.id);
    let audits_before = command_log_count(&path);

    let error = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job.id.clone(),
                name: "No schedule".into(),
                expected_job_version: version_before,
            },
        )
        .expect_err("uncalculable schedule rejected");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "schedule_start_required",
            ..
        }
    ));
    assert_eq!(baseline_count(&path), 0);
    assert_eq!(current_job_version(&path, &job.id), version_before);
    assert_eq!(command_log_count(&path), audits_before);
}

#[test]
fn create_baseline_rejects_a_stale_job_version() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);
    let audits_before = command_log_count(&path);

    let error = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Stale".into(),
                expected_job_version: job_version + 5,
            },
        )
        .expect_err("version conflict");
    assert!(matches!(error, ApplicationError::VersionConflict { .. }));
    assert_eq!(baseline_count(&path), 0);
    assert_eq!(command_log_count(&path), audits_before);
}

#[test]
fn duplicate_command_id_does_not_rerun_baseline_creation() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);

    let context = command_context();
    service
        .create_baseline(
            context.clone(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Once".into(),
                expected_job_version: job_version,
            },
        )
        .expect("first apply");
    let version_after = current_job_version(&path, &job_id);
    let audits_after = command_log_count(&path);

    let error = service
        .create_baseline(
            context,
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Twice".into(),
                expected_job_version: version_after,
            },
        )
        .expect_err("duplicate command rejected");
    assert!(matches!(error, ApplicationError::DuplicateCommand { .. }));
    assert_eq!(baseline_count(&path), 1);
    assert_eq!(current_job_version(&path, &job_id), version_after);
    assert_eq!(command_log_count(&path), audits_after);
}

#[test]
fn later_task_edits_leave_the_baseline_snapshot_unchanged() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, task_id, task_version, job_version) = seed_scheduled_leaf(&path);

    let baseline = service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "Frozen".into(),
                expected_job_version: job_version,
            },
        )
        .expect("baseline");
    let snapshot = baseline_rows(&path, &baseline.id);

    // Change the task duration, which reschedules the live projection.
    service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: task_id.clone(),
                duration_minutes: Some(960),
                expected_version: task_version,
                expected_job_version: job_version + 1,
            },
        )
        .expect("edit duration");

    // The immutable baseline rows are byte-for-byte unchanged.
    assert_eq!(baseline_rows(&path, &baseline.id), snapshot);
    assert_eq!(
        snapshot,
        vec![(
            task_id,
            "2026-08-17T08:00:00".to_string(),
            "2026-08-17T16:00:00".to_string(),
            480,
        )]
    );
}

#[test]
fn verified_backup_and_clean_restore_accept_v7_and_exact_v6() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (service, job_id, _task_id, _task_version, job_version) = seed_scheduled_leaf(&path);
    service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id,
                name: "Backed up".into(),
                expected_job_version: job_version,
            },
        )
        .expect("baseline");

    // A live v7 snapshot verifies and restores into a fresh app-data directory.
    let backup_dir = tempfile::tempdir().expect("temp");
    let backup_path = backup_dir.path().join("v7.backup.sqlite3");
    let backup = service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("verify v7 backup");
    assert!(backup.verified);
    let v7_target = backup_dir.path().join("restored-v7");
    let v7_result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup_path.to_string_lossy().into_owned(),
            target_app_data_dir: v7_target.to_string_lossy().into_owned(),
        })
        .expect("restore v7");
    assert!(v7_result.verified);
    let restored_version: i64 = Connection::open(v7_target.join("contractorproject.sqlite3"))
        .expect("open restored v7")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("restored version");
    assert_eq!(restored_version, 7);

    // An exact-v6 snapshot is still accepted by the restore preflight.
    let v6_dir = tempfile::tempdir().expect("temp");
    let v6_path = v6_dir.path().join("v6.backup.sqlite3");
    write_exact_v6_database(&v6_path, "job-v6");
    let v6_target = v6_dir.path().join("restored-v6");
    let v6_result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: v6_path.to_string_lossy().into_owned(),
            target_app_data_dir: v6_target.to_string_lossy().into_owned(),
        })
        .expect("verify exact v6 restore");
    assert!(v6_result.verified);
}

/// Writes an exact-v6 database (schema migrations 1..6, data-date/progress
/// columns present, no baseline tables) with one scheduled leaf.
fn write_exact_v6_database(path: &std::path::Path, job_id: &str) {
    let connection = Connection::open(path).expect("create v6");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES (1, '2026-08-16T00:00:00.000Z'), (2, '2026-08-16T00:00:00.000Z'), (3, '2026-08-16T00:00:00.000Z'), (4, '2026-08-16T00:00:00.000Z'), (5, '2026-08-16T00:00:00.000Z'), (6, '2026-08-16T00:00:00.000Z');
             CREATE TABLE jobs (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL, timezone TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), schedule_start TEXT, calendar_json TEXT NOT NULL DEFAULT '[]', data_date TEXT);
             CREATE TABLE tasks (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, parent_task_id TEXT, sort_key INTEGER NOT NULL CHECK (sort_key >= 0), name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), duration_minutes INTEGER CHECK (duration_minutes >= 0), start_no_earlier_than TEXT, finish_no_later_than TEXT, percent_complete INTEGER, actual_start TEXT, actual_finish TEXT, UNIQUE (job_id, id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, parent_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT);
             CREATE INDEX tasks_job_parent_order ON tasks(job_id, parent_task_id, sort_key, id);
             CREATE UNIQUE INDEX tasks_root_sibling_order ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
             CREATE UNIQUE INDEX tasks_child_sibling_order ON tasks(job_id, parent_task_id, sort_key) WHERE parent_task_id IS NOT NULL;
             CREATE TABLE command_log (command_id TEXT NOT NULL PRIMARY KEY CHECK (length(command_id) BETWEEN 1 AND 128), actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')), client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120), created_at TEXT NOT NULL, summary TEXT NOT NULL CHECK (length(summary) <= 240));
             CREATE TABLE task_dependencies (job_id TEXT NOT NULL, predecessor_task_id TEXT NOT NULL, successor_task_id TEXT NOT NULL, lag_minutes INTEGER NOT NULL CHECK (lag_minutes >= 0), PRIMARY KEY (predecessor_task_id, successor_task_id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, predecessor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, FOREIGN KEY (job_id, successor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, CHECK (predecessor_task_id <> successor_task_id));
             CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);",
        )
        .expect("write exact v6 schema");
    connection
        .execute(
            "INSERT INTO jobs VALUES (?1, 'Existing v6', 'draft', 'UTC', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 2, '2026-08-17', '{\"workingWeekdays\":[\"monday\",\"tuesday\",\"wednesday\",\"thursday\",\"friday\"],\"workdayStartMinute\":480,\"workdayDurationMinutes\":480}', NULL)",
            [job_id],
        )
        .expect("seed v6 job");
    connection
        .execute(
            "INSERT INTO tasks VALUES ('leaf-v6', ?1, NULL, 0, 'Leaf', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, 480, NULL, NULL, NULL, NULL, NULL)",
            [job_id],
        )
        .expect("seed v6 task");
    connection
        .execute(
            "INSERT INTO command_log VALUES ('v6-audit', 'agent', 'test', '2026-08-16T00:00:00.000Z', 'updated task duration')",
            [],
        )
        .expect("seed v6 audit");
}
