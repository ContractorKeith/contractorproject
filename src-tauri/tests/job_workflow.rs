use contractorproject_lib::application::{
    AddDependencyRequest, RemoveDependencyRequest, UpdateScheduleRequest, UpdateTaskDurationRequest,
};
use contractorproject_lib::application::{
    ApplicationError, ApplicationService, ArchiveJobRequest, CommandActor, CommandContext,
    CreateBackupRequest, CreateJobRequest, CreateTaskRequest, JobStatus, RestoreJobRequest,
    VerifyRestoreRequest,
};
use contractorproject_lib::application::{
    ReorderTaskRequest, TaskConstraintKind, UpdateTaskConstraintRequest, UpdateTaskRequest,
};
use contractorproject_lib::scheduling::{CalendarWeekday, DependencyType, WorkingCalendar};
use rusqlite::Connection;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Barrier,
};

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

fn command_log_count(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .expect("open audit database")
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count command log")
}

fn canonical_snapshot(path: &std::path::Path) -> Vec<Vec<String>> {
    let connection = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open snapshot read-only");
    [
        "SELECT printf('%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q', id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version) FROM jobs ORDER BY id",
        "SELECT printf('%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q', id, job_id, parent_task_id, sort_key, name, duration_minutes, start_no_earlier_than, finish_no_later_than, created_at, updated_at, version) FROM tasks ORDER BY id",
        "SELECT printf('%Q|%Q|%Q|%Q', job_id, predecessor_task_id, successor_task_id, lag_minutes) FROM task_dependencies ORDER BY job_id, predecessor_task_id, successor_task_id",
        "SELECT printf('%Q|%Q|%Q|%Q|%Q', command_id, actor, client_name, created_at, summary) FROM command_log ORDER BY command_id",
    ]
    .into_iter()
    .map(|query| {
        connection
            .prepare(query)
            .expect("prepare canonical snapshot")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query canonical snapshot")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect canonical snapshot")
    })
    .collect()
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

#[test]
fn task_constraints_are_versioned_audited_atomic_and_persisted() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Constraints".into(),
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
        .expect("task")
        .task;
    let duration = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: task.id.clone(),
                duration_minutes: Some(480),
                expected_version: task.version,
                expected_job_version: job.version + 1,
            },
        )
        .expect("duration");
    let set = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-20".into()),
                expected_version: duration.task.version,
                expected_job_version: duration.job_version,
            },
        )
        .expect("set constraint");
    assert_eq!(
        set.task.start_no_earlier_than.as_deref(),
        Some("2026-08-20")
    );
    let both = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::FinishNoLaterThan,
                value: Some("2026-08-21".into()),
                expected_version: set.task.version,
                expected_job_version: set.job_version,
            },
        )
        .expect("set second constraint");
    assert_eq!(
        both.task.start_no_earlier_than.as_deref(),
        Some("2026-08-20")
    );
    assert_eq!(
        both.task.finish_no_later_than.as_deref(),
        Some("2026-08-21")
    );
    let replaced = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-21".into()),
                expected_version: both.task.version,
                expected_job_version: both.job_version,
            },
        )
        .expect("replace constraint");
    assert_eq!(
        replaced.task.start_no_earlier_than.as_deref(),
        Some("2026-08-21")
    );
    assert_eq!(
        replaced.task.finish_no_later_than.as_deref(),
        Some("2026-08-21")
    );
    let cleared = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::FinishNoLaterThan,
                value: None,
                expected_version: replaced.task.version,
                expected_job_version: replaced.job_version,
            },
        )
        .expect("clear constraint");
    assert_eq!(cleared.task.finish_no_later_than, None);
    let summary: String = Connection::open(&path)
        .expect("open audit database")
        .query_row(
            "SELECT summary FROM command_log ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("constraint audit summary");
    assert_eq!(summary, "updated task constraint");
    assert!(!summary.contains("2026-08-21"));
    assert!(!summary.contains("Leaf"));
    let before_version_failures = canonical_snapshot(&path);
    assert!(matches!(
        service.update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-22".into()),
                expected_version: cleared.task.version - 1,
                expected_job_version: cleared.job_version,
            }
        ),
        Err(ApplicationError::VersionConflict {
            resource: "task",
            ..
        })
    ));
    assert!(matches!(
        service.update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-22".into()),
                expected_version: cleared.task.version,
                expected_job_version: cleared.job_version - 1,
            }
        ),
        Err(ApplicationError::VersionConflict {
            resource: "job",
            ..
        })
    ));
    assert!(matches!(
        service.update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: "missing-task".into(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-22".into()),
                expected_version: 1,
                expected_job_version: cleared.job_version,
            }
        ),
        Err(ApplicationError::NotFound {
            resource: "task",
            ..
        })
    ));
    assert_eq!(canonical_snapshot(&path), before_version_failures);
    let duplicate_context = CommandContext {
        command_id: "constraint-duplicate".into(),
        actor: CommandActor::Agent,
        client_name: "integration-test".into(),
    };
    let duplicate_first = service
        .update_task_constraint(
            duplicate_context.clone(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-22".into()),
                expected_version: cleared.task.version,
                expected_job_version: cleared.job_version,
            },
        )
        .expect("first command");
    let before_duplicate = canonical_snapshot(&path);
    assert!(matches!(
        service.update_task_constraint(
            duplicate_context,
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-23".into()),
                expected_version: duplicate_first.task.version,
                expected_job_version: duplicate_first.job_version,
            }
        ),
        Err(ApplicationError::DuplicateCommand { .. })
    ));
    assert_eq!(canonical_snapshot(&path), before_duplicate);
    let before_leaf_invariants = canonical_snapshot(&path);
    assert!(matches!(
        service.create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(task.id.clone()),
                name: "Forbidden child".into(),
                expected_job_version: duplicate_first.job_version,
            }
        ),
        Err(ApplicationError::ValidationFailed {
            code: "summary_conversion_requires_cleanup",
            ..
        })
    ));
    assert!(matches!(
        service.update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: task.id.clone(),
                duration_minutes: None,
                expected_version: duplicate_first.task.version,
                expected_job_version: duplicate_first.job_version,
            }
        ),
        Err(ApplicationError::ValidationFailed {
            code: "summary_constraint",
            ..
        })
    ));
    assert_eq!(canonical_snapshot(&path), before_leaf_invariants);
    let other = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Other leaf".into(),
                expected_job_version: duplicate_first.job_version,
            },
        )
        .expect("other leaf")
        .task;
    let before_reparent = canonical_snapshot(&path);
    assert!(matches!(
        service.reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: other.id,
                new_parent_task_id: Some(task.id.clone()),
                new_sibling_index: 0,
                expected_version: other.version,
                expected_job_version: duplicate_first.job_version + 1,
            }
        ),
        Err(ApplicationError::ValidationFailed {
            code: "summary_conversion_requires_cleanup",
            ..
        })
    ));
    assert_eq!(canonical_snapshot(&path), before_reparent);
    let incomplete = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Incomplete leaf".into(),
                expected_job_version: duplicate_first.job_version + 1,
            },
        )
        .expect("incomplete leaf");
    let before_full_schedule_rejection = canonical_snapshot(&path);
    assert!(matches!(
        service.update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-23".into()),
                expected_version: duplicate_first.task.version,
                expected_job_version: incomplete.job_version,
            }
        ),
        Err(ApplicationError::ValidationFailed {
            field: "schedule",
            ..
        })
    ));
    assert_eq!(canonical_snapshot(&path), before_full_schedule_rejection);
    let completed_incomplete = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: incomplete.task.id,
                duration_minutes: Some(480),
                expected_version: incomplete.task.version,
                expected_job_version: incomplete.job_version,
            },
        )
        .expect("complete second leaf");
    let before = canonical_snapshot(&path);
    let error = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("bad".into()),
                expected_version: duplicate_first.task.version,
                expected_job_version: completed_incomplete.job_version,
            },
        )
        .expect_err("bad date");
    assert!(matches!(
        error,
        ApplicationError::InvalidInput { field: "value", .. }
    ));
    assert_eq!(canonical_snapshot(&path), before);
    let error = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-8-1".into()),
                expected_version: duplicate_first.task.version,
                expected_job_version: completed_incomplete.job_version,
            },
        )
        .expect_err("reject noncanonical date");
    assert!(matches!(
        error,
        ApplicationError::InvalidInput { field: "value", .. }
    ));
    assert_eq!(canonical_snapshot(&path), before);
    let connection = Connection::open(&path).expect("open trigger database");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_constraint_audit BEFORE INSERT ON command_log
             WHEN NEW.summary = 'updated task constraint'
             BEGIN SELECT RAISE(ABORT, 'forced audit failure'); END;",
        )
        .expect("install trigger");
    drop(connection);
    let before_audit_failure = canonical_snapshot(&path);
    assert!(service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: task.id.clone(),
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: None,
                expected_version: duplicate_first.task.version,
                expected_job_version: completed_incomplete.job_version,
            },
        )
        .is_err());
    assert_eq!(canonical_snapshot(&path), before_audit_failure);
    drop(service);
    let reopened = ApplicationService::open(&path).expect("reopen");
    let tasks = reopened.list_tasks(&job.id).expect("tasks");
    assert_eq!(
        tasks.tasks[0].start_no_earlier_than.as_deref(),
        Some("2026-08-22")
    );
    assert_eq!(tasks.tasks[0].finish_no_later_than.as_deref(), None);
}

#[test]
fn summary_task_constraints_are_rejected_without_drift() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Summary constraints".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let parent = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Summary".into(),
                expected_job_version: job.version,
            },
        )
        .expect("parent")
        .task;
    let child = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(parent.id.clone()),
                name: "Leaf".into(),
                expected_job_version: job.version + 1,
            },
        )
        .expect("child");
    let before = canonical_snapshot(&path);
    assert!(matches!(
        service.update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: parent.id,
                kind: TaskConstraintKind::StartNoEarlierThan,
                value: Some("2026-08-20".into()),
                expected_version: 1,
                expected_job_version: child.job_version,
            }
        ),
        Err(ApplicationError::ValidationFailed {
            code: "summary_constraint",
            field: "taskId",
            ..
        })
    ));
    assert_eq!(canonical_snapshot(&path), before);
}

#[test]
fn persisted_schedule_inputs_validate_atomically_and_survive_restart() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Schedule".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let first = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "First".into(),
                expected_job_version: 1,
            },
        )
        .expect("first")
        .task;
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Second".into(),
                expected_job_version: 2,
            },
        )
        .expect("second")
        .task;
    let audit_before_invalid = command_log_count(&path);
    let invalid = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: WorkingCalendar {
                    working_weekdays: vec![],
                    workday_start_minute: 480,
                    workday_duration_minutes: 480,
                },
                expected_job_version: 3,
            },
        )
        .expect_err("invalid calendar");
    assert_eq!(invalid.kind(), "validation_failed");
    let invalid_date = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("not-a-date".into()),
                calendar: working_calendar(),
                expected_job_version: 3,
            },
        )
        .expect_err("invalid date");
    assert_eq!(invalid_date.kind(), "invalid_input");
    let invalid_interval = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: WorkingCalendar {
                    working_weekdays: vec![CalendarWeekday::Monday],
                    workday_start_minute: 1_400,
                    workday_duration_minutes: 60,
                },
                expected_job_version: 3,
            },
        )
        .expect_err("invalid work interval");
    assert_eq!(invalid_interval.kind(), "validation_failed");
    assert_eq!(
        service.list_tasks(&job.id).expect("unchanged").job_version,
        3
    );
    assert_eq!(command_log_count(&path), audit_before_invalid);
    let first = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: first.id.clone(),
                duration_minutes: Some(480),
                expected_version: 1,
                expected_job_version: 3,
            },
        )
        .expect("duration")
        .task;
    let second = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: second.id.clone(),
                duration_minutes: Some(0),
                expected_version: 1,
                expected_job_version: 4,
            },
        )
        .expect("milestone")
        .task;
    let linked = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.id.clone(),
                successor_task_id: second.id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: 5,
            },
        )
        .expect("dependency");
    assert_eq!(linked.dependencies.len(), 1);
    let stale = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: second.id.clone(),
                successor_task_id: first.id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: 5,
            },
        )
        .expect_err("stale");
    assert_eq!(stale.kind(), "version_conflict");
    let blocked = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: first.id.clone(),
                duration_minutes: None,
                expected_version: first.version,
                expected_job_version: 6,
            },
        )
        .expect_err("endpoint duration");
    assert_eq!(blocked.kind(), "validation_failed");
    let schedule = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: 6,
            },
        )
        .expect("schedule");
    let duplicate_context = command_context();
    let schedule = service
        .update_schedule(
            duplicate_context.clone(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-18".into()),
                calendar: working_calendar(),
                expected_job_version: schedule.version,
            },
        )
        .expect("second schedule update");
    let duplicate = service
        .update_schedule(
            duplicate_context,
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-19".into()),
                calendar: working_calendar(),
                expected_job_version: schedule.version,
            },
        )
        .expect_err("duplicate command ID");
    assert_eq!(duplicate.kind(), "duplicate_command");
    drop(service);
    let reopened = ApplicationService::open(&path).expect("reopen");
    assert_eq!(reopened.list_jobs().expect("jobs"), vec![schedule.clone()]);
    assert_eq!(
        reopened
            .list_tasks(&job.id)
            .expect("links")
            .dependencies
            .len(),
        1
    );
    let removed = reopened
        .remove_dependency(
            command_context(),
            RemoveDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.id,
                successor_task_id: second.id,
                dependency_type: None,
                expected_job_version: schedule.version,
            },
        )
        .expect("remove");
    assert!(removed.dependencies.is_empty());
}

#[test]
fn get_schedule_rebuilds_the_same_gantt_projection_after_reopen() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Read schedule".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let summary = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: 1,
            },
        )
        .expect("summary")
        .task;
    let activity = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(summary.id.clone()),
                name: "Excavate".into(),
                expected_job_version: 2,
            },
        )
        .expect("activity")
        .task;
    let milestone = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Inspection".into(),
                expected_job_version: 3,
            },
        )
        .expect("milestone")
        .task;
    let activity = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: activity.id.clone(),
                duration_minutes: Some(480),
                expected_version: activity.version,
                expected_job_version: 4,
            },
        )
        .expect("activity duration")
        .task;
    let milestone = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: milestone.id.clone(),
                duration_minutes: Some(0),
                expected_version: milestone.version,
                expected_job_version: 5,
            },
        )
        .expect("milestone duration")
        .task;
    let scheduled = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: 6,
            },
        )
        .expect("schedule settings");
    service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: activity.id.clone(),
                successor_task_id: milestone.id,
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: scheduled.version,
            },
        )
        .expect("dependency");

    let constrained = service
        .update_task_constraint(
            command_context(),
            UpdateTaskConstraintRequest {
                task_id: activity.id.clone(),
                kind: TaskConstraintKind::FinishNoLaterThan,
                value: Some("2026-08-16".into()),
                expected_version: activity.version,
                expected_job_version: 8,
            },
        )
        .expect("deadline constraint")
        .task;

    let before = service.get_schedule(&job.id).expect("schedule read model");
    assert_eq!(before.rows.len(), 3);
    assert_eq!(before.schedule_start.to_string(), "2026-08-17 08:00:00");
    assert_eq!(before.schedule_finish.to_string(), "2026-08-17 16:00:00");
    assert_eq!(before.rows[0].task_id, summary.id);
    assert!(before.rows[0].summary);
    assert_eq!(before.rows[0].wbs, "1");
    assert_eq!(before.rows[0].depth, 0);
    assert_eq!(before.rows[0].total_float_minutes, -480);
    assert!(before.rows[0].critical);
    assert!(before.rows[0].start_no_earlier_than.is_none());
    assert!(before.rows[0].finish_no_later_than.is_none());
    assert!(before.rows[0].constraint_violated);
    assert_eq!(before.rows[1].wbs, "1.1");
    assert_eq!(before.rows[1].depth, 1);
    assert_eq!(before.rows[1].start.to_string(), "2026-08-17 08:00:00");
    assert_eq!(before.rows[1].finish.to_string(), "2026-08-17 16:00:00");
    assert_eq!(before.rows[1].total_float_minutes, -480);
    assert!(before.rows[1].critical);
    assert_eq!(
        before.rows[1].finish_no_later_than,
        Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 16).expect("date"))
    );
    assert!(before.rows[1].constraint_violated);
    assert!(before.rows[2].milestone);
    assert_eq!(before.rows[2].start, before.rows[2].finish);
    assert_eq!(before.rows[2].start.to_string(), "2026-08-17 16:00:00");
    assert_eq!(before.rows[2].total_float_minutes, 0);
    assert!(before.rows[2].critical);
    assert_eq!(
        before.rows[2].predecessor_ids,
        vec![before.rows[1].task_id.clone()]
    );
    assert_eq!(before.critical_path, vec![before.rows[1].task_id.clone()]);
    drop(service);

    let after = ApplicationService::open(&path)
        .expect("reopen")
        .get_schedule(&job.id)
        .expect("reopened schedule read model");
    assert_eq!(after, before);
    assert_eq!(constrained.id, activity.id);
}

#[test]
fn persisted_dependency_validation_rejects_invalid_graph_changes_atomically() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Graph".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let mut job_version = job.version;
    let mut tasks = Vec::new();
    for name in ["First", "Second", "Third", "Unscheduled"] {
        let mutation = service
            .create_task(
                command_context(),
                CreateTaskRequest {
                    job_id: job.id.clone(),
                    parent_task_id: None,
                    name: name.into(),
                    expected_job_version: job_version,
                },
            )
            .expect("task");
        job_version = mutation.job_version;
        tasks.push(mutation.task);
    }
    for (index, task) in tasks.iter_mut().take(3).enumerate() {
        let mutation = service
            .update_task_duration(
                command_context(),
                UpdateTaskDurationRequest {
                    task_id: task.id.clone(),
                    duration_minutes: Some(if index == 2 { 0 } else { 480 }),
                    expected_version: task.version,
                    expected_job_version: job_version,
                },
            )
            .expect("duration");
        job_version = mutation.job_version;
        *task = mutation.task;
    }

    let before = service.list_tasks(&job.id).expect("before failures");
    let audit_before = command_log_count(&path);
    let cases = [
        (
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[0].id.clone(),
                successor_task_id: tasks[0].id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: job_version,
            },
            "validation_failed",
        ),
        (
            // Negative lag is legal now; an unknown type code is the rejection.
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[0].id.clone(),
                successor_task_id: tasks[1].id.clone(),
                lag_minutes: -1,
                dependency_type: Some("XX".into()),
                expected_job_version: job_version,
            },
            "invalid_input",
        ),
        (
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[0].id.clone(),
                successor_task_id: tasks[3].id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: job_version,
            },
            "validation_failed",
        ),
    ];
    for (request, expected_kind) in cases {
        assert_eq!(
            service
                .add_dependency(command_context(), request)
                .expect_err("invalid edge")
                .kind(),
            expected_kind
        );
        assert_eq!(service.list_tasks(&job.id).expect("unchanged"), before);
        assert_eq!(command_log_count(&path), audit_before);
    }

    let other = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Other".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("other job");
    let other_task = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: other.id.clone(),
                parent_task_id: None,
                name: "Other task".into(),
                expected_job_version: other.version,
            },
        )
        .expect("other task")
        .task;
    let cross_job_audit = command_log_count(&path);
    assert_eq!(
        service
            .add_dependency(
                command_context(),
                AddDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: tasks[0].id.clone(),
                    successor_task_id: other_task.id,
                    lag_minutes: 0,
                    dependency_type: None,
                    expected_job_version: job_version,
                },
            )
            .expect_err("cross-job endpoint")
            .kind(),
        "validation_failed"
    );
    assert_eq!(
        service.list_tasks(&job.id).expect("cross-job unchanged"),
        before
    );
    assert_eq!(command_log_count(&path), cross_job_audit);

    let first_edge = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[0].id.clone(),
                successor_task_id: tasks[1].id.clone(),
                lag_minutes: 60,
                dependency_type: None,
                expected_job_version: job_version,
            },
        )
        .expect("first edge");
    job_version = first_edge.job_version;
    let duplicate_before = first_edge.clone();
    let duplicate_audit = command_log_count(&path);
    assert_eq!(
        service
            .add_dependency(
                command_context(),
                AddDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: tasks[0].id.clone(),
                    successor_task_id: tasks[1].id.clone(),
                    lag_minutes: 60,
                    dependency_type: None,
                    expected_job_version: job_version,
                },
            )
            .expect_err("duplicate edge")
            .kind(),
        "validation_failed"
    );
    assert_eq!(
        service.list_tasks(&job.id).expect("duplicate unchanged"),
        duplicate_before
    );
    assert_eq!(command_log_count(&path), duplicate_audit);

    let chain = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[1].id.clone(),
                successor_task_id: tasks[2].id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: job_version,
            },
        )
        .expect("second edge");
    let cycle_audit = command_log_count(&path);
    assert_eq!(
        service
            .add_dependency(
                command_context(),
                AddDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: tasks[2].id.clone(),
                    successor_task_id: tasks[0].id.clone(),
                    lag_minutes: 0,
                    dependency_type: None,
                    expected_job_version: chain.job_version,
                },
            )
            .expect_err("cycle")
            .kind(),
        "validation_failed"
    );
    assert_eq!(service.list_tasks(&job.id).expect("cycle unchanged"), chain);
    assert_eq!(command_log_count(&path), cycle_audit);

    let hierarchy_audit = command_log_count(&path);
    assert_eq!(
        service
            .create_task(
                command_context(),
                CreateTaskRequest {
                    job_id: job.id.clone(),
                    parent_task_id: Some(tasks[0].id.clone()),
                    name: "Invalid child".into(),
                    expected_job_version: chain.job_version,
                },
            )
            .expect_err("scheduled leaf cannot become summary")
            .kind(),
        "validation_failed"
    );
    assert_eq!(
        service
            .reorder_task(
                command_context(),
                ReorderTaskRequest {
                    task_id: tasks[3].id.clone(),
                    new_parent_task_id: Some(tasks[0].id.clone()),
                    new_sibling_index: 0,
                    expected_version: tasks[3].version,
                    expected_job_version: chain.job_version,
                },
            )
            .expect_err("scheduled leaf cannot receive moved child")
            .kind(),
        "validation_failed"
    );
    assert_eq!(
        service.list_tasks(&job.id).expect("hierarchy unchanged"),
        chain
    );
    assert_eq!(command_log_count(&path), hierarchy_audit);
}

#[test]
fn created_job_is_available_after_reopening_the_database() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");

    let service = ApplicationService::open(&database_path).expect("open application service");
    let created = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    drop(service);

    let reopened = ApplicationService::open(&database_path).expect("reopen application service");
    let jobs = reopened.list_jobs().expect("list jobs");

    assert_eq!(jobs, vec![created]);
}

#[test]
fn nested_tasks_are_listed_in_sibling_order_after_reopening_the_database() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");

    let root = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: 1,
            },
        )
        .expect("create root task")
        .task;
    let child = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(root.id.clone()),
                name: "Layout".into(),
                expected_job_version: 2,
            },
        )
        .expect("create child task")
        .task;
    let second_root = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Closeout".into(),
                expected_job_version: 3,
            },
        )
        .expect("create second root task")
        .task;
    drop(service);

    let reopened = ApplicationService::open(&database_path).expect("reopen application service");
    let hierarchy = reopened.list_tasks(&job.id).expect("list tasks");
    let tasks = hierarchy.tasks;

    assert_eq!(hierarchy.job_version, 4);
    assert_eq!(
        tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec![root.id.as_str(), child.id.as_str(), second_root.id.as_str()]
    );
    assert_eq!(tasks[0].parent_task_id, None);
    assert_eq!(tasks[0].sort_key, 0);
    assert_eq!(tasks[0].version, 1);
    assert_eq!(tasks[1].parent_task_id.as_deref(), Some(root.id.as_str()));
    assert_eq!(tasks[1].sort_key, 0);
    assert_eq!(tasks[2].sort_key, 1);
}

#[test]
fn task_creation_rejects_a_stale_job_version() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: 1,
            },
        )
        .expect("create first task");

    let error = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id,
                parent_task_id: None,
                name: "Closeout".into(),
                expected_job_version: 1,
            },
        )
        .expect_err("reject stale job version");

    assert_eq!(error.kind(), "version_conflict");
    assert!(error.to_string().contains("current version 2"));
}

#[test]
fn task_edit_increments_versions_and_rejects_a_stale_task_version() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    let created = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: 1,
            },
        )
        .expect("create task");

    let updated = service
        .update_task(
            command_context(),
            UpdateTaskRequest {
                task_id: created.task.id.clone(),
                name: "  Site preparation  ".into(),
                expected_version: 1,
            },
        )
        .expect("edit task");

    assert_eq!(updated.task.name, "Site preparation");
    assert_eq!(updated.task.version, 2);
    assert_eq!(updated.job_version, 3);

    let error = service
        .update_task(
            command_context(),
            UpdateTaskRequest {
                task_id: created.task.id,
                name: "Stale overwrite".into(),
                expected_version: 1,
            },
        )
        .expect_err("reject stale task version");
    assert_eq!(error.kind(), "version_conflict");
    assert!(error.to_string().contains("current version 2"));

    let hierarchy = service.list_tasks(&job.id).expect("list tasks");
    assert_eq!(hierarchy.job_version, 3);
    assert_eq!(hierarchy.tasks, vec![updated.task]);
}

#[test]
fn reordered_task_hierarchy_and_shifted_sibling_versions_survive_restart() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    let root = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: 1,
            },
        )
        .expect("create root")
        .task;
    let first_child = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(root.id.clone()),
                name: "Layout".into(),
                expected_job_version: 2,
            },
        )
        .expect("create first child")
        .task;
    let second_child = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(root.id.clone()),
                name: "Excavation".into(),
                expected_job_version: 3,
            },
        )
        .expect("create second child")
        .task;
    let closeout = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Closeout".into(),
                expected_job_version: 4,
            },
        )
        .expect("create closeout")
        .task;

    let reordered = service
        .reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: closeout.id.clone(),
                new_parent_task_id: Some(root.id.clone()),
                new_sibling_index: 1,
                expected_version: 1,
                expected_job_version: 5,
            },
        )
        .expect("reparent closeout");

    assert_eq!(reordered.job_version, 6);
    assert_eq!(
        reordered
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            root.id.as_str(),
            first_child.id.as_str(),
            closeout.id.as_str(),
            second_child.id.as_str(),
        ]
    );
    assert_eq!(
        reordered.tasks[2].parent_task_id.as_deref(),
        Some(root.id.as_str())
    );
    assert_eq!(reordered.tasks[2].sort_key, 1);
    assert_eq!(reordered.tasks[2].version, 2);
    assert_eq!(reordered.tasks[3].sort_key, 2);
    assert_eq!(reordered.tasks[3].version, 2);
    drop(service);

    let reopened = ApplicationService::open(&database_path).expect("reopen application service");
    assert_eq!(reopened.list_tasks(&job.id).expect("list tasks"), reordered);
}

#[test]
fn cross_job_parent_is_rejected_without_changing_the_destination_job() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let service = ApplicationService::open(temp.path().join("contractorproject.sqlite3"))
        .expect("open application service");
    let first_job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "First job".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create first job");
    let second_job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Second job".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create second job");
    let first_job_task = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: first_job.id,
                parent_task_id: None,
                name: "First job task".into(),
                expected_job_version: 1,
            },
        )
        .expect("create first job task")
        .task;

    let error = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: second_job.id.clone(),
                parent_task_id: Some(first_job_task.id),
                name: "Invalid child".into(),
                expected_job_version: 1,
            },
        )
        .expect_err("reject cross-job parent");

    assert_eq!(error.kind(), "validation_failed");
    let hierarchy = service
        .list_tasks(&second_job.id)
        .expect("list second job tasks");
    assert_eq!(hierarchy.job_version, 1);
    assert!(hierarchy.tasks.is_empty());
}

#[test]
fn parent_cycle_is_rejected_without_changing_the_hierarchy() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let service = ApplicationService::open(temp.path().join("contractorproject.sqlite3"))
        .expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    let parent = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Parent".into(),
                expected_job_version: 1,
            },
        )
        .expect("create parent")
        .task;
    let child = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(parent.id.clone()),
                name: "Child".into(),
                expected_job_version: 2,
            },
        )
        .expect("create child")
        .task;
    let before = service.list_tasks(&job.id).expect("list initial hierarchy");

    let error = service
        .reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: parent.id,
                new_parent_task_id: Some(child.id),
                new_sibling_index: 0,
                expected_version: 1,
                expected_job_version: 3,
            },
        )
        .expect_err("reject parent cycle");

    assert_eq!(error.kind(), "validation_failed");
    assert_eq!(service.list_tasks(&job.id).expect("list hierarchy"), before);
}

#[test]
fn invalid_reorder_index_rolls_back_order_and_versions() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let service = ApplicationService::open(temp.path().join("contractorproject.sqlite3"))
        .expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    let first = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "First".into(),
                expected_job_version: 1,
            },
        )
        .expect("create first task")
        .task;
    service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Second".into(),
                expected_job_version: 2,
            },
        )
        .expect("create second task");
    let before = service.list_tasks(&job.id).expect("list initial hierarchy");

    let error = service
        .reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: first.id,
                new_parent_task_id: None,
                new_sibling_index: 2,
                expected_version: 1,
                expected_job_version: 3,
            },
        )
        .expect_err("reject out-of-range index");

    assert_eq!(error.kind(), "invalid_input");
    assert_eq!(service.list_tasks(&job.id).expect("list hierarchy"), before);
}

#[test]
fn reorder_rejects_a_stale_job_version_without_changing_task_versions() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let service = ApplicationService::open(temp.path().join("contractorproject.sqlite3"))
        .expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "First".into(),
                expected_job_version: 1,
            },
        )
        .expect("create first task");
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Second".into(),
                expected_job_version: 2,
            },
        )
        .expect("create second task")
        .task;
    let before = service.list_tasks(&job.id).expect("list initial hierarchy");

    let error = service
        .reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: second.id,
                new_parent_task_id: None,
                new_sibling_index: 0,
                expected_version: 1,
                expected_job_version: 2,
            },
        )
        .expect_err("reject stale job version");

    assert_eq!(error.kind(), "version_conflict");
    assert!(error.to_string().contains("current version 3"));
    assert_eq!(service.list_tasks(&job.id).expect("list hierarchy"), before);
}

#[test]
fn version_one_database_is_backed_up_before_the_task_migration() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let connection = Connection::open(&database_path).expect("create version one database");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timezone TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL CHECK (version > 0)
             );
             INSERT INTO schema_migrations (version, applied_at)
             VALUES (1, '2026-08-14T00:00:00.000Z');
             INSERT INTO jobs (
                id, name, status, timezone, created_at, updated_at, version
             ) VALUES (
                'job-v1', 'Existing job', 'draft', 'America/New_York',
                '2026-08-14T00:00:00.000Z', '2026-08-14T00:00:00.000Z', 1
             );",
        )
        .expect("write version one schema");
    drop(connection);

    let service = ApplicationService::open(&database_path).expect("migrate application service");
    assert_eq!(service.list_jobs().expect("list migrated jobs").len(), 1);
    let migrated = Connection::open(&database_path).expect("open migrated database");
    let migrated_version: i64 = migrated
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("read migrated schema version");
    let command_log_tables: i64 = migrated
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'command_log'",
            [],
            |row| row.get(0),
        )
        .expect("inspect command audit schema");
    assert_eq!(migrated_version, 8);
    assert_eq!(command_log_tables, 1);
    let backup_path = temp
        .path()
        .join("contractorproject.sqlite3.pre-migration-v2.bak");
    assert!(backup_path.is_file(), "pre-migration backup should exist");
    let backup = Connection::open(backup_path).expect("open pre-migration backup");
    let migration: i64 = backup
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("read backup migration version");
    let task_tables: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'tasks'",
            [],
            |row| row.get(0),
        )
        .expect("inspect backup schema");
    assert_eq!(migration, 1);
    assert_eq!(task_tables, 0);
}

#[test]
fn version_three_database_is_backed_up_and_migrates_schedule_inputs() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let connection = Connection::open(&database_path).expect("create v3 database");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             INSERT INTO schema_migrations (version, applied_at) VALUES
                (1, '2026-08-14T00:00:00.000Z'),
                (2, '2026-08-14T00:00:00.000Z'),
                (3, '2026-08-14T00:00:00.000Z');
             CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timezone TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL CHECK (version > 0)
             );
             CREATE TABLE tasks (
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
                FOREIGN KEY (job_id, parent_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT
             );
             CREATE TABLE command_log (
                command_id TEXT NOT NULL PRIMARY KEY CHECK (length(command_id) BETWEEN 1 AND 128),
                actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')),
                client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120),
                created_at TEXT NOT NULL,
                summary TEXT NOT NULL CHECK (length(summary) <= 240)
             );
             INSERT INTO jobs (id, name, status, timezone, created_at, updated_at, version)
             VALUES ('job-v3', 'Existing schedule', 'draft', 'UTC', '2026-08-14T00:00:00.000Z', '2026-08-14T00:00:00.000Z', 2);
             INSERT INTO tasks (id, job_id, parent_task_id, sort_key, name, created_at, updated_at, version)
             VALUES ('task-v3', 'job-v3', NULL, 0, 'Existing task', '2026-08-14T00:00:00.000Z', '2026-08-14T00:00:00.000Z', 1);",
        )
        .expect("write v3 schema");
    drop(connection);

    let service = ApplicationService::open(&database_path).expect("migrate v3");
    let migrated_job = service.list_jobs().expect("jobs").pop().expect("job");
    assert_eq!(migrated_job.schedule_start, None);
    assert_eq!(migrated_job.calendar, working_calendar());
    let hierarchy = service.list_tasks(&migrated_job.id).expect("tasks");
    assert_eq!(hierarchy.tasks[0].duration_minutes, None);
    assert!(hierarchy.dependencies.is_empty());

    let backup_path = temp
        .path()
        .join("contractorproject.sqlite3.pre-migration-v4.bak");
    assert!(
        backup_path.is_file(),
        "v4 pre-migration backup should exist"
    );
    let backup = Connection::open(backup_path).expect("open v3 backup");
    let migration: i64 = backup
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("backup version");
    let schedule_columns: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('jobs') WHERE name = 'schedule_start'",
            [],
            |row| row.get(0),
        )
        .expect("backup columns");
    assert_eq!(migration, 3);
    assert_eq!(schedule_columns, 0);
}

#[test]
fn populated_exact_v4_backup_restores_read_only_then_owned_target_migrates_to_v5() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let target = temp.path().join("restored");
    let connection = Connection::open(&path).expect("create v4");
    connection.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
         INSERT INTO schema_migrations VALUES (1, '2026-08-16T00:00:00.000Z'), (2, '2026-08-16T00:00:00.000Z'), (3, '2026-08-16T00:00:00.000Z'), (4, '2026-08-16T00:00:00.000Z');
         CREATE TABLE jobs (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL, timezone TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), schedule_start TEXT, calendar_json TEXT NOT NULL DEFAULT '[]');
         CREATE TABLE tasks (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, parent_task_id TEXT, sort_key INTEGER NOT NULL CHECK (sort_key >= 0), name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), duration_minutes INTEGER CHECK (duration_minutes >= 0), UNIQUE (job_id, id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, parent_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT);
         CREATE INDEX tasks_job_parent_order ON tasks(job_id, parent_task_id, sort_key, id);
         CREATE UNIQUE INDEX tasks_root_sibling_order ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
         CREATE UNIQUE INDEX tasks_child_sibling_order ON tasks(job_id, parent_task_id, sort_key) WHERE parent_task_id IS NOT NULL;
         CREATE TABLE command_log (command_id TEXT NOT NULL PRIMARY KEY CHECK (length(command_id) BETWEEN 1 AND 128), actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')), client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120), created_at TEXT NOT NULL, summary TEXT NOT NULL CHECK (length(summary) <= 240));
         CREATE TABLE task_dependencies (job_id TEXT NOT NULL, predecessor_task_id TEXT NOT NULL, successor_task_id TEXT NOT NULL, lag_minutes INTEGER NOT NULL CHECK (lag_minutes >= 0), PRIMARY KEY (predecessor_task_id, successor_task_id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, predecessor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, FOREIGN KEY (job_id, successor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, CHECK (predecessor_task_id <> successor_task_id));
         CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);
         INSERT INTO jobs VALUES ('job-v4', 'Existing v4', 'draft', 'UTC', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 4, '2026-08-17', '{\"workingWeekdays\":[\"monday\",\"tuesday\",\"wednesday\",\"thursday\",\"friday\"],\"workdayStartMinute\":480,\"workdayDurationMinutes\":480}');
         INSERT INTO tasks VALUES ('summary', 'job-v4', NULL, 0, 'Summary', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, NULL), ('first', 'job-v4', 'summary', 0, 'First', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 2, 480), ('second', 'job-v4', NULL, 1, 'Second', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, 480);
         INSERT INTO task_dependencies VALUES ('job-v4', 'first', 'second', 0);
         INSERT INTO command_log VALUES ('v4-audit', 'agent', 'test', '2026-08-16T00:00:00.000Z', 'updated task duration');",
    ).expect("write exact v4");
    drop(connection);

    let service = ApplicationService::open(&path).expect("migrate source");
    let hierarchy = service.list_tasks("job-v4").expect("preserved tasks");
    assert_eq!(hierarchy.tasks.len(), 3);
    assert!(hierarchy
        .tasks
        .iter()
        .all(|task| task.start_no_earlier_than.is_none() && task.finish_no_later_than.is_none()));
    assert_eq!(hierarchy.dependencies.len(), 1);
    let v4_backup = temp
        .path()
        .join("contractorproject.sqlite3.pre-migration-v5.bak");
    let backup =
        Connection::open_with_flags(&v4_backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open v4 backup read-only");
    assert_eq!(
        backup
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                .get::<_, i64>(
                0
            ))
            .expect("v4 version"),
        4
    );
    assert_eq!(
        backup
            .query_row("SELECT COUNT(*) FROM command_log", [], |row| row
                .get::<_, i64>(0))
            .expect("v4 audit"),
        1
    );
    drop(backup);
    let result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: v4_backup.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect("restore v4 backup");
    assert_eq!(
        (
            result.job_count,
            result.task_count,
            result.dependency_count,
            result.command_log_count
        ),
        (1, 3, 1, 1)
    );
    let backup_after_restore =
        Connection::open_with_flags(&v4_backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("reopen v4 source backup");
    assert_eq!(
        backup_after_restore
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                .get::<_, i64>(
                0
            ))
            .expect("unchanged v4 source version"),
        4
    );
    drop(backup_after_restore);
    let restored =
        Connection::open(target.join("contractorproject.sqlite3")).expect("open restored v5");
    assert_eq!(
        restored
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                .get::<_, i64>(
                0
            ))
            .expect("current version"),
        8
    );
    assert_eq!(restored.query_row("SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name IN ('start_no_earlier_than', 'finish_no_later_than')", [], |row| row.get::<_, i64>(0)).expect("constraint fields"), 2);
}

#[test]
fn command_audit_is_atomic_bounded_and_does_not_include_request_content() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let context = CommandContext {
        command_id: "agent-create-job-1".into(),
        actor: CommandActor::Agent,
        client_name: "contractorproject-mcp".into(),
    };
    let secret_like_name = "Client password: do-not-record";
    let job = service
        .create_job(
            context.clone(),
            CreateJobRequest {
                name: secret_like_name.into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");

    let duplicate = service
        .create_job(
            context,
            CreateJobRequest {
                name: "Should not be created".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect_err("reject duplicate command ID");
    assert_eq!(duplicate.kind(), "duplicate_command");

    let rejected_context = command_context();
    let rejected = service.create_task(
        rejected_context,
        CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Should roll back".into(),
            expected_job_version: 99,
        },
    );
    assert_eq!(
        rejected.expect_err("reject stale job version").kind(),
        "version_conflict"
    );
    drop(service);

    let connection = Connection::open(database_path).expect("open database for audit inspection");
    let audit_rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count audit rows");
    let (actor, client_name, summary): (String, String, String) = connection
        .query_row(
            "SELECT actor, client_name, summary FROM command_log WHERE command_id = ?1",
            ["agent-create-job-1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read audit row");
    let task_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .expect("count tasks");
    assert_eq!(audit_rows, 1, "duplicates and rollbacks leave no audit row");
    assert_eq!(task_count, 0, "failed command leaves no domain mutation");
    assert_eq!(actor, "agent");
    assert_eq!(client_name, "contractorproject-mcp");
    assert_eq!(summary, "created job");
    assert!(summary.chars().count() <= 240);
    assert!(!summary.contains(secret_like_name));
}

#[test]
fn task_mutations_write_one_audit_record_but_noop_and_failed_reorders_write_none() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Ridgeline Fence — Phase 2".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect("create job");
    let root = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: 1,
            },
        )
        .expect("create task")
        .task;
    let updated = service
        .update_task(
            command_context(),
            UpdateTaskRequest {
                task_id: root.id.clone(),
                name: "Site preparation".into(),
                expected_version: 1,
            },
        )
        .expect("update task")
        .task;
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Closeout".into(),
                expected_job_version: 3,
            },
        )
        .expect("create second task")
        .task;
    let reordered = service
        .reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: second.id.clone(),
                new_parent_task_id: None,
                new_sibling_index: 0,
                expected_version: second.version,
                expected_job_version: 4,
            },
        )
        .expect("reorder task");
    let before_noop = reordered.clone();
    let updated_after_reorder = reordered
        .tasks
        .iter()
        .find(|task| task.id == updated.id)
        .expect("updated task remains in hierarchy")
        .version;
    let noop = service
        .reorder_task(
            command_context(),
            ReorderTaskRequest {
                task_id: second.id.clone(),
                new_parent_task_id: None,
                new_sibling_index: 0,
                expected_version: 2,
                expected_job_version: 5,
            },
        )
        .expect("accept noop reorder");
    assert_eq!(noop, before_noop, "no-op must not change the hierarchy");
    let failed = service.reorder_task(
        command_context(),
        ReorderTaskRequest {
            task_id: updated.id,
            new_parent_task_id: None,
            new_sibling_index: 2,
            expected_version: updated_after_reorder,
            expected_job_version: 5,
        },
    );
    assert_eq!(
        failed.expect_err("reject invalid index").kind(),
        "invalid_input"
    );
    drop(service);

    let connection = Connection::open(database_path).expect("open database for audit inspection");
    let (audit_rows, tasks): (i64, i64) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM command_log), (SELECT COUNT(*) FROM tasks)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read audit and task counts");
    assert_eq!(tasks, 2);
    assert_eq!(
        audit_rows, 5,
        "job plus four successful task mutations only"
    );
}

#[test]
fn command_log_schema_rejects_missing_or_empty_identity_fields() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    drop(service);
    let connection = Connection::open(database_path).expect("open database for schema checks");

    for statement in [
        "INSERT INTO command_log (command_id, actor, client_name, created_at, summary)
         VALUES (NULL, 'user', 'desktop-ui', '2026-08-16T00:00:00.000Z', 'test')",
        "INSERT INTO command_log (command_id, actor, client_name, created_at, summary)
         VALUES ('', 'user', 'desktop-ui', '2026-08-16T00:00:00.000Z', 'test')",
        "INSERT INTO command_log (command_id, actor, client_name, created_at, summary)
         VALUES ('schema-check', 'user', '', '2026-08-16T00:00:00.000Z', 'test')",
    ] {
        assert!(
            connection.execute(statement, []).is_err(),
            "identity constraint should reject: {statement}"
        );
    }
}

#[test]
fn schedule_input_schema_rejects_negative_and_cross_job_rows() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open");
    let first_job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "First".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("first job");
    let first = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: first_job.id.clone(),
                parent_task_id: None,
                name: "First task".into(),
                expected_job_version: first_job.version,
            },
        )
        .expect("first task")
        .task;
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: first_job.id.clone(),
                parent_task_id: None,
                name: "Second task".into(),
                expected_job_version: 2,
            },
        )
        .expect("second task")
        .task;
    let other_job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Other".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("other job");
    let other = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: other_job.id.clone(),
                parent_task_id: None,
                name: "Other task".into(),
                expected_job_version: other_job.version,
            },
        )
        .expect("other task")
        .task;
    drop(service);

    let connection = Connection::open(database_path).expect("open schema database");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("foreign keys");
    assert!(connection
        .execute(
            "UPDATE tasks SET duration_minutes = -1 WHERE id = ?1",
            [&first.id]
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO task_dependencies (job_id, predecessor_task_id, successor_task_id, lag_minutes) VALUES (?1, ?2, ?2, 0)",
            rusqlite::params![first_job.id, first.id],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO task_dependencies (job_id, predecessor_task_id, successor_task_id, lag_minutes) VALUES (?1, ?2, ?3, -1)",
            rusqlite::params![first_job.id, first.id, second.id],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO task_dependencies (job_id, predecessor_task_id, successor_task_id, lag_minutes) VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![first_job.id, first.id, other.id],
        )
        .is_err());
}

#[test]
fn audit_insert_failure_rolls_back_the_domain_mutation() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open application service");
    let connection = Connection::open(&database_path).expect("open database for failure trigger");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_command_audit
             BEFORE INSERT ON command_log
             BEGIN
                 SELECT RAISE(ABORT, 'forced audit failure');
             END;",
        )
        .expect("install audit failure trigger");
    drop(connection);

    let error = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Must roll back".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect_err("audit failure should reject the mutation");
    assert_eq!(error.kind(), "storage_unavailable");
    drop(service);

    let connection = Connection::open(database_path).expect("inspect rolled-back transaction");
    let job_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .expect("count jobs");
    let audit_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count audit rows");
    assert_eq!(job_count, 0, "domain insert must roll back");
    assert_eq!(audit_count, 0, "audit insert must leave no partial row");
}

#[test]
fn archive_and_restore_are_atomic_recoverable_and_preserve_schedule_inputs() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Recoverable schedule".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let summary = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Site work".into(),
                expected_job_version: job.version,
            },
        )
        .expect("summary");
    let activity = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(summary.task.id.clone()),
                name: "Excavate".into(),
                expected_job_version: summary.job_version,
            },
        )
        .expect("activity");
    let milestone = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Inspect".into(),
                expected_job_version: activity.job_version,
            },
        )
        .expect("milestone");
    let activity = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: activity.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: activity.task.version,
                expected_job_version: milestone.job_version,
            },
        )
        .expect("activity duration");
    let milestone = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: milestone.task.id.clone(),
                duration_minutes: Some(0),
                expected_version: milestone.task.version,
                expected_job_version: activity.job_version,
            },
        )
        .expect("milestone duration");
    let scheduled = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: milestone.job_version,
            },
        )
        .expect("schedule");
    let linked = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: activity.task.id.clone(),
                successor_task_id: milestone.task.id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: scheduled.version,
            },
        )
        .expect("dependency");
    let hierarchy_before = service.list_tasks(&job.id).expect("hierarchy");
    let schedule_before = service.get_schedule(&job.id).expect("schedule projection");
    let active_jobs_before = service.list_jobs().expect("active jobs");
    let audit_before = command_log_count(&database_path);

    let unknown = service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: "missing-job".into(),
                expected_job_version: 1,
            },
        )
        .expect_err("unknown job");
    assert_eq!(unknown.kind(), "not_found");
    let stale = service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: linked.job_version - 1,
            },
        )
        .expect_err("stale version");
    assert_eq!(stale.kind(), "version_conflict");
    let invalid_restore = service
        .restore_job(
            command_context(),
            RestoreJobRequest {
                job_id: job.id.clone(),
                expected_job_version: linked.job_version,
            },
        )
        .expect_err("draft job cannot be restored");
    assert_eq!(invalid_restore.kind(), "validation_failed");
    let unknown_restore = service
        .restore_job(
            command_context(),
            RestoreJobRequest {
                job_id: "missing-job".into(),
                expected_job_version: 1,
            },
        )
        .expect_err("unknown restore job");
    assert_eq!(unknown_restore.kind(), "not_found");
    assert_eq!(
        service.list_jobs().expect("unchanged active jobs"),
        active_jobs_before
    );
    assert_eq!(
        service.list_tasks(&job.id).expect("unchanged hierarchy"),
        hierarchy_before
    );
    assert_eq!(
        service.get_schedule(&job.id).expect("unchanged schedule"),
        schedule_before
    );
    assert_eq!(command_log_count(&database_path), audit_before);

    let archive_context = command_context();
    let archived = service
        .archive_job(
            archive_context.clone(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: linked.job_version,
            },
        )
        .expect("archive");
    assert_eq!(archived.status, JobStatus::Archived);
    let stale_restore = service
        .restore_job(
            command_context(),
            RestoreJobRequest {
                job_id: job.id.clone(),
                expected_job_version: archived.version - 1,
            },
        )
        .expect_err("stale restore version");
    assert_eq!(stale_restore.kind(), "version_conflict");
    let duplicate_archive = service
        .archive_job(
            archive_context,
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: archived.version,
            },
        )
        .expect_err("duplicate archive command");
    assert_eq!(duplicate_archive.kind(), "duplicate_command");
    assert!(service.list_jobs().expect("active jobs").is_empty());
    assert_eq!(
        service
            .list_jobs_by_status(JobStatus::Archived)
            .expect("archived jobs"),
        vec![archived.clone()]
    );
    let repeated = service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: archived.version,
            },
        )
        .expect_err("already archived");
    assert_eq!(repeated.kind(), "validation_failed");
    let archived_hierarchy_before_reopen = service.list_tasks(&job.id).expect("archived hierarchy");
    let archived_schedule_before_reopen = service.get_schedule(&job.id).expect("archived schedule");
    assert_eq!(
        service
            .list_jobs_by_status(JobStatus::Archived)
            .expect("unchanged archived jobs"),
        vec![archived.clone()]
    );
    assert_eq!(command_log_count(&database_path), audit_before + 1);
    drop(service);

    let reopened = ApplicationService::open(&database_path).expect("reopen");
    let archived_hierarchy = reopened.list_tasks(&job.id).expect("preserved hierarchy");
    assert_eq!(archived_hierarchy, archived_hierarchy_before_reopen);
    assert_eq!(archived_hierarchy.tasks, hierarchy_before.tasks);
    assert_eq!(
        archived_hierarchy.dependencies,
        hierarchy_before.dependencies
    );
    let archived_schedule = reopened.get_schedule(&job.id).expect("preserved schedule");
    assert_eq!(archived_schedule, archived_schedule_before_reopen);
    assert_eq!(archived_schedule.rows, schedule_before.rows);
    assert_eq!(
        archived_schedule.schedule_start,
        schedule_before.schedule_start
    );
    assert_eq!(
        archived_schedule.schedule_finish,
        schedule_before.schedule_finish
    );
    assert_eq!(
        archived_schedule.critical_path,
        schedule_before.critical_path
    );
    let restore_context = command_context();
    let restored = reopened
        .restore_job(
            restore_context.clone(),
            RestoreJobRequest {
                job_id: job.id.clone(),
                expected_job_version: archived.version,
            },
        )
        .expect("restore");
    assert_eq!(restored.status, JobStatus::Draft);
    let restored_hierarchy_before_duplicate =
        reopened.list_tasks(&job.id).expect("restored hierarchy");
    let restored_schedule_before_duplicate =
        reopened.get_schedule(&job.id).expect("restored schedule");
    let duplicate_restore = reopened
        .restore_job(
            restore_context,
            RestoreJobRequest {
                job_id: job.id.clone(),
                expected_job_version: restored.version,
            },
        )
        .expect_err("duplicate restore command");
    assert_eq!(duplicate_restore.kind(), "duplicate_command");
    let repeated_restore = reopened
        .restore_job(
            command_context(),
            RestoreJobRequest {
                job_id: job.id.clone(),
                expected_job_version: restored.version,
            },
        )
        .expect_err("already restored");
    assert_eq!(repeated_restore.kind(), "validation_failed");
    assert_eq!(reopened.list_jobs().expect("active jobs"), vec![restored]);
    assert!(reopened
        .list_jobs_by_status(JobStatus::Archived)
        .expect("archived jobs")
        .is_empty());
    let restored_hierarchy = reopened.list_tasks(&job.id).expect("restored hierarchy");
    assert_eq!(restored_hierarchy, restored_hierarchy_before_duplicate);
    assert_eq!(restored_hierarchy.tasks, hierarchy_before.tasks);
    assert_eq!(
        restored_hierarchy.dependencies,
        hierarchy_before.dependencies
    );
    let restored_schedule = reopened.get_schedule(&job.id).expect("restored schedule");
    assert_eq!(restored_schedule, restored_schedule_before_duplicate);
    assert_eq!(restored_schedule.rows, schedule_before.rows);
    assert_eq!(
        restored_schedule.schedule_start,
        schedule_before.schedule_start
    );
    assert_eq!(
        restored_schedule.schedule_finish,
        schedule_before.schedule_finish
    );
    assert_eq!(
        restored_schedule.critical_path,
        schedule_before.critical_path
    );
}

#[test]
fn archive_audit_failure_rolls_back_status_and_version() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Atomic archive".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let audit_before = command_log_count(&database_path);
    Connection::open(&database_path)
        .expect("trigger connection")
        .execute_batch(
            "CREATE TRIGGER reject_archive_audit
             BEFORE INSERT ON command_log
             BEGIN
                 SELECT RAISE(ABORT, 'forced audit failure');
             END;",
        )
        .expect("install trigger");

    let failure = service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: job.version,
            },
        )
        .expect_err("audit failure");
    assert_eq!(failure.kind(), "storage_unavailable");
    assert_eq!(service.list_jobs().expect("unchanged jobs"), vec![job]);
    assert_eq!(command_log_count(&database_path), audit_before);
}

#[test]
fn archived_jobs_reject_every_normal_mutation_without_drift() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Immutable archive".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let first = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "First".into(),
                expected_job_version: job.version,
            },
        )
        .expect("first");
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Second".into(),
                expected_job_version: first.job_version,
            },
        )
        .expect("second");
    let first = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: first.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: first.task.version,
                expected_job_version: second.job_version,
            },
        )
        .expect("first duration");
    let second = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: second.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: second.task.version,
                expected_job_version: first.job_version,
            },
        )
        .expect("second duration");
    let linked = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: second.job_version,
            },
        )
        .expect("dependency");
    let archived = service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: linked.job_version,
            },
        )
        .expect("archive");
    let hierarchy_before = service.list_tasks(&job.id).expect("hierarchy");
    let archived_jobs_before = service
        .list_jobs_by_status(JobStatus::Archived)
        .expect("archived jobs");
    let audit_before = command_log_count(&database_path);

    let failures = [
        service
            .create_task(
                command_context(),
                CreateTaskRequest {
                    job_id: job.id.clone(),
                    parent_task_id: None,
                    name: "Blocked".into(),
                    expected_job_version: archived.version,
                },
            )
            .map(|_| ()),
        service
            .update_task(
                command_context(),
                UpdateTaskRequest {
                    task_id: first.task.id.clone(),
                    name: "Blocked".into(),
                    expected_version: first.task.version,
                },
            )
            .map(|_| ()),
        service
            .reorder_task(
                command_context(),
                ReorderTaskRequest {
                    task_id: first.task.id.clone(),
                    new_parent_task_id: None,
                    new_sibling_index: 1,
                    expected_version: first.task.version,
                    expected_job_version: archived.version,
                },
            )
            .map(|_| ()),
        service
            .update_schedule(
                command_context(),
                UpdateScheduleRequest {
                    job_id: job.id.clone(),
                    schedule_start: Some("2026-08-17".into()),
                    calendar: working_calendar(),
                    expected_job_version: archived.version,
                },
            )
            .map(|_| ()),
        service
            .update_task_duration(
                command_context(),
                UpdateTaskDurationRequest {
                    task_id: first.task.id.clone(),
                    duration_minutes: Some(600),
                    expected_version: first.task.version,
                    expected_job_version: archived.version,
                },
            )
            .map(|_| ()),
        service
            .add_dependency(
                command_context(),
                AddDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: second.task.id.clone(),
                    successor_task_id: first.task.id.clone(),
                    lag_minutes: 0,
                    dependency_type: None,
                    expected_job_version: archived.version,
                },
            )
            .map(|_| ()),
        service
            .remove_dependency(
                command_context(),
                RemoveDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: first.task.id.clone(),
                    successor_task_id: second.task.id.clone(),
                    dependency_type: None,
                    expected_job_version: archived.version,
                },
            )
            .map(|_| ()),
    ];
    for failure in failures {
        let error = failure.expect_err("archived job must be immutable");
        assert_eq!(error.kind(), "validation_failed");
        assert_eq!(
            error.to_string(),
            "restore the job before changing its tasks or schedule"
        );
        assert_eq!(
            service.list_tasks(&job.id).expect("unchanged hierarchy"),
            hierarchy_before
        );
        assert_eq!(
            service
                .list_jobs_by_status(JobStatus::Archived)
                .expect("unchanged archive"),
            archived_jobs_before
        );
        assert_eq!(command_log_count(&database_path), audit_before);
    }
}

#[test]
fn restore_audit_failure_rolls_back_status_and_version() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Atomic restore".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let archived = service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: job.version,
            },
        )
        .expect("archive");
    let audit_before = command_log_count(&database_path);
    Connection::open(&database_path)
        .expect("trigger connection")
        .execute_batch(
            "CREATE TRIGGER reject_restore_audit
             BEFORE INSERT ON command_log
             BEGIN
                 SELECT RAISE(ABORT, 'forced audit failure');
             END;",
        )
        .expect("install trigger");

    let failure = service
        .restore_job(
            command_context(),
            RestoreJobRequest {
                job_id: job.id.clone(),
                expected_job_version: archived.version,
            },
        )
        .expect_err("audit failure");
    assert_eq!(failure.kind(), "storage_unavailable");
    assert!(service.list_jobs().expect("active jobs").is_empty());
    assert_eq!(
        service
            .list_jobs_by_status(JobStatus::Archived)
            .expect("unchanged archived jobs"),
        vec![archived]
    );
    assert_eq!(command_log_count(&database_path), audit_before);
}

#[test]
fn audit_failure_rolls_back_every_schedule_input_mutation() {
    let temp = tempfile::tempdir().expect("temp");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Atomic schedule".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let mut job_version = job.version;
    let mut tasks = Vec::new();
    for name in ["First", "Second", "Third"] {
        let mutation = service
            .create_task(
                command_context(),
                CreateTaskRequest {
                    job_id: job.id.clone(),
                    parent_task_id: None,
                    name: name.into(),
                    expected_job_version: job_version,
                },
            )
            .expect("task");
        job_version = mutation.job_version;
        tasks.push(mutation.task);
    }
    for task in &mut tasks {
        let mutation = service
            .update_task_duration(
                command_context(),
                UpdateTaskDurationRequest {
                    task_id: task.id.clone(),
                    duration_minutes: Some(480),
                    expected_version: task.version,
                    expected_job_version: job_version,
                },
            )
            .expect("duration");
        job_version = mutation.job_version;
        *task = mutation.task;
    }
    let linked = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[0].id.clone(),
                successor_task_id: tasks[1].id.clone(),
                lag_minutes: 0,
                dependency_type: None,
                expected_job_version: job_version,
            },
        )
        .expect("existing dependency");
    job_version = linked.job_version;
    let jobs_before = service.list_jobs().expect("jobs before");
    let hierarchy_before = service.list_tasks(&job.id).expect("hierarchy before");
    let audit_before = command_log_count(&database_path);

    Connection::open(&database_path)
        .expect("open trigger connection")
        .execute_batch(
            "CREATE TRIGGER reject_schedule_audit
             BEFORE INSERT ON command_log
             BEGIN
                 SELECT RAISE(ABORT, 'forced audit failure');
             END;",
        )
        .expect("install trigger");

    let failures = [
        service
            .update_schedule(
                command_context(),
                UpdateScheduleRequest {
                    job_id: job.id.clone(),
                    schedule_start: Some("2026-08-17".into()),
                    calendar: working_calendar(),
                    expected_job_version: job_version,
                },
            )
            .map(|_| ()),
        service
            .update_task_duration(
                command_context(),
                UpdateTaskDurationRequest {
                    task_id: tasks[0].id.clone(),
                    duration_minutes: Some(600),
                    expected_version: tasks[0].version,
                    expected_job_version: job_version,
                },
            )
            .map(|_| ()),
        service
            .add_dependency(
                command_context(),
                AddDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: tasks[1].id.clone(),
                    successor_task_id: tasks[2].id.clone(),
                    lag_minutes: 0,
                    dependency_type: None,
                    expected_job_version: job_version,
                },
            )
            .map(|_| ()),
        service
            .remove_dependency(
                command_context(),
                RemoveDependencyRequest {
                    job_id: job.id.clone(),
                    predecessor_task_id: tasks[0].id.clone(),
                    successor_task_id: tasks[1].id.clone(),
                    dependency_type: None,
                    expected_job_version: job_version,
                },
            )
            .map(|_| ()),
    ];
    for failure in failures {
        assert_eq!(
            failure.expect_err("audit failure").kind(),
            "storage_unavailable"
        );
        assert_eq!(service.list_jobs().expect("jobs unchanged"), jobs_before);
        assert_eq!(
            service.list_tasks(&job.id).expect("hierarchy unchanged"),
            hierarchy_before
        );
        assert_eq!(command_log_count(&database_path), audit_before);
    }
}

#[test]
fn command_context_rejects_oversized_identifiers_before_mutating() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let service = ApplicationService::open(temp.path().join("contractorproject.sqlite3"))
        .expect("open application service");
    let error = service
        .create_job(
            CommandContext {
                command_id: "x".repeat(129),
                actor: CommandActor::Import,
                client_name: "importer".into(),
            },
            CreateJobRequest {
                name: "No mutation".into(),
                timezone: "America/New_York".into(),
            },
        )
        .expect_err("reject oversized command ID");
    assert_eq!(error.kind(), "invalid_input");
    assert!(service.list_jobs().expect("list jobs").is_empty());
}

#[test]
fn online_backup_preserves_a_consistent_populated_wal_snapshot_without_audit_writes() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let backup_path = temp.path().join("scheduled-job.backup.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Archived scheduled job".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create job");
    let summary = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Summary".into(),
                expected_job_version: job.version,
            },
        )
        .expect("create summary");
    let activity = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(summary.task.id.clone()),
                name: "Activity".into(),
                expected_job_version: summary.job_version,
            },
        )
        .expect("create nested activity");
    let milestone = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Milestone".into(),
                expected_job_version: activity.job_version,
            },
        )
        .expect("create milestone");
    let activity = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: activity.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: activity.task.version,
                expected_job_version: milestone.job_version,
            },
        )
        .expect("set duration");
    let milestone = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: milestone.task.id.clone(),
                duration_minutes: Some(0),
                expected_version: milestone.task.version,
                expected_job_version: activity.job_version,
            },
        )
        .expect("set milestone");
    let linked = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: activity.task.id.clone(),
                successor_task_id: milestone.task.id.clone(),
                lag_minutes: 30,
                dependency_type: None,
                expected_job_version: milestone.job_version,
            },
        )
        .expect("add dependency");
    let scheduled = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: linked.job_version,
            },
        )
        .expect("schedule job");
    let projection = service.get_schedule(&job.id).expect("build schedule");
    service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: scheduled.version,
            },
        )
        .expect("archive job");
    let active = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Active job".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create active job");
    assert_eq!(active.status, JobStatus::Draft);

    let source_connection = Connection::open(&database_path).expect("open source");
    let journal_mode: String = source_connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read journal mode");
    assert_eq!(journal_mode, "wal");
    let audit_before_backup = command_log_count(&database_path);

    let backup = service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("create verified online backup");
    assert_eq!(backup.destination, backup_path.to_string_lossy());
    assert!(backup.created_at_utc.ends_with('Z'));
    assert!(backup.byte_size > 0);
    assert!(backup.verified);
    assert_eq!(command_log_count(&database_path), audit_before_backup);
    assert!(
        std::fs::read_dir(temp.path())
            .expect("list backup directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".scheduled-job.backup.sqlite3.")),
        "published backup must not leave owned incomplete SQLite sidecars"
    );

    let backup_connection =
        Connection::open_with_flags(&backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open backup read-only");
    let archived_count: i64 = backup_connection
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE status = 'archived'",
            [],
            |row| row.get(0),
        )
        .expect("archived job in backup");
    let active_count: i64 = backup_connection
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE status = 'draft'",
            [],
            |row| row.get(0),
        )
        .expect("active job in backup");
    let nested_task_count: i64 = backup_connection
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE parent_task_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("nested task in backup");
    let dependency_count: i64 = backup_connection
        .query_row("SELECT COUNT(*) FROM task_dependencies", [], |row| {
            row.get(0)
        })
        .expect("dependency in backup");
    assert_eq!(
        (
            archived_count,
            active_count,
            nested_task_count,
            dependency_count
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(command_log_count(&backup_path), audit_before_backup);

    let reopened = ApplicationService::open(&backup_path).expect("open backup after verification");
    let backed_up_projection = reopened
        .get_schedule(&job.id)
        .expect("preserved projection");
    assert_eq!(backed_up_projection.rows, projection.rows);
    assert_eq!(
        backed_up_projection.schedule_start,
        projection.schedule_start
    );
    assert_eq!(
        backed_up_projection.schedule_finish,
        projection.schedule_finish
    );
    assert_eq!(backed_up_projection.critical_path, projection.critical_path);
}

#[test]
fn backup_rejects_existing_destination_without_overwrite_or_live_mutation() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let backup_path = temp.path().join("existing.backup.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open service");
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Live data".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create job");
    std::fs::write(&backup_path, b"do not replace").expect("reserve existing file");
    let before_destination = std::fs::read(&backup_path).expect("read destination");
    let audit_before = command_log_count(&database_path);

    let error = service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect_err("must not overwrite existing destination");
    assert_eq!(error.kind(), "backup_destination_exists");
    assert_eq!(error.to_string(), "backup destination already exists");
    assert_eq!(
        std::fs::read(&backup_path).expect("read destination"),
        before_destination
    );
    assert_eq!(command_log_count(&database_path), audit_before);
    assert_eq!(
        service.list_jobs().expect("live database unchanged").len(),
        1
    );
}

#[test]
fn online_backup_completes_while_a_wal_reader_holds_a_snapshot() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let backup_path = temp.path().join("concurrent.backup.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open service");
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Concurrent reader".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create job");

    let reader = Connection::open(&database_path).expect("open concurrent reader");
    reader
        .execute_batch("PRAGMA journal_mode = WAL; BEGIN;")
        .expect("begin WAL read transaction");
    let rows_visible_to_reader: i64 = reader
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .expect("read snapshot");

    let backup = service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("create backup with concurrent reader");
    assert!(backup.verified);
    assert_eq!(
        reader
            .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get::<_, i64>(0))
            .expect("reader keeps its snapshot"),
        rows_visible_to_reader
    );
    reader
        .execute_batch("COMMIT")
        .expect("close read transaction");
}

#[test]
fn backup_failure_does_not_leave_a_destination_or_mutate_the_live_database() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let destination = temp.path().join("missing-parent").join("backup.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open service");
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Live data".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create job");
    let audit_before = command_log_count(&database_path);

    let error = service
        .create_verified_backup(CreateBackupRequest {
            destination: destination.to_string_lossy().into_owned(),
        })
        .expect_err("reject unavailable destination");
    assert_eq!(error.kind(), "backup_failed");
    assert_eq!(error.to_string(), "backup could not be created");
    assert!(!destination.exists());
    assert_eq!(command_log_count(&database_path), audit_before);
    assert_eq!(
        service.list_jobs().expect("live database unchanged").len(),
        1
    );
}

#[test]
fn online_backup_is_a_complete_snapshot_before_a_concurrent_wal_writer_commits() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");
    let backup_path = temp.path().join("writer-snapshot.backup.sqlite3");
    let service = ApplicationService::open(&database_path).expect("open service");
    let writer = Connection::open(&database_path).expect("open concurrent writer");
    writer
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; BEGIN IMMEDIATE;")
        .expect("begin WAL write transaction");
    writer
        .execute(
            "INSERT INTO jobs (
                id, name, status, timezone, schedule_start, calendar_json, created_at, updated_at, version
             ) VALUES (?1, ?2, 'draft', ?3, NULL, ?4, ?5, ?5, 1)",
            rusqlite::params![
                "writer-job",
                "Uncommitted job",
                "UTC",
                serde_json::to_string(&working_calendar()).expect("serialize calendar"),
                "2026-08-16T00:00:00.000Z",
            ],
        )
        .expect("write uncommitted job");
    writer
        .execute(
            "INSERT INTO command_log (command_id, actor, client_name, created_at, summary)
             VALUES (?1, 'user', 'test', ?2, 'created job')",
            rusqlite::params!["writer-command", "2026-08-16T00:00:00.000Z"],
        )
        .expect("write uncommitted audit row");

    service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("backup while WAL writer is open");
    writer.execute_batch("COMMIT").expect("commit writer");

    let backup =
        Connection::open_with_flags(&backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open backup");
    let backup_jobs: i64 = backup
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .expect("count backed up jobs");
    let backup_audit: i64 = backup
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count backed up audit rows");
    assert_eq!((backup_jobs, backup_audit), (0, 0));
    assert_eq!(service.list_jobs().expect("live job after commit").len(), 1);
    assert_eq!(command_log_count(&database_path), 1);
}

#[test]
fn restore_verification_activates_only_a_verified_backup_point_in_fresh_app_data() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let database_path = temp.path().join("source.sqlite3");
    let backup_path = temp.path().join("scheduled-job.backup.sqlite3");
    let target_app_data_dir = temp.path().join("restored-app-data");
    let service = ApplicationService::open(&database_path).expect("open source service");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Archived schedule".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create scheduled job");
    let summary = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Summary".into(),
                expected_job_version: job.version,
            },
        )
        .expect("create summary");
    let activity = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: Some(summary.task.id.clone()),
                name: "Activity".into(),
                expected_job_version: summary.job_version,
            },
        )
        .expect("create activity");
    let milestone = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Milestone".into(),
                expected_job_version: activity.job_version,
            },
        )
        .expect("create milestone");
    let activity = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: activity.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: activity.task.version,
                expected_job_version: milestone.job_version,
            },
        )
        .expect("set activity duration");
    let milestone = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: milestone.task.id.clone(),
                duration_minutes: Some(0),
                expected_version: milestone.task.version,
                expected_job_version: activity.job_version,
            },
        )
        .expect("set milestone duration");
    let dependency = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: activity.task.id.clone(),
                successor_task_id: milestone.task.id.clone(),
                lag_minutes: 30,
                dependency_type: None,
                expected_job_version: milestone.job_version,
            },
        )
        .expect("add dependency");
    let scheduled = service
        .update_schedule(
            command_context(),
            UpdateScheduleRequest {
                job_id: job.id.clone(),
                schedule_start: Some("2026-08-17".into()),
                calendar: working_calendar(),
                expected_job_version: dependency.job_version,
            },
        )
        .expect("schedule job");
    service
        .archive_job(
            command_context(),
            ArchiveJobRequest {
                job_id: job.id.clone(),
                expected_job_version: scheduled.version,
            },
        )
        .expect("archive job");
    let expected_projection = service
        .get_schedule(&job.id)
        .expect("build archived schedule at backup point");
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Active at backup point".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create active job");
    let audit_before_backup = command_log_count(&database_path);
    service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("create verified backup");
    let backup_snapshot = canonical_snapshot(&backup_path);

    // This mutation occurs after backup creation and must not appear in the
    // restored target.
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Created after backup".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("mutate source after backup");
    assert_ne!(canonical_snapshot(&database_path), backup_snapshot);

    let result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup_path.to_string_lossy().into_owned(),
            target_app_data_dir: target_app_data_dir.to_string_lossy().into_owned(),
        })
        .expect("verify restore into fresh app data");
    assert_eq!(
        result,
        contractorproject_lib::application::RestoreVerificationResult {
            verified: true,
            job_count: 2,
            task_count: 3,
            dependency_count: 1,
            command_log_count: audit_before_backup,
        }
    );
    let restored_database_path = target_app_data_dir.join("contractorproject.sqlite3");
    assert_eq!(canonical_snapshot(&restored_database_path), backup_snapshot);
    let restored =
        ApplicationService::open(&restored_database_path).expect("open restored service");
    assert_eq!(
        restored.get_schedule(&job.id).expect("restored schedule"),
        expected_projection
    );
    drop(restored);
    let reopened =
        ApplicationService::open(&restored_database_path).expect("reopen restored service");
    assert_eq!(canonical_snapshot(&restored_database_path), backup_snapshot);
    assert_eq!(
        reopened.get_schedule(&job.id).expect("reopened schedule"),
        expected_projection
    );
}

#[test]
fn restore_verification_rejects_invalid_backups_without_activating_or_mutating_targets() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let source_path = temp.path().join("source.sqlite3");
    let backup_path = temp.path().join("valid.backup.sqlite3");
    let service = ApplicationService::open(&source_path).expect("open source service");
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Source job".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create source job");
    service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("create valid backup");
    let source_before = canonical_snapshot(&source_path);

    for (name, bytes) in [
        ("corrupt", b"not sqlite".as_slice()),
        ("truncated", b"SQLite".as_slice()),
    ] {
        let input = temp.path().join(format!("{name}.sqlite3"));
        std::fs::write(&input, bytes).expect("write invalid input");
        let input_before = std::fs::read(&input).expect("read invalid input");
        let target = temp.path().join(format!("{name}-target"));
        let error = service
            .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
                backup_path: input.to_string_lossy().into_owned(),
                target_app_data_dir: target.to_string_lossy().into_owned(),
            })
            .expect_err("reject invalid backup");
        assert_eq!(error.kind(), "restore_verification_failed");
        assert_eq!(error.to_string(), "restore verification failed");
        assert_eq!(
            std::fs::read(&input).expect("input unchanged"),
            input_before
        );
        assert!(!target.exists(), "invalid input must not activate a target");
    }

    let foreign_schema = temp.path().join("foreign-schema.sqlite3");
    let foreign = Connection::open(&foreign_schema).expect("create foreign schema database");
    foreign
        .execute_batch("CREATE TABLE foreign_data (id INTEGER PRIMARY KEY);")
        .expect("create foreign schema");
    drop(foreign);
    let foreign_before = std::fs::read(&foreign_schema).expect("read foreign schema");
    let foreign_target = temp.path().join("foreign-target");
    let error = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: foreign_schema.to_string_lossy().into_owned(),
            target_app_data_dir: foreign_target.to_string_lossy().into_owned(),
        })
        .expect_err("reject foreign schema");
    assert_eq!(error.kind(), "restore_verification_failed");
    assert_eq!(
        std::fs::read(&foreign_schema).expect("foreign input unchanged"),
        foreign_before
    );
    assert!(!foreign_target.exists());

    let existing_target = temp.path().join("existing-target");
    std::fs::create_dir(&existing_target).expect("reserve target directory");
    std::fs::write(existing_target.join("sentinel"), b"do not replace").expect("write sentinel");
    let valid_before = std::fs::read(&backup_path).expect("read valid backup");
    let error = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup_path.to_string_lossy().into_owned(),
            target_app_data_dir: existing_target.to_string_lossy().into_owned(),
        })
        .expect_err("reject existing target");
    assert_eq!(error.kind(), "restore_target_exists");
    assert_eq!(error.to_string(), "restore target already exists");
    assert_eq!(
        std::fs::read(existing_target.join("sentinel")).expect("sentinel remains"),
        b"do not replace"
    );
    assert_eq!(
        std::fs::read(&backup_path).expect("valid backup unchanged"),
        valid_before
    );
    assert_eq!(canonical_snapshot(&source_path), source_before);
    assert!(
        std::fs::read_dir(temp.path())
            .expect("list temporary directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains(".restore-staging")),
        "failed restores clean only their owned staging directories"
    );
}

#[test]
fn restore_verification_rejects_a_v4_named_lookalike_schema_before_target_reservation() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let source_path = temp.path().join("source.sqlite3");
    let foreign_path = temp.path().join("lookalike-v4.sqlite3");
    let target = temp.path().join("lookalike-target");
    let service = ApplicationService::open(&source_path).expect("open source service");
    let foreign = Connection::open(&foreign_path).expect("create lookalike database");
    foreign
        .execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES
                 (1, '2026-08-16T00:00:00.000Z'),
                 (2, '2026-08-16T00:00:00.000Z'),
                 (3, '2026-08-16T00:00:00.000Z'),
                 (4, '2026-08-16T00:00:00.000Z');
             CREATE TABLE jobs (id TEXT PRIMARY KEY);
             CREATE TABLE tasks (id TEXT PRIMARY KEY);
             CREATE TABLE command_log (command_id TEXT PRIMARY KEY);
             CREATE TABLE task_dependencies (predecessor_task_id TEXT PRIMARY KEY);",
        )
        .expect("create all required lookalike names");
    drop(foreign);
    let before = std::fs::read(&foreign_path).expect("read lookalike input");

    let error = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: foreign_path.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect_err("reject incompatible named schema");
    assert_eq!(error.kind(), "restore_verification_failed");
    assert_eq!(error.to_string(), "restore verification failed");
    assert_eq!(
        std::fs::read(&foreign_path).expect("input unchanged"),
        before
    );
    assert!(!target.exists(), "lookalike input never reserves a target");
}

#[test]
fn restore_preflight_rejects_invalid_v5_constraint_domain_without_mutation() {
    let temp = tempfile::tempdir().expect("temporary app data");
    for (name, value, summary_constraint) in [
        ("malformed", "not-a-date", false),
        ("noncanonical", "2026-8-1", false),
        ("summary", "2026-08-20", true),
    ] {
        let source_path = temp.path().join(format!("{name}.sqlite3"));
        let target = temp.path().join(format!("{name}-target"));
        let service = ApplicationService::open(&source_path).expect("open source");
        let job = service
            .create_job(
                command_context(),
                CreateJobRequest {
                    name: "Constraint preflight".into(),
                    timezone: "UTC".into(),
                },
            )
            .expect("job");
        let parent = service
            .create_task(
                command_context(),
                CreateTaskRequest {
                    job_id: job.id.clone(),
                    parent_task_id: None,
                    name: "Parent".into(),
                    expected_job_version: job.version,
                },
            )
            .expect("parent")
            .task;
        if summary_constraint {
            service
                .create_task(
                    command_context(),
                    CreateTaskRequest {
                        job_id: job.id.clone(),
                        parent_task_id: Some(parent.id.clone()),
                        name: "Child".into(),
                        expected_job_version: job.version + 1,
                    },
                )
                .expect("child");
            Connection::open(&source_path)
                .expect("open source direct")
                .execute(
                    "UPDATE tasks SET start_no_earlier_than = ?1 WHERE id = ?2",
                    rusqlite::params![value, parent.id],
                )
                .expect("write invalid summary constraint");
        } else {
            let updated = service
                .update_task_duration(
                    command_context(),
                    UpdateTaskDurationRequest {
                        task_id: parent.id.clone(),
                        duration_minutes: Some(480),
                        expected_version: parent.version,
                        expected_job_version: job.version + 1,
                    },
                )
                .expect("duration");
            assert_eq!(updated.task.duration_minutes, Some(480));
            Connection::open(&source_path)
                .expect("open source direct")
                .execute(
                    "UPDATE tasks SET start_no_earlier_than = ?1 WHERE id = ?2",
                    rusqlite::params![value, parent.id],
                )
                .expect("write malformed constraint");
        }
        let before = canonical_snapshot(&source_path);
        let error = service
            .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
                backup_path: source_path.to_string_lossy().into_owned(),
                target_app_data_dir: target.to_string_lossy().into_owned(),
            })
            .expect_err("reject invalid v5 input");
        assert_eq!(error.kind(), "restore_verification_failed");
        assert_eq!(canonical_snapshot(&source_path), before);
        assert!(
            !target.exists(),
            "invalid preflight never reserves a target"
        );
    }
}

#[test]
fn restore_verification_rejects_widened_v4_command_constraints_before_target_reservation() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let source_path = temp.path().join("source.sqlite3");
    let foreign_path = temp.path().join("widened-v4.sqlite3");
    let target = temp.path().join("widened-target");
    let service = ApplicationService::open(&source_path).expect("open source service");
    let foreign = Connection::open(&foreign_path).expect("create widened schema database");
    foreign
        .execute_batch(
            "CREATE TABLE schema_migrations (
                 version INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL
             );
             INSERT INTO schema_migrations VALUES
                 (1, '2026-08-16T00:00:00.000Z'),
                 (2, '2026-08-16T00:00:00.000Z'),
                 (3, '2026-08-16T00:00:00.000Z'),
                 (4, '2026-08-16T00:00:00.000Z');
             CREATE TABLE jobs (
                 id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL,
                 timezone TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                 version INTEGER NOT NULL CHECK (version > 0), schedule_start TEXT,
                 calendar_json TEXT NOT NULL DEFAULT '[]'
             );
             CREATE TABLE tasks (
                 id TEXT PRIMARY KEY, job_id TEXT NOT NULL, parent_task_id TEXT,
                 sort_key INTEGER NOT NULL CHECK (sort_key >= 0), name TEXT NOT NULL,
                 created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                 version INTEGER NOT NULL CHECK (version > 0),
                 duration_minutes INTEGER CHECK (duration_minutes >= 0),
                 UNIQUE (job_id, id),
                 FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT,
                 FOREIGN KEY (job_id, parent_task_id)
                     REFERENCES tasks(job_id, id) ON DELETE RESTRICT
             );
             CREATE INDEX tasks_job_parent_order ON tasks(job_id, parent_task_id, sort_key, id);
             CREATE UNIQUE INDEX tasks_root_sibling_order
                 ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
             CREATE UNIQUE INDEX tasks_child_sibling_order
                 ON tasks(job_id, parent_task_id, sort_key) WHERE parent_task_id IS NOT NULL;
             CREATE TABLE command_log (
                 command_id TEXT NOT NULL PRIMARY KEY
                     CHECK (length(command_id) BETWEEN 1 AND 1280),
                 actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import', 'evil')),
                 client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120),
                 created_at TEXT NOT NULL,
                 summary TEXT NOT NULL CHECK (length(summary) <= 240)
             );
             CREATE TABLE task_dependencies (
                 job_id TEXT NOT NULL, predecessor_task_id TEXT NOT NULL,
                 successor_task_id TEXT NOT NULL,
                 lag_minutes INTEGER NOT NULL CHECK (lag_minutes >= 0),
                 PRIMARY KEY (predecessor_task_id, successor_task_id),
                 FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT,
                 FOREIGN KEY (job_id, predecessor_task_id)
                     REFERENCES tasks(job_id, id) ON DELETE RESTRICT,
                 FOREIGN KEY (job_id, successor_task_id)
                     REFERENCES tasks(job_id, id) ON DELETE RESTRICT,
                 CHECK (predecessor_task_id <> successor_task_id)
             );
             CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);",
        )
        .expect("create widened v4 lookalike");
    drop(foreign);
    let before = std::fs::read(&foreign_path).expect("read widened input");

    let error = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: foreign_path.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect_err("reject widened command constraints");
    assert_eq!(error.kind(), "restore_verification_failed");
    assert_eq!(error.to_string(), "restore verification failed");
    assert_eq!(
        std::fs::read(&foreign_path).expect("input unchanged"),
        before
    );
    assert!(!target.exists(), "widened input never reserves a target");
}

#[test]
fn restore_verification_has_one_no_clobber_winner_under_target_contention() {
    let temp = tempfile::tempdir().expect("temporary app data");
    let source_path = temp.path().join("source.sqlite3");
    let backup_path = temp.path().join("valid.backup.sqlite3");
    let target = temp.path().join("contended-target");
    let service = Arc::new(ApplicationService::open(&source_path).expect("open source service"));
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Source job".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create source job");
    service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("create valid backup");

    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let service = Arc::clone(&service);
            let barrier = Arc::clone(&barrier);
            let backup_path = backup_path.clone();
            let target = target.clone();
            std::thread::spawn(move || {
                barrier.wait();
                service.verify_restore_into_fresh_app_data(VerifyRestoreRequest {
                    backup_path: backup_path.to_string_lossy().into_owned(),
                    target_app_data_dir: target.to_string_lossy().into_owned(),
                })
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("restore thread completes"))
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .map(ApplicationError::kind)
            .collect::<Vec<_>>(),
        vec!["restore_target_exists"]
    );
    assert!(target.join("contractorproject.sqlite3").is_file());
}

#[cfg(unix)]
#[test]
fn restore_verification_rejects_a_dangling_target_symlink_without_touching_it() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary app data");
    let source_path = temp.path().join("source.sqlite3");
    let backup_path = temp.path().join("valid.backup.sqlite3");
    let target = temp.path().join("dangling-target");
    let service = ApplicationService::open(&source_path).expect("open source service");
    service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Source job".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create source job");
    service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("create valid backup");
    let backup_before = std::fs::read(&backup_path).expect("read valid backup");
    symlink(temp.path().join("missing-target"), &target).expect("create dangling symlink");

    let error = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup_path.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect_err("reject dangling target symlink");
    assert_eq!(error.kind(), "restore_target_exists");
    assert!(std::fs::symlink_metadata(&target)
        .expect("symlink remains")
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(&backup_path).expect("backup unchanged"),
        backup_before
    );
}

#[test]
fn typed_dependencies_persist_and_remove_by_type_across_restart() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Typed links".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let first = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "First".into(),
                expected_job_version: 1,
            },
        )
        .expect("first");
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Second".into(),
                expected_job_version: first.job_version,
            },
        )
        .expect("second");
    let first_task = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: first.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: first.task.version,
                expected_job_version: second.job_version,
            },
        )
        .expect("first duration");
    let second_task = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: second.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: second.task.version,
                expected_job_version: first_task.job_version,
            },
        )
        .expect("second duration");
    let mut job_version = second_task.job_version;

    // A typed SS link with negative lag persists exactly as entered.
    let linked = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                lag_minutes: -60,
                dependency_type: Some("SS".into()),
                expected_job_version: job_version,
            },
        )
        .expect("add SS link");
    job_version = linked.job_version;
    assert_eq!(linked.dependencies.len(), 1);
    assert_eq!(
        linked.dependencies[0].dependency_type,
        DependencyType::StartStart
    );
    assert_eq!(linked.dependencies[0].lag_minutes, -60);

    // The same pair with a different type is legal.
    let dual = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                lag_minutes: 0,
                dependency_type: Some("FS".into()),
                expected_job_version: job_version,
            },
        )
        .expect("add FS link on the same pair");
    job_version = dual.job_version;
    assert_eq!(dual.dependencies.len(), 2);

    // An exact repeat of an existing typed link is rejected atomically.
    let before = service.list_tasks(&job.id).expect("before duplicate");
    let audit_before = command_log_count(&path);
    let duplicate = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                lag_minutes: 999,
                dependency_type: Some("SS".into()),
                expected_job_version: job_version,
            },
        )
        .expect_err("duplicate typed link");
    assert_eq!(duplicate.kind(), "validation_failed");
    // An unknown type code is rejected before any mutation.
    let bad_type = service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                lag_minutes: 0,
                dependency_type: Some("ZZ".into()),
                expected_job_version: job_version,
            },
        )
        .expect_err("unknown type code");
    assert_eq!(bad_type.kind(), "invalid_input");
    assert_eq!(service.list_tasks(&job.id).expect("unchanged"), before);
    assert_eq!(command_log_count(&path), audit_before);

    // Reopen: both typed links survive the restart.
    drop(service);
    let reopened = ApplicationService::open(&path).expect("reopen");
    let hierarchy = reopened.list_tasks(&job.id).expect("links");
    assert_eq!(hierarchy.dependencies.len(), 2);
    let types: Vec<DependencyType> = hierarchy
        .dependencies
        .iter()
        .map(|dependency| dependency.dependency_type)
        .collect();
    assert!(types.contains(&DependencyType::StartStart));
    assert!(types.contains(&DependencyType::FinishStart));

    // Removing identifies the row by type, leaving the other link intact.
    let removed = reopened
        .remove_dependency(
            command_context(),
            RemoveDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                dependency_type: Some("SS".into()),
                expected_job_version: job_version,
            },
        )
        .expect("remove SS link");
    assert_eq!(removed.dependencies.len(), 1);
    assert_eq!(
        removed.dependencies[0].dependency_type,
        DependencyType::FinishStart
    );
}

#[test]
fn migration_v8_rebuilds_dependencies_on_fresh_and_existing_v7_database() {
    // A fresh database opens at v8 with the typed dependency column.
    let fresh = tempfile::tempdir().expect("temp");
    let fresh_path = fresh.path().join("contractorproject.sqlite3");
    let _service = ApplicationService::open(&fresh_path).expect("open fresh");
    let connection = Connection::open(&fresh_path).expect("inspect fresh");
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    assert_eq!(version, 8);
    let typed_column: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('task_dependencies') WHERE name = 'dependency_type'",
            [],
            |row| row.get(0),
        )
        .expect("dependency_type column");
    assert_eq!(typed_column, 1);

    // An existing exact-v7 database migrates its FS rows forward.
    let existing = tempfile::tempdir().expect("temp");
    let existing_path = existing.path().join("contractorproject.sqlite3");
    write_exact_v7_database_with_dependency(&existing_path);
    let service = ApplicationService::open(&existing_path).expect("migrate v7");
    let migrated_version: i64 = Connection::open(&existing_path)
        .expect("open migrated")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migrated version");
    assert_eq!(migrated_version, 8);
    let hierarchy = service.list_tasks("job-v7").expect("preserved links");
    assert_eq!(hierarchy.dependencies.len(), 1);
    assert_eq!(
        hierarchy.dependencies[0].dependency_type,
        DependencyType::FinishStart
    );
    // The pre-migration-v8 backup retains the original v7 snapshot.
    let backup_path =
        existing_path.with_file_name("contractorproject.sqlite3.pre-migration-v8.bak");
    let backup_version: i64 =
        Connection::open_with_flags(&backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open pre-migration backup")
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("backup version");
    assert_eq!(backup_version, 7);
}

#[test]
fn verified_backup_and_clean_restore_accept_v8() {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("contractorproject.sqlite3");
    let service = ApplicationService::open(&path).expect("open");
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: "Backup".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("job");
    let first = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "First".into(),
                expected_job_version: 1,
            },
        )
        .expect("first");
    let second = service
        .create_task(
            command_context(),
            CreateTaskRequest {
                job_id: job.id.clone(),
                parent_task_id: None,
                name: "Second".into(),
                expected_job_version: first.job_version,
            },
        )
        .expect("second");
    let first_task = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: first.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: first.task.version,
                expected_job_version: second.job_version,
            },
        )
        .expect("first duration");
    let second_task = service
        .update_task_duration(
            command_context(),
            UpdateTaskDurationRequest {
                task_id: second.task.id.clone(),
                duration_minutes: Some(480),
                expected_version: second.task.version,
                expected_job_version: first_task.job_version,
            },
        )
        .expect("second duration");
    service
        .add_dependency(
            command_context(),
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: first.task.id.clone(),
                successor_task_id: second.task.id.clone(),
                lag_minutes: -30,
                dependency_type: Some("FF".into()),
                expected_job_version: second_task.job_version,
            },
        )
        .expect("typed link");

    let backup_dir = tempfile::tempdir().expect("temp");
    let backup_path = backup_dir.path().join("v8.backup.sqlite3");
    let backup = service
        .create_verified_backup(CreateBackupRequest {
            destination: backup_path.to_string_lossy().into_owned(),
        })
        .expect("verify v8 backup");
    assert!(backup.verified);
    let target = backup_dir.path().join("restored-v8");
    let result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup_path.to_string_lossy().into_owned(),
            target_app_data_dir: target.to_string_lossy().into_owned(),
        })
        .expect("restore v8");
    assert!(result.verified);
    assert_eq!(result.dependency_count, 1);
    let restored_version: i64 = Connection::open(target.join("contractorproject.sqlite3"))
        .expect("open restored")
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("restored version");
    assert_eq!(restored_version, 8);
}

// Writes an exact-v7 database with one FS dependency for migration testing.
fn write_exact_v7_database_with_dependency(path: &std::path::Path) {
    let connection = Connection::open(path).expect("create v7");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES
                (1, '2026-08-16T00:00:00.000Z'), (2, '2026-08-16T00:00:00.000Z'),
                (3, '2026-08-16T00:00:00.000Z'), (4, '2026-08-16T00:00:00.000Z'),
                (5, '2026-08-16T00:00:00.000Z'), (6, '2026-08-16T00:00:00.000Z'),
                (7, '2026-08-16T00:00:00.000Z');
             CREATE TABLE jobs (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL, timezone TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), schedule_start TEXT, calendar_json TEXT NOT NULL DEFAULT '[]', data_date TEXT);
             CREATE TABLE tasks (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, parent_task_id TEXT, sort_key INTEGER NOT NULL CHECK (sort_key >= 0), name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL CHECK (version > 0), duration_minutes INTEGER CHECK (duration_minutes >= 0), start_no_earlier_than TEXT, finish_no_later_than TEXT, percent_complete INTEGER, actual_start TEXT, actual_finish TEXT, UNIQUE (job_id, id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, parent_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT);
             CREATE INDEX tasks_job_parent_order ON tasks(job_id, parent_task_id, sort_key, id);
             CREATE UNIQUE INDEX tasks_root_sibling_order ON tasks(job_id, sort_key) WHERE parent_task_id IS NULL;
             CREATE UNIQUE INDEX tasks_child_sibling_order ON tasks(job_id, parent_task_id, sort_key) WHERE parent_task_id IS NOT NULL;
             CREATE TABLE command_log (command_id TEXT NOT NULL PRIMARY KEY CHECK (length(command_id) BETWEEN 1 AND 128), actor TEXT NOT NULL CHECK (actor IN ('user', 'agent', 'import')), client_name TEXT NOT NULL CHECK (length(client_name) BETWEEN 1 AND 120), created_at TEXT NOT NULL, summary TEXT NOT NULL CHECK (length(summary) <= 240));
             CREATE TABLE task_dependencies (job_id TEXT NOT NULL, predecessor_task_id TEXT NOT NULL, successor_task_id TEXT NOT NULL, lag_minutes INTEGER NOT NULL CHECK (lag_minutes >= 0), PRIMARY KEY (predecessor_task_id, successor_task_id), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT, FOREIGN KEY (job_id, predecessor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, FOREIGN KEY (job_id, successor_task_id) REFERENCES tasks(job_id, id) ON DELETE RESTRICT, CHECK (predecessor_task_id <> successor_task_id));
             CREATE INDEX task_dependencies_job_successor ON task_dependencies(job_id, successor_task_id);
             CREATE TABLE baselines (id TEXT PRIMARY KEY, job_id TEXT NOT NULL, name TEXT NOT NULL, created_at TEXT NOT NULL, is_comparison_default INTEGER NOT NULL DEFAULT 0 CHECK (is_comparison_default IN (0, 1)), UNIQUE (job_id, name), FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT);
             CREATE TABLE baseline_tasks (baseline_id TEXT NOT NULL, task_id TEXT NOT NULL, start TEXT NOT NULL, finish TEXT NOT NULL, duration_minutes INTEGER NOT NULL CHECK (duration_minutes >= 0), PRIMARY KEY (baseline_id, task_id), FOREIGN KEY (baseline_id) REFERENCES baselines(id) ON DELETE RESTRICT);
             CREATE UNIQUE INDEX baselines_one_default_per_job ON baselines(job_id) WHERE is_comparison_default = 1;
             INSERT INTO jobs VALUES ('job-v7', 'Existing v7', 'draft', 'UTC', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 5, '2026-08-17', '{\"workingWeekdays\":[\"monday\",\"tuesday\",\"wednesday\",\"thursday\",\"friday\"],\"workdayStartMinute\":480,\"workdayDurationMinutes\":480}', NULL);
             INSERT INTO tasks VALUES ('first', 'job-v7', NULL, 0, 'First', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, 480, NULL, NULL, NULL, NULL, NULL), ('second', 'job-v7', NULL, 1, 'Second', '2026-08-16T00:00:00.000Z', '2026-08-16T00:00:00.000Z', 1, 480, NULL, NULL, NULL, NULL, NULL);
             INSERT INTO task_dependencies VALUES ('job-v7', 'first', 'second', 0);",
        )
        .expect("write exact v7");
}
