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

/// Opening a database file migrates it, so pointing --database at somebody
/// else's SQLite file would stamp a ContractorProject schema into it. An
/// existing file has to already be ours; a path that does not exist yet is
/// still created (the tool's documented behavior).
#[test]
fn a_foreign_database_is_refused_and_left_untouched() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);

    // Somebody else's SQLite file — real database, not our schema.
    let foreign = temp.path().join("someone-elses.sqlite3");
    {
        let connection = rusqlite::Connection::open(&foreign).expect("create foreign database");
        connection
            .execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);")
            .expect("seed foreign database");
    }
    let before = std::fs::read(&foreign).expect("read foreign database");

    let error =
        import(&options(envelope.clone(), foreign.clone())).expect_err("foreign database refused");
    assert!(error.contains("no ContractorProject schema"), "{error}");
    assert_eq!(
        std::fs::read(&foreign).expect("reread foreign database"),
        before,
        "a refused import must leave the file byte-identical"
    );

    // Not even a plain non-database file gets opened and migrated.
    let text_file = temp.path().join("notes.txt");
    std::fs::write(&text_file, b"just some text").expect("write text file");
    let error = import(&options(envelope.clone(), text_file.clone())).expect_err("text refused");
    assert!(
        error.contains("could not be read") || error.contains("no ContractorProject schema"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(&text_file).expect("reread text file"),
        b"just some text"
    );

    // A path that does not exist yet is still created fresh.
    let fresh = temp.path().join("new").join("contractorproject.sqlite3");
    let imported = import(&options(envelope, fresh.clone())).expect("fresh database is created");
    assert_eq!(imported.job_name, "Backyard privacy fence");
    let service = ApplicationService::open(&fresh).expect("reopen fresh database");
    assert_eq!(service.list_jobs().expect("list jobs").len(), 1);
}

/// A database written by a newer build is refused rather than migrated
/// backwards or half-read.
#[test]
fn a_database_from_a_newer_build_is_refused() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let database = temp.path().join("contractorproject.sqlite3");
    import(&options(envelope.clone(), database.clone())).expect("seed a real database");

    {
        let connection = rusqlite::Connection::open(&database).expect("open database");
        connection
            .execute_batch(
                "INSERT INTO schema_migrations (version, applied_at) \
                 VALUES (9999, '2099-01-01T00:00:00.000Z');",
            )
            .expect("pretend a newer build wrote it");
    }

    let error = import(&options(envelope, database)).expect_err("newer schema refused");
    assert!(error.contains("schema v9999"), "{error}");
    assert!(error.contains("update ContractorProject"), "{error}");
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
