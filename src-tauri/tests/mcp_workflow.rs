//! End-to-end tests for the contractorproject-mcp helper: real JSON-RPC lines
//! through `Server::handle` against a temporary database the app created.

use contractorproject_lib::application::ApplicationService;
use contractorproject_lib::mcp::{Mode, Server};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// A database created the way the app creates it, then closed.
fn app_database(directory: &Path) -> PathBuf {
    let path = directory.join("contractorproject.sqlite3");
    drop(ApplicationService::open(&path).expect("create database"));
    path
}

fn server(path: &Path, mode: Mode) -> Server {
    Server::open(path, mode, "test-agent", &mut Vec::new()).expect("open helper")
}

/// Call a tool and return either its structured result or the error object.
fn call(server: &mut Server, name: &str, arguments: Value) -> Result<Value, Value> {
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    });
    let response = server.handle(&request.to_string()).expect("a response");
    match response.get("result") {
        Some(result) => Ok(result["structuredContent"].clone()),
        None => Err(response["error"].clone()),
    }
}

fn tool_names(server: &mut Server) -> Vec<String> {
    let response = server
        .handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .expect("a response");
    response["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn initialize_reports_versions_and_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut helper = server(&app_database(dir.path()), Mode::ReadOnly);
    let response = helper
        .handle(r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}"#)
        .unwrap();
    assert_eq!(
        response["result"]["serverInfo"]["name"],
        json!("contractorproject-mcp")
    );
    assert_eq!(response["result"]["serverInfo"]["apiVersion"], json!("1.0"));
    assert!(response["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("read-only"));
    // A notification gets no answer.
    assert!(helper
        .handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .is_none());
}

#[test]
fn read_only_hides_and_refuses_write_tools() {
    let dir = tempfile::tempdir().unwrap();
    let mut helper = server(&app_database(dir.path()), Mode::ReadOnly);
    let names = tool_names(&mut helper);
    assert!(names.contains(&"list_jobs".to_string()));
    assert!(!names.contains(&"create_job".to_string()));
    let error = call(
        &mut helper,
        "create_job",
        json!({ "commandId": "c1", "name": "Fence", "timezone": "America/New_York" }),
    )
    .unwrap_err();
    assert_eq!(error["data"]["kind"], json!("read_only"));
}

#[test]
fn an_agent_builds_and_schedules_a_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut helper = server(&app_database(dir.path()), Mode::ReadWrite);

    let job = call(
        &mut helper,
        "create_job",
        json!({ "commandId": "job-1", "name": "Backyard fence", "timezone": "America/New_York" }),
    )
    .unwrap();
    let job_id = job["id"].as_str().unwrap().to_string();
    assert_eq!(job["version"], json!(1));

    let posts = call(
        &mut helper,
        "create_task",
        json!({ "commandId": "task-1", "jobId": job_id, "name": "Set posts", "expectedJobVersion": 1 }),
    )
    .unwrap();
    let job_version = posts["jobVersion"].as_i64().unwrap();
    let posts_id = posts["task"]["id"].as_str().unwrap().to_string();

    let panels = call(
        &mut helper,
        "create_task",
        json!({ "commandId": "task-2", "jobId": job_id, "name": "Hang panels", "expectedJobVersion": job_version }),
    )
    .unwrap();
    let mut job_version = panels["jobVersion"].as_i64().unwrap();
    let panels_id = panels["task"]["id"].as_str().unwrap().to_string();

    let scheduled = call(
        &mut helper,
        "update_schedule",
        json!({ "commandId": "sched-1", "jobId": job_id, "scheduleStart": "2026-10-12",
                "calendar": { "workingWeekdays": ["monday","tuesday","wednesday","thursday","friday"],
                              "workdayStartMinute": 420, "workdayDurationMinutes": 480 },
                "expectedJobVersion": job_version }),
    )
    .unwrap();
    job_version = scheduled["version"].as_i64().unwrap();

    for (index, (task_id, command)) in [(&posts_id, "dur-1"), (&panels_id, "dur-2")]
        .iter()
        .enumerate()
    {
        let tasks = call(&mut helper, "list_tasks", json!({ "jobId": job_id })).unwrap();
        let version = tasks["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["id"] == json!(task_id))
            .unwrap()["version"]
            .as_i64()
            .unwrap();
        let updated = call(
            &mut helper,
            "update_task_duration",
            json!({ "commandId": command, "taskId": task_id, "durationMinutes": 480 * (index as i64 + 1),
                    "expectedVersion": version, "expectedJobVersion": job_version }),
        )
        .unwrap();
        job_version = updated["jobVersion"].as_i64().unwrap();
    }

    let linked = call(
        &mut helper,
        "add_dependency",
        json!({ "commandId": "dep-1", "jobId": job_id, "predecessorTaskId": posts_id,
                "successorTaskId": panels_id, "lagMinutes": 0, "expectedJobVersion": job_version }),
    )
    .unwrap();
    job_version = linked["jobVersion"].as_i64().unwrap();
    assert_eq!(linked["dependencies"].as_array().unwrap().len(), 1);

    // The schedule is the same read model the desktop app renders.
    let schedule = call(&mut helper, "get_schedule", json!({ "jobId": job_id })).unwrap();
    assert!(schedule.is_object(), "{schedule}");

    let fetched = call(
        &mut helper,
        "get_job",
        json!({ "jobId": job_id, "includeTasks": true }),
    )
    .unwrap();
    assert_eq!(fetched["job"]["version"], json!(job_version));
    assert_eq!(fetched["tasks"]["tasks"].as_array().unwrap().len(), 2);

    let listed = call(&mut helper, "list_jobs", json!({})).unwrap();
    assert_eq!(listed["totalCount"], json!(1));
}

#[test]
fn stale_versions_and_repeated_commands_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut helper = server(&app_database(dir.path()), Mode::ReadWrite);
    let job = call(
        &mut helper,
        "create_job",
        json!({ "commandId": "job-1", "name": "Gate repair", "timezone": "America/New_York" }),
    )
    .unwrap();
    let job_id = job["id"].as_str().unwrap().to_string();
    call(
        &mut helper,
        "create_task",
        json!({ "commandId": "task-1", "jobId": job_id, "name": "Rehang gate", "expectedJobVersion": 1 }),
    )
    .unwrap();

    // The job moved to version 2; writing against version 1 is a conflict.
    let conflict = call(
        &mut helper,
        "create_task",
        json!({ "commandId": "task-2", "jobId": job_id, "name": "Paint", "expectedJobVersion": 1 }),
    )
    .unwrap_err();
    assert_eq!(conflict["data"]["kind"], json!("version_conflict"));
    assert_eq!(conflict["data"]["currentVersion"], json!(2));

    // Reusing an applied command ID changes nothing.
    let duplicate = call(
        &mut helper,
        "create_job",
        json!({ "commandId": "job-1", "name": "Gate repair", "timezone": "America/New_York" }),
    )
    .unwrap_err();
    assert_eq!(duplicate["data"]["kind"], json!("duplicate_command"));
    let listed = call(&mut helper, "list_jobs", json!({})).unwrap();
    assert_eq!(listed["totalCount"], json!(1));

    // A write without a command ID is the caller's mistake.
    let missing = call(
        &mut helper,
        "create_job",
        json!({ "name": "No id", "timezone": "America/New_York" }),
    )
    .unwrap_err();
    assert_eq!(missing["data"]["kind"], json!("invalid_input"));
    assert_eq!(missing["data"]["field"], json!("commandId"));
}

#[test]
fn the_helper_never_creates_or_adopts_a_foreign_database() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.sqlite3");
    assert!(Server::open(&missing, Mode::ReadWrite, "t", &mut Vec::new()).is_err());
    assert!(!missing.exists(), "the helper must not create a database");

    let foreign = dir.path().join("foreign.sqlite3");
    rusqlite::Connection::open(&foreign)
        .unwrap()
        .execute_batch("CREATE TABLE contacts (id TEXT);")
        .unwrap();
    let before = std::fs::read(&foreign).unwrap();
    assert!(Server::open(&foreign, Mode::ReadWrite, "t", &mut Vec::new()).is_err());
    assert_eq!(
        std::fs::read(&foreign).unwrap(),
        before,
        "a refused file is untouched"
    );
}

#[test]
fn unknown_tools_and_methods_get_clear_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut helper = server(&app_database(dir.path()), Mode::ReadWrite);
    let error = call(&mut helper, "drop_tables", json!({})).unwrap_err();
    assert_eq!(error["data"]["kind"], json!("not_found"));
    let response = helper
        .handle(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#)
        .unwrap();
    assert_eq!(response["error"]["code"], json!(-32601));
    let response = helper.handle("not json").unwrap();
    assert_eq!(response["error"]["code"], json!(-32700));
}

/// The field scenario: "concrete got delayed from this Wednesday to next
/// Monday — update the schedule." A start-no-earlier-than constraint on the
/// concrete task moves it, and the dependent framing task follows.
#[test]
fn an_agent_pushes_a_delayed_task_and_its_successor_moves() {
    let dir = tempfile::tempdir().unwrap();
    let mut helper = server(&app_database(dir.path()), Mode::ReadWrite);
    let job = call(
        &mut helper,
        "create_job",
        json!({ "commandId": "j", "name": "Garage slab", "timezone": "America/New_York" }),
    )
    .unwrap();
    let job_id = job["id"].as_str().unwrap().to_string();

    // Wednesday 2026-10-14 start, Monday–Friday, 7:00 for 8 hours.
    let mut job_version = call(
        &mut helper,
        "update_schedule",
        json!({ "commandId": "s", "jobId": job_id, "scheduleStart": "2026-10-14",
                "calendar": { "workingWeekdays": ["monday","tuesday","wednesday","thursday","friday"],
                              "workdayStartMinute": 420, "workdayDurationMinutes": 480 },
                "expectedJobVersion": 1 }),
    )
    .unwrap()["version"]
        .as_i64()
        .unwrap();

    let mut ids = Vec::new();
    for (name, days) in [("Pour concrete", 1), ("Frame walls", 2)] {
        let created = call(
            &mut helper,
            "create_task",
            json!({ "commandId": format!("t-{name}"), "jobId": job_id, "name": name,
                    "expectedJobVersion": job_version }),
        )
        .unwrap();
        let task_id = created["task"]["id"].as_str().unwrap().to_string();
        let task_version = created["task"]["version"].as_i64().unwrap();
        job_version = created["jobVersion"].as_i64().unwrap();
        let updated = call(
            &mut helper,
            "update_task_duration",
            json!({ "commandId": format!("d-{name}"), "taskId": task_id,
                    "durationMinutes": 480 * days, "expectedVersion": task_version,
                    "expectedJobVersion": job_version }),
        )
        .unwrap();
        job_version = updated["jobVersion"].as_i64().unwrap();
        ids.push((task_id, updated["task"]["version"].as_i64().unwrap()));
    }
    let (concrete, concrete_version) = ids[0].clone();
    let framing = ids[1].0.clone();
    job_version = call(
        &mut helper,
        "add_dependency",
        json!({ "commandId": "link", "jobId": job_id, "predecessorTaskId": concrete,
                "successorTaskId": framing, "lagMinutes": 0, "expectedJobVersion": job_version }),
    )
    .unwrap()["jobVersion"]
        .as_i64()
        .unwrap();

    let start_of = |schedule: &Value, task: &str| -> String {
        schedule["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["taskId"] == json!(task))
            .unwrap()["start"]
            .as_str()
            .unwrap()[..10]
            .to_string()
    };
    let before = call(&mut helper, "get_schedule", json!({ "jobId": job_id })).unwrap();
    assert_eq!(start_of(&before, &concrete), "2026-10-14");
    assert_eq!(start_of(&before, &framing), "2026-10-15");

    // The delay: concrete cannot start before next Monday.
    call(
        &mut helper,
        "update_task_constraint",
        json!({ "commandId": "delay", "taskId": concrete, "kind": "start_no_earlier_than",
                "value": "2026-10-19", "expectedVersion": concrete_version,
                "expectedJobVersion": job_version }),
    )
    .unwrap();

    let after = call(&mut helper, "get_schedule", json!({ "jobId": job_id })).unwrap();
    assert_eq!(start_of(&after, &concrete), "2026-10-19");
    assert_eq!(start_of(&after, &framing), "2026-10-20");
}
