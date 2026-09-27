// Command-boundary contract for dated calendar exceptions (issue #53): audited
// add/remove, atomic validation, durability, and regression against baselines
// and typed dependencies.

use chrono::{Duration, NaiveDate};
use contractorproject_lib::application::{
    AddDependencyRequest, ApplicationError, ApplicationService, CalendarExceptionRequest,
    CommandActor, CommandContext, CreateBackupRequest, CreateBaselineRequest, CreateJobRequest,
    CreateTaskRequest, Job, UpdateJobDataDateRequest, UpdateScheduleRequest,
    UpdateTaskDurationRequest, UpdateTaskProgressRequest, VerifyRestoreRequest,
};
use contractorproject_lib::scheduling::{CalendarWeekday, DependencyType, WorkingCalendar};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_COMMAND_ID: AtomicU64 = AtomicU64::new(1);

fn command_context() -> CommandContext {
    CommandContext {
        command_id: format!(
            "test-command-{}",
            NEXT_COMMAND_ID.fetch_add(1, Ordering::Relaxed)
        ),
        actor: CommandActor::Agent,
        client_name: "integration-test".into(),
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
        workday_start_minute: 8 * 60,
        workday_duration_minutes: 8 * 60,
        exceptions: Vec::new(),
    }
}

const MONDAY: &str = "2026-08-17";

fn job_state(service: &ApplicationService, job_id: &str) -> Job {
    service
        .list_jobs()
        .expect("list jobs")
        .into_iter()
        .find(|job| job.id == job_id)
        .expect("job present")
}

/// Creates a job with a Monday schedule start and a single 480-minute leaf `A`.
fn job_with_leaf(service: &ApplicationService) -> (String, String) {
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Exceptions".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some(MONDAY.into()),
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
                name: "A".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");
    service
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
    (job.id, task.task.id)
}

fn leaf_start(service: &ApplicationService, job_id: &str, task_id: &str) -> String {
    service
        .get_schedule(job_id)
        .expect("schedule")
        .rows
        .into_iter()
        .find(|row| row.task_id == task_id)
        .expect("row present")
        .start
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

fn calendar_exception_count(path: &Path) -> i64 {
    Connection::open(path)
        .expect("open db")
        .query_row("SELECT COUNT(*) FROM calendar_exceptions", [], |row| {
            row.get(0)
        })
        .expect("count exceptions")
}

#[test]
fn adding_an_exception_persists_shifts_the_schedule_and_survives_restart() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let (job_id, task_id);
    {
        let service = ApplicationService::open(&path).expect("open");
        (job_id, task_id) = job_with_leaf(&service);
        // A starts Monday before any exception.
        assert_eq!(
            leaf_start(&service, &job_id, &task_id),
            "2026-08-17T08:00:00"
        );

        let version = job_state(&service, &job_id).version;
        let job = service
            .add_calendar_exception(
                command_context(),
                CalendarExceptionRequest {
                    job_id: job_id.clone(),
                    date: MONDAY.into(),
                    expected_job_version: version,
                },
            )
            .expect("add exception");
        assert_eq!(job.version, version + 1);
        assert_eq!(job.calendar_exceptions, vec![MONDAY.to_string()]);
        // Closing Monday pushes A to Tuesday.
        assert_eq!(
            leaf_start(&service, &job_id, &task_id),
            "2026-08-18T08:00:00"
        );
    }
    // Reopen: the exception and its scheduling effect survive.
    let service = ApplicationService::open(&path).expect("reopen");
    assert_eq!(
        job_state(&service, &job_id).calendar_exceptions,
        vec![MONDAY.to_string()]
    );
    assert_eq!(
        leaf_start(&service, &job_id, &task_id),
        "2026-08-18T08:00:00"
    );
}

#[test]
fn removing_an_exception_restores_the_schedule() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, task_id) = job_with_leaf(&service);

    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("add");
    assert_eq!(
        leaf_start(&service, &job_id, &task_id),
        "2026-08-18T08:00:00"
    );

    let version = job_state(&service, &job_id).version;
    let job = service
        .remove_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("remove");
    assert!(job.calendar_exceptions.is_empty());
    assert_eq!(
        leaf_start(&service, &job_id, &task_id),
        "2026-08-17T08:00:00"
    );
    assert_eq!(calendar_exception_count(&path), 0);
}

#[test]
fn a_duplicate_command_id_is_rejected_without_a_second_row() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    let version = job_state(&service, &job_id).version;
    let context = command_context();
    service
        .add_calendar_exception(
            context.clone(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("first add");
    let error = service
        .add_calendar_exception(
            context,
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "2026-08-18".into(),
                expected_job_version: version + 1,
            },
        )
        .expect_err("replayed command");
    assert!(matches!(error, ApplicationError::DuplicateCommand { .. }));
    assert_eq!(calendar_exception_count(&path), 1);
}

#[test]
fn a_stale_job_version_is_rejected() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    let error = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: 1, // stale
            },
        )
        .expect_err("stale version");
    assert!(matches!(error, ApplicationError::VersionConflict { .. }));
    assert_eq!(calendar_exception_count(&path), 0);
}

#[test]
fn removing_a_missing_exception_is_a_not_found() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    let version = job_state(&service, &job_id).version;
    let error = service
        .remove_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect_err("missing exception");
    assert!(matches!(error, ApplicationError::NotFound { .. }));
}

#[test]
fn adding_the_same_date_twice_is_rejected() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("first add");
    let version = job_state(&service, &job_id).version;
    let error = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect_err("duplicate date");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "calendar_exception_exists",
            ..
        }
    ));
}

#[test]
fn a_non_canonical_date_is_rejected_at_the_command_boundary() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    let version = job_state(&service, &job_id).version;
    let error = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "2026-8-1".into(),
                expected_job_version: version,
            },
        )
        .expect_err("non-canonical date");
    assert!(matches!(error, ApplicationError::InvalidInput { .. }));
}

#[test]
fn an_exception_that_inverts_persisted_actuals_rejects_atomically() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, task_id) = job_with_leaf(&service);

    // Complete A with equal Thursday actuals and a Friday data date.
    let hierarchy = service.list_tasks(&job_id).expect("tasks");
    let leaf = hierarchy
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .expect("leaf");
    let version = job_state(&service, &job_id).version;
    service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job_id.clone(),
                data_date: Some("2026-08-21".into()),
                expected_job_version: version,
            },
        )
        .expect("data date");
    let version = job_state(&service, &job_id).version;
    service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: task_id.clone(),
                clear: false,
                percent_complete: Some(100),
                actual_start: Some("2026-08-20".into()),
                actual_finish: Some("2026-08-20".into()),
                expected_version: leaf.version,
                expected_job_version: version,
            },
        )
        .expect("progress");

    // Closing Thursday inverts the completed actuals (start normalizes forward,
    // finish backward), so the add rejects and nothing persists.
    let version = job_state(&service, &job_id).version;
    let error = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "2026-08-20".into(),
                expected_job_version: version,
            },
        )
        .expect_err("inverting exception");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "progress_normalized_order",
            ..
        }
    ));
    assert_eq!(calendar_exception_count(&path), 0);
    assert_eq!(job_state(&service, &job_id).version, version);
}

#[test]
fn a_pre_existing_structural_defect_is_not_blamed_on_the_exception() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    // A job whose only leaf has no duration cannot schedule, but the exception
    // add must not be blamed for that pre-existing defect.
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Setup".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some(MONDAY.into()),
                calendar: working_calendar(),
                expected_job_version: job.version,
            },
        )
        .expect("schedule");
    service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "durationless".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");

    let version = job_state(&service, &job.id).version;
    let updated = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job.id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("exception add succeeds despite structural defect");
    assert_eq!(updated.calendar_exceptions, vec![MONDAY.to_string()]);
}

#[test]
fn baselines_and_typed_dependencies_are_untouched_by_an_exception() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, task_a) = job_with_leaf(&service);

    // Add a second leaf B and a typed SS dependency A -> B.
    let version = job_state(&service, &job_id).version;
    let task_b = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job_id.clone(),
                parent_task_id: None,
                name: "B".into(),
                expected_job_version: version,
            },
        )
        .expect("task b");
    service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: task_b.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: task_b.task.version,
                expected_job_version: task_b.job_version,
            },
        )
        .expect("duration b");
    let version = job_state(&service, &job_id).version;
    service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job_id.clone(),
                predecessor_task_id: task_a.clone(),
                successor_task_id: task_b.task.id.clone(),
                dependency_type: Some("SS".into()),
                lag_minutes: 0,
                expected_job_version: version,
            },
        )
        .expect("dependency");

    // Snapshot a baseline, then capture its immutable rows.
    let version = job_state(&service, &job_id).version;
    service
        .create_baseline(
            command_context(),
            CreateBaselineRequest {
                job_id: job_id.clone(),
                name: "v1".into(),
                expected_job_version: version,
            },
        )
        .expect("baseline");
    let baseline_before = baseline_rows(&path);

    // Add an exception, which replans the live schedule.
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("exception");

    // The baseline snapshot rows are byte-identical, the typed link is intact,
    // and the schedule still computes with a baseline comparison (variance).
    assert_eq!(baseline_rows(&path), baseline_before);
    let hierarchy = service.list_tasks(&job_id).expect("tasks");
    assert_eq!(hierarchy.dependencies.len(), 1);
    assert_eq!(
        hierarchy.dependencies[0].dependency_type,
        DependencyType::StartStart
    );
    let schedule = service.get_schedule(&job_id).expect("schedule");
    assert!(schedule.baseline_id.is_some());
    assert!(schedule.rows.iter().any(|row| row.baseline.is_some()));
}

fn baseline_rows(path: &Path) -> Vec<String> {
    let connection = Connection::open(path).expect("open db");
    let mut statement = connection
        .prepare(
            "SELECT baseline_id, task_id, start, finish, duration_minutes
             FROM baseline_tasks ORDER BY baseline_id, task_id",
        )
        .expect("prepare");
    statement
        .query_map([], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .expect("query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect")
}

// --- Migration and backup/restore (v9 and later) ----------------------------

/// Writes an exact-v8 database (schema_migrations 1..8, typed dependencies, one
/// scheduled leaf) so the v8 -> v9 upgrade can be exercised.
fn write_exact_v8_database(path: &Path) {
    let connection = Connection::open(path).expect("create v8");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES
                (1, '2026-08-16T00:00:00.000Z'), (2, '2026-08-16T00:00:00.000Z'),
                (3, '2026-08-16T00:00:00.000Z'), (4, '2026-08-16T00:00:00.000Z'),
                (5, '2026-08-16T00:00:00.000Z'), (6, '2026-08-16T00:00:00.000Z'),
                (7, '2026-08-16T00:00:00.000Z'), (8, '2026-08-16T00:00:00.000Z');
             CREATE TABLE jobs (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL, timezone TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), schedule_start TEXT, calendar_json TEXT NOT NULL DEFAULT '[]', data_date TEXT);
             CREATE TABLE tasks (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, parent_task_id TEXT, sort_key INTEGER NOT NULL CHECK (sort_key >= 0), name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), duration_minutes INTEGER CHECK (duration_minutes >= 0), start_no_earlier_than TEXT, finish_no_later_than TEXT, percent_complete INTEGER, actual_start TEXT, actual_finish TEXT, UNIQUE (job_id, id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, parent_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT);
             CREATE INDEX tasks_job_parent_order ON tasks(job_id, parent_task_id, sort_key, id);
             CREATE UNIQUE INDEX tasks_root_sibling_order ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
             CREATE UNIQUE INDEX tasks_child_sibling_order ON tasks(job_id, parent_task_id, sort_key) WHERE parent_task_id IS NOT NULL;
             CREATE TABLE command_log (command_id TEXT NOT NULL PRIMARY KEY CHECK (length(command_id) BETWEEN 1 AND 128), actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')), client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120), created_at TEXT NOT NULL, summary TEXT NOT NULL CHECK (length(summary) <= 240));
             CREATE TABLE task_dependencies (job_id TEXT NOT NULL, predecessor_task_id TEXT NOT NULL, successor_task_id TEXT NOT NULL, dependency_type TEXT NOT NULL CHECK (dependency_type IN ('FS', 'SS', 'FF', 'SF')), lag_minutes INTEGER NOT NULL, PRIMARY KEY (predecessor_task_id, successor_task_id, dependency_type), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, predecessor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, FOREIGN KEY (job_id, successor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, CHECK (predecessor_task_id <> successor_task_id));
             CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);
             CREATE TABLE baselines (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, name TEXT NOT NULL, created_at TEXT NOT NULL, is_comparison_default INTEGER NOT NULL DEFAULT 0 CHECK (is_comparison_default IN (0, 1)), UNIQUE (job_id, name), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT);
             CREATE TABLE baseline_tasks (baseline_id TEXT NOT NULL, task_id TEXT NOT NULL, start TEXT NOT NULL, finish TEXT NOT NULL, duration_minutes INTEGER NOT NULL CHECK (duration_minutes >= 0), PRIMARY KEY (baseline_id, task_id), FOREIGN KEY (baseline_id) REFERENCES baselines(id) ON DELETE RESTRICT);
             CREATE UNIQUE INDEX baselines_one_default_per_job ON baselines(job_id) WHERE is_comparison_default = 1;
             INSERT INTO jobs VALUES ('job-v8', 'Existing v8', 'draft', 'UTC', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 3, '2026-08-17', '{\"workingWeekdays\":[\"monday\",\"tuesday\",\"wednesday\",\"thursday\",\"friday\"],\"workdayStartMinute\":480,\"workdayDurationMinutes\":480}', NULL);
             INSERT INTO tasks VALUES ('leaf', 'job-v8', NULL, 0, 'Leaf', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, 480, NULL, NULL, NULL, NULL, NULL);",
        )
        .expect("write exact v8");
}

#[test]
fn a_fresh_database_applies_v9_and_later_schema_migrations() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let _service = ApplicationService::open(&path).expect("open");
    let connection = Connection::open(&path).expect("inspect");
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("version");
    assert_eq!(version, 10);
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'calendar_exceptions'",
            [],
            |row| row.get(0),
        )
        .expect("table");
    assert_eq!(table_count, 1);
}

#[test]
fn an_exact_v8_database_upgrades_and_then_accepts_an_exception() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    write_exact_v8_database(&path);
    let service = ApplicationService::open(&path).expect("migrate v8");
    let migrated: i64 = Connection::open(&path)
        .expect("open")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("version");
    assert_eq!(migrated, 10);
    // The migrated job accepts an exception, which shifts its leaf off Monday.
    assert_eq!(
        leaf_start(&service, "job-v8", "leaf"),
        "2026-08-17T08:00:00"
    );
    let version = job_state(&service, "job-v8").version;
    let job = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: "job-v8".into(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("add exception after upgrade");
    assert_eq!(job.calendar_exceptions, vec![MONDAY.to_string()]);
    assert_eq!(
        leaf_start(&service, "job-v8", "leaf"),
        "2026-08-18T08:00:00"
    );
}

#[test]
fn the_current_database_with_an_exception_backs_up_and_restores() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("add");

    let backup_dir = tempfile::tempdir().expect("temp");
    let backup_path = backup_dir.path().join("current.backup.sqlite3");
    let backup = service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("backup");
    assert!(backup.verified);

    let target = backup_dir.path().join("restored-current");
    let result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup_path.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect("restore");
    assert!(result.verified);
    let restored = target.join("contractorproject.sqlite3");
    let restored_version: i64 = Connection::open(&restored)
        .expect("open restored")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("version");
    assert_eq!(restored_version, 10);
    assert_eq!(calendar_exception_count(&restored), 1);
}

// --- Boundary validation independent of schedule computability (item 2) -----

fn bare_job(service: &ApplicationService) -> String {
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Bare".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job")
        .id
}

fn calendar_json(path: &Path, job_id: &str) -> String {
    Connection::open(path)
        .expect("open db")
        .query_row(
            "SELECT calendar_json FROM jobs WHERE id = ?1",
            [job_id],
            |row| row.get::<_, String>(0),
        )
        .expect("calendar json")
}

/// Inserts `count` distinct in-range canonical exception dates for a job directly,
/// bypassing the command layer, to set up the per-job cap boundary cheaply.
fn seed_exceptions(path: &Path, job_id: &str, count: usize) {
    let connection = Connection::open(path).expect("open db");
    let mut day = NaiveDate::from_ymd_opt(2000, 1, 1).expect("date");
    for _ in 0..count {
        connection
            .execute(
                "INSERT INTO calendar_exceptions (job_id, exception_date) VALUES (?1, ?2)",
                rusqlite::params![job_id, day.format("%Y-%m-%d").to_string()],
            )
            .expect("seed exception");
        day += Duration::days(1);
    }
}

#[test]
fn an_out_of_range_year_is_rejected_on_a_start_less_job() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job_id = bare_job(&service); // no schedule start
    let version = job_state(&service, &job_id).version;
    let error = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "1999-12-25".into(),
                expected_job_version: version,
            },
        )
        .expect_err("out of range");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "calendar_exception_out_of_range",
            ..
        }
    ));
    // Nothing was staged, so backup verification stays healthy.
    assert_eq!(calendar_exception_count(&path), 0);
    assert!(
        service
            .create_verified_backup(CreateBackupRequest {
                destination: temp
                    .path()
                    .join("ok.backup.sqlite3")
                    .to_string_lossy()
                    .into_owned(),
            })
            .expect("backup")
            .verified
    );
}

#[test]
fn the_per_job_cap_holds_at_the_command_boundary() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);
    // 3999 pre-seeded rows: the 4000th add is accepted at the boundary.
    seed_exceptions(&path, &job_id, 3999);
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "2099-01-01".into(),
                expected_job_version: version,
            },
        )
        .expect("4000th accepted");
    assert_eq!(calendar_exception_count(&path), 4000);
    // The 4001st is rejected by the transaction-scoped count check.
    let version = job_state(&service, &job_id).version;
    let error = service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "2099-01-02".into(),
                expected_job_version: version,
            },
        )
        .expect_err("4001st rejected");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "calendar_too_many_exceptions",
            ..
        }
    ));
    assert_eq!(calendar_exception_count(&path), 4000);
}

// --- The staged-bad-exception wedge can no longer occur (item 3) -------------

#[test]
fn a_valid_exception_staged_before_a_schedule_start_stays_healthy() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job_id = bare_job(&service);
    // Stage a valid exception while the job has no schedule start.
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("stage exception");
    // Now set the schedule start and a leaf; the schedule computes cleanly.
    let version = job_state(&service, &job_id).version;
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job_id.clone(),
                schedule_start: Some(MONDAY.into()),
                calendar: working_calendar(),
                expected_job_version: version,
            },
        )
        .expect("set start with a staged exception");
    let task = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job_id.clone(),
                parent_task_id: None,
                name: "A".into(),
                expected_job_version: job.version,
            },
        )
        .expect("task");
    service
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
    // get_schedule is healthy and the staged closure shifted the leaf off Monday.
    assert_eq!(
        leaf_start(&service, &job_id, &task.task.id),
        "2026-08-18T08:00:00"
    );
}

#[test]
fn a_staged_exception_that_inverts_later_actuals_is_caught_at_statusing() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, task_id) = job_with_leaf(&service);
    // Stage an exception on a Thursday.
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: "2026-08-20".into(),
                expected_job_version: version,
            },
        )
        .expect("stage exception");
    // Complete the leaf with equal actuals on the now-closed Thursday: the staged
    // exception inverts normalization, so the statusing edit rejects.
    let leaf = service
        .list_tasks(&job_id)
        .expect("tasks")
        .tasks
        .into_iter()
        .find(|task| task.id == task_id)
        .expect("leaf");
    let version = job_state(&service, &job_id).version;
    service
        .update_job_data_date(
            command_context(),
            UpdateJobDataDateRequest {
                job_id: job_id.clone(),
                data_date: Some("2026-08-21".into()),
                expected_job_version: version,
            },
        )
        .expect("data date");
    let version = job_state(&service, &job_id).version;
    let error = service
        .update_task_progress(
            command_context(),
            UpdateTaskProgressRequest {
                task_id: task_id.clone(),
                clear: false,
                percent_complete: Some(100),
                actual_start: Some("2026-08-20".into()),
                actual_finish: Some("2026-08-20".into()),
                expected_version: leaf.version,
                expected_job_version: version,
            },
        )
        .expect_err("inverting actuals rejected at statusing");
    assert!(matches!(
        error,
        ApplicationError::ValidationFailed {
            code: "progress_normalized_order",
            ..
        }
    ));
}

// --- calendar_json stays exception-free (item 4) ----------------------------

#[test]
fn update_schedule_strips_client_supplied_exceptions_from_calendar_json() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let (job_id, _task_id) = job_with_leaf(&service);

    // A client attaches exceptions to the calendar in an update_schedule call.
    let mut calendar = working_calendar();
    calendar.exceptions = vec![NaiveDate::from_ymd_opt(2026, 8, 25).expect("date")];
    let version = job_state(&service, &job_id).version;
    let job = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job_id.clone(),
                schedule_start: Some(MONDAY.into()),
                calendar,
                expected_job_version: version,
            },
        )
        .expect("update schedule");
    // The v9 table is the single source of truth: nothing was persisted there and
    // calendar_json carries no exceptions.
    assert!(job.calendar_exceptions.is_empty());
    assert_eq!(calendar_exception_count(&path), 0);
    assert!(!calendar_json(&path, &job_id).contains("exception"));

    // Adding then removing a real exception also leaves calendar_json untouched.
    let version = job_state(&service, &job_id).version;
    service
        .add_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("add");
    assert!(!calendar_json(&path, &job_id).contains("exception"));
    let version = job_state(&service, &job_id).version;
    service
        .remove_calendar_exception(
            command_context(),
            CalendarExceptionRequest {
                job_id: job_id.clone(),
                date: MONDAY.into(),
                expected_job_version: version,
            },
        )
        .expect("remove");
    assert!(!calendar_json(&path, &job_id).contains("exception"));
}
