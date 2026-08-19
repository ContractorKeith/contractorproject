//! Integration tests for the ContractorCRM hand-off envelope import: a valid
//! envelope creates a job, a newer schemaVersion is refused, malformed JSON
//! fails cleanly, and unknown fields are ignored (additive contract).

use contractorproject_lib::application::ApplicationService;
use contractorproject_lib::handoff_import::{import, Options};
use std::path::{Path, PathBuf};

fn write_envelope(directory: &Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, body).expect("write envelope");
    path
}

fn options(envelope: PathBuf, database: PathBuf) -> Options {
    Options {
        envelope,
        database,
        timezone: Some("America/New_York".to_owned()),
    }
}

const VALID_ENVELOPE: &str = r#"{
  "schemaVersion": 1,
  "kind": "opportunity_handoff",
  "exportedAt": "2026-08-19T18:00:00.000Z",
  "product": { "name": "ContractorCRM", "version": "0.1.0" },
  "opportunity": {
    "id": "opportunity-1",
    "name": "Backyard privacy fence",
    "stageName": "Won",
    "value": { "valueMinor": 250000, "currencyCode": "USD" }
  },
  "contact": null,
  "company": null
}"#;

#[test]
fn valid_envelope_creates_a_job_visible_through_list_jobs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let database = temp.path().join("contractorproject.sqlite3");

    let imported = import(&options(envelope, database.clone())).expect("import envelope");
    assert_eq!(imported.job_name, "Backyard privacy fence");
    assert!(!imported.job_id.is_empty());
    assert!(!imported.created_at.is_empty());

    let service = ApplicationService::open(&database).expect("reopen database");
    let jobs = service.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, imported.job_id);
    assert_eq!(jobs[0].name, "Backyard privacy fence");
    assert_eq!(jobs[0].timezone, "America/New_York");
}

#[test]
fn a_second_import_creates_a_distinct_job() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let database = temp.path().join("contractorproject.sqlite3");

    let first = import(&options(envelope.clone(), database.clone())).expect("first import");
    let second = import(&options(envelope, database.clone())).expect("second import");
    assert_ne!(first.job_id, second.job_id);

    let service = ApplicationService::open(&database).expect("reopen database");
    assert_eq!(service.list_jobs().expect("list jobs").len(), 2);
}

#[test]
fn newer_schema_version_is_refused_with_an_update_message() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(
        temp.path(),
        "future.json",
        &VALID_ENVELOPE.replace("\"schemaVersion\": 1", "\"schemaVersion\": 2"),
    );
    let database = temp.path().join("contractorproject.sqlite3");

    let error = import(&options(envelope, database.clone())).expect_err("newer version refused");
    assert!(error.contains("schemaVersion 2"), "{error}");
    assert!(error.contains("Update ContractorProject"), "{error}");
    assert!(
        !database.exists(),
        "a refused import must not create a database"
    );
}

#[test]
fn malformed_json_fails_cleanly() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "broken.json", "{ not json");
    let database = temp.path().join("contractorproject.sqlite3");

    let error = import(&options(envelope, database)).expect_err("malformed JSON refused");
    assert!(error.contains("not a valid hand-off envelope"), "{error}");
}

#[test]
fn a_missing_envelope_file_is_reported_by_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = temp.path().join("absent.json");
    let database = temp.path().join("contractorproject.sqlite3");

    let error = import(&options(envelope, database)).expect_err("missing envelope refused");
    assert!(error.contains("envelope file not found"), "{error}");
}

#[test]
fn unknown_fields_and_a_missing_kind_are_treated_per_the_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    // Additive fields a future CRM might add — the import must ignore them.
    let additive = write_envelope(
        temp.path(),
        "additive.json",
        r#"{
          "schemaVersion": 1,
          "kind": "opportunity_handoff",
          "futureTopLevelField": { "anything": [1, 2, 3] },
          "opportunity": { "name": "Side yard gate", "futureField": true },
          "contact": null
        }"#,
    );
    let database = temp.path().join("contractorproject.sqlite3");
    let imported = import(&options(additive, database)).expect("additive envelope imports");
    assert_eq!(imported.job_name, "Side yard gate");

    // A wrong or missing discriminator is not a hand-off envelope.
    let wrong_kind = write_envelope(
        temp.path(),
        "wrong.json",
        r#"{ "schemaVersion": 1, "kind": "quote_export", "opportunity": { "name": "X" } }"#,
    );
    let error = import(&options(wrong_kind, temp.path().join("other.sqlite3")))
        .expect_err("wrong kind refused");
    assert!(error.contains("quote_export"), "{error}");

    // A blank opportunity name has nothing to name the job after.
    let nameless = write_envelope(
        temp.path(),
        "nameless.json",
        r#"{ "schemaVersion": 1, "kind": "opportunity_handoff", "opportunity": { "name": "  " } }"#,
    );
    let error = import(&options(nameless, temp.path().join("none.sqlite3")))
        .expect_err("blank name refused");
    assert!(error.contains("opportunity.name"), "{error}");
}
