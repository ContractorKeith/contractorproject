use contractorproject_lib::application::{ApplicationService, CreateJobRequest, CreateTaskRequest};
use contractorproject_lib::application::{ReorderTaskRequest, UpdateTaskRequest};
use rusqlite::Connection;

#[test]
fn created_job_is_available_after_reopening_the_database() {
    let temp = tempfile::tempdir().expect("create temporary app data");
    let database_path = temp.path().join("contractorproject.sqlite3");

    let service = ApplicationService::open(&database_path).expect("open application service");
    let created = service
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");

    let root = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Site work".into(),
            expected_job_version: 1,
        })
        .expect("create root task")
        .task;
    let child = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: Some(root.id.clone()),
            name: "Layout".into(),
            expected_job_version: 2,
        })
        .expect("create child task")
        .task;
    let second_root = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Closeout".into(),
            expected_job_version: 3,
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");
    service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Site work".into(),
            expected_job_version: 1,
        })
        .expect("create first task");

    let error = service
        .create_task(CreateTaskRequest {
            job_id: job.id,
            parent_task_id: None,
            name: "Closeout".into(),
            expected_job_version: 1,
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");
    let created = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Site work".into(),
            expected_job_version: 1,
        })
        .expect("create task");

    let updated = service
        .update_task(UpdateTaskRequest {
            task_id: created.task.id.clone(),
            name: "  Site preparation  ".into(),
            expected_version: 1,
        })
        .expect("edit task");

    assert_eq!(updated.task.name, "Site preparation");
    assert_eq!(updated.task.version, 2);
    assert_eq!(updated.job_version, 3);

    let error = service
        .update_task(UpdateTaskRequest {
            task_id: created.task.id,
            name: "Stale overwrite".into(),
            expected_version: 1,
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");
    let root = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Site work".into(),
            expected_job_version: 1,
        })
        .expect("create root")
        .task;
    let first_child = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: Some(root.id.clone()),
            name: "Layout".into(),
            expected_job_version: 2,
        })
        .expect("create first child")
        .task;
    let second_child = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: Some(root.id.clone()),
            name: "Excavation".into(),
            expected_job_version: 3,
        })
        .expect("create second child")
        .task;
    let closeout = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Closeout".into(),
            expected_job_version: 4,
        })
        .expect("create closeout")
        .task;

    let reordered = service
        .reorder_task(ReorderTaskRequest {
            task_id: closeout.id.clone(),
            new_parent_task_id: Some(root.id.clone()),
            new_sibling_index: 1,
            expected_version: 1,
            expected_job_version: 5,
        })
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
        .create_job(CreateJobRequest {
            name: "First job".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create first job");
    let second_job = service
        .create_job(CreateJobRequest {
            name: "Second job".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create second job");
    let first_job_task = service
        .create_task(CreateTaskRequest {
            job_id: first_job.id,
            parent_task_id: None,
            name: "First job task".into(),
            expected_job_version: 1,
        })
        .expect("create first job task")
        .task;

    let error = service
        .create_task(CreateTaskRequest {
            job_id: second_job.id.clone(),
            parent_task_id: Some(first_job_task.id),
            name: "Invalid child".into(),
            expected_job_version: 1,
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");
    let parent = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Parent".into(),
            expected_job_version: 1,
        })
        .expect("create parent")
        .task;
    let child = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: Some(parent.id.clone()),
            name: "Child".into(),
            expected_job_version: 2,
        })
        .expect("create child")
        .task;
    let before = service.list_tasks(&job.id).expect("list initial hierarchy");

    let error = service
        .reorder_task(ReorderTaskRequest {
            task_id: parent.id,
            new_parent_task_id: Some(child.id),
            new_sibling_index: 0,
            expected_version: 1,
            expected_job_version: 3,
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");
    let first = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "First".into(),
            expected_job_version: 1,
        })
        .expect("create first task")
        .task;
    service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Second".into(),
            expected_job_version: 2,
        })
        .expect("create second task");
    let before = service.list_tasks(&job.id).expect("list initial hierarchy");

    let error = service
        .reorder_task(ReorderTaskRequest {
            task_id: first.id,
            new_parent_task_id: None,
            new_sibling_index: 2,
            expected_version: 1,
            expected_job_version: 3,
        })
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
        .create_job(CreateJobRequest {
            name: "Ridgeline Fence — Phase 2".into(),
            timezone: "America/New_York".into(),
        })
        .expect("create job");
    service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "First".into(),
            expected_job_version: 1,
        })
        .expect("create first task");
    let second = service
        .create_task(CreateTaskRequest {
            job_id: job.id.clone(),
            parent_task_id: None,
            name: "Second".into(),
            expected_job_version: 2,
        })
        .expect("create second task")
        .task;
    let before = service.list_tasks(&job.id).expect("list initial hierarchy");

    let error = service
        .reorder_task(ReorderTaskRequest {
            task_id: second.id,
            new_parent_task_id: None,
            new_sibling_index: 0,
            expected_version: 1,
            expected_job_version: 2,
        })
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
