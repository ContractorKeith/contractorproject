use contractorproject_lib::application::{
    ApplicationService, CommandActor, CommandContext, CreateBackupRequest, CreateJobRequest,
    CreateTaskRequest,
};
use std::process::Command;

fn context(id: &str) -> CommandContext {
    CommandContext {
        command_id: id.to_owned(),
        actor: CommandActor::Agent,
        client_name: "recovery-cli-test".into(),
    }
}

fn backup_fixture(directory: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let active_database = directory.join("active.sqlite3");
    let backup = directory.join("valid.backup.sqlite3");
    let service = ApplicationService::open(&active_database).expect("open active database");
    let job = service
        .create_job(
            context("cli-job"),
            CreateJobRequest {
                name: "CLI recovery sample fence".into(),
                timezone: "UTC".into(),
            },
        )
        .expect("create sample job");
    service
        .create_task(
            context("cli-task"),
            CreateTaskRequest {
                job_id: job.id,
                parent_task_id: None,
                name: "Post setting".into(),
                expected_job_version: 1,
            },
        )
        .expect("create sample task");
    service
        .create_verified_backup(CreateBackupRequest {
            destination: backup.to_string_lossy().into_owned(),
        })
        .expect("create sample backup");
    (active_database, backup)
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_contractorproject-recovery"))
}

#[test]
fn operator_binary_restores_and_reports_bounded_result_without_touching_source() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (active_database, backup) = backup_fixture(temp.path());
    let target = temp.path().join("recovered-app-data");
    let output = command()
        .arg("--backup")
        .arg(&backup)
        .arg("--target-app-data-dir")
        .arg(&target)
        .output()
        .expect("run recovery operator command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON result");
    assert_eq!(result["verified"], true);
    assert_eq!(result["jobCount"], 1);
    assert_eq!(result["taskCount"], 1);
    assert_eq!(result["dependencyCount"], 0);
    let source = ApplicationService::open(&active_database).expect("reopen active database");
    assert_eq!(source.list_jobs().expect("active jobs").len(), 1);
    assert!(target.join("contractorproject.sqlite3").is_file());
}

#[test]
fn operator_binary_rejects_corrupt_backup_without_creating_target() {
    let temp = tempfile::tempdir().expect("tempdir");
    let corrupt_backup = temp.path().join("corrupt.sqlite3");
    std::fs::write(&corrupt_backup, b"not a SQLite backup").expect("write corrupt input");
    let target = temp.path().join("must-not-exist");
    let output = command()
        .arg("--backup")
        .arg(&corrupt_backup)
        .arg("--target-app-data-dir")
        .arg(&target)
        .output()
        .expect("run recovery operator command");
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("restore verification failed"), "{error}");
    assert!(!target.exists());
}

#[test]
fn operator_binary_rejects_occupied_target_without_modifying_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (_active_database, backup) = backup_fixture(temp.path());
    let target = temp.path().join("occupied");
    std::fs::create_dir(&target).expect("create occupied target");
    let sentinel = target.join("keep.txt");
    std::fs::write(&sentinel, b"preserve this file").expect("write sentinel");
    let output = command()
        .arg("--backup")
        .arg(&backup)
        .arg("--target-app-data-dir")
        .arg(&target)
        .output()
        .expect("run recovery operator command");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("restore target already exists"));
    assert_eq!(
        std::fs::read(&sentinel).expect("read sentinel"),
        b"preserve this file"
    );
    assert_eq!(std::fs::read_dir(&target).expect("list target").count(), 1);
}
