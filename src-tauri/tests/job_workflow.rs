use contractorproject_lib::application::{
    AddDependencyRequest, RemoveDependencyRequest, UpdateScheduleRequest, UpdateTaskDurationRequest,
};
use contractorproject_lib::application::{
    ApplicationService, CommandActor, CommandContext, CreateJobRequest, CreateTaskRequest,
};
use contractorproject_lib::application::{ReorderTaskRequest, UpdateTaskRequest};
use contractorproject_lib::scheduling::{CalendarWeekday, WorkingCalendar};
use rusqlite::Connection;
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

fn command_log_count(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .expect("open audit database")
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count command log")
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
                expected_job_version: schedule.version,
            },
        )
        .expect("remove");
    assert!(removed.dependencies.is_empty());
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
                expected_job_version: job_version,
            },
            "validation_failed",
        ),
        (
            AddDependencyRequest {
                job_id: job.id.clone(),
                predecessor_task_id: tasks[0].id.clone(),
                successor_task_id: tasks[1].id.clone(),
                lag_minutes: -1,
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
    assert_eq!(migrated_version, 4);
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
