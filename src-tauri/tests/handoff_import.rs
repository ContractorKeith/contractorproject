//! Integration tests for the ContractorCRM hand-off envelope import: a valid
//! envelope creates a job, a newer schemaVersion is refused, malformed JSON
//! fails cleanly, and unknown fields are ignored (additive contract).

use contractorproject_lib::application::{
    ApplicationService, CreateBackupRequest, VerifyRestoreRequest,
};
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
fn retry_after_restart_returns_the_same_job_without_duplicate_audit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let database = temp.path().join("contractorproject.sqlite3");

    let first = import(&options(envelope.clone(), database.clone())).expect("first import");
    drop(ApplicationService::open(&database).expect("process restart reopen"));
    let second = import(&options(envelope, database.clone())).expect("second import");
    assert_eq!(first.job_id, second.job_id);

    let service = ApplicationService::open(&database).expect("reopen database");
    assert_eq!(service.list_jobs().expect("list jobs").len(), 1);
    let audit_rows: i64 = rusqlite::Connection::open(&database)
        .expect("open audit database")
        .query_row("SELECT COUNT(*) FROM command_log", [], |row| row.get(0))
        .expect("count audit rows");
    assert_eq!(audit_rows, 1);
}

#[test]
fn distinct_opportunities_create_distinct_jobs_and_changed_content_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = temp.path().join("contractorproject.sqlite3");
    let original = write_envelope(temp.path(), "one.json", VALID_ENVELOPE);
    let first = import(&options(original, database.clone())).expect("first import");
    let other = write_envelope(
        temp.path(),
        "two.json",
        &VALID_ENVELOPE
            .replace("opportunity-1", "opportunity-2")
            .replace("Backyard privacy fence", "Front yard gate"),
    );
    let second = import(&options(other, database.clone())).expect("distinct opportunity");
    assert_ne!(first.job_id, second.job_id);

    let changed = write_envelope(
        temp.path(),
        "changed.json",
        &VALID_ENVELOPE.replace("Backyard privacy fence", "Changed fence scope"),
    );
    let error = import(&options(changed, database.clone())).expect_err("changed identity refused");
    assert!(error.contains("different imported content"), "{error}");
    let service = ApplicationService::open(&database).expect("reopen database");
    let jobs = service.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 2);
    let first_job = jobs
        .iter()
        .find(|job| job.id == first.job_id)
        .expect("first job remains");
    assert_eq!(first_job.name, "Backyard privacy fence");
}

#[test]
fn retry_after_verified_restore_returns_the_mapped_job() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let database = temp.path().join("source.sqlite3");
    let backup = temp.path().join("source.backup.sqlite3");
    let restored_data = temp.path().join("restored-app-data");
    let first = import(&options(envelope.clone(), database.clone())).expect("first import");
    let source = ApplicationService::open(&database).expect("open source");
    source
        .create_verified_backup(CreateBackupRequest {
            destination: backup.to_string_lossy().into_owned(),
        })
        .expect("create verified backup");
    source
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: backup.to_string_lossy().into_owned(),
            target_app_data_dir: restored_data.to_string_lossy().into_owned(),
        })
        .expect("restore into fresh app data");

    let restored_database = restored_data.join("contractorproject.sqlite3");
    let retried =
        import(&options(envelope, restored_database.clone())).expect("retry after restore");
    assert_eq!(retried.job_id, first.job_id);
    let restored = ApplicationService::open(&restored_database).expect("reopen restored database");
    assert_eq!(restored.list_jobs().expect("list restored jobs").len(), 1);
}

#[test]
fn failed_audit_write_rolls_back_job_and_source_mapping_together() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let database = temp.path().join("contractorproject.sqlite3");
    ApplicationService::open(&database).expect("create application database");
    rusqlite::Connection::open(&database)
        .expect("open database")
        .execute_batch(
            "CREATE TRIGGER fail_import_audit BEFORE INSERT ON command_log
             BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        )
        .expect("install audit failure trigger");

    let error = import(&options(envelope, database.clone())).expect_err("audit failure rolls back");
    assert!(error.contains("could not create the job"), "{error}");
    let connection = rusqlite::Connection::open(&database).expect("reopen database");
    let jobs: i64 = connection
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .unwrap();
    let sources: i64 = connection
        .query_row("SELECT COUNT(*) FROM external_job_sources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!((jobs, sources), (0, 0));
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

#[test]
fn a_foreign_database_with_a_lookalike_migration_version_is_not_migrated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let envelope = write_envelope(temp.path(), "handoff.json", VALID_ENVELOPE);
    let foreign = temp.path().join("lookalike.sqlite3");
    {
        let connection = rusqlite::Connection::open(&foreign).expect("create lookalike database");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 INSERT INTO schema_migrations VALUES (1, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (2, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (3, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (4, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (5, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (6, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (7, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (8, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (9, '2026-01-01T00:00:00.000Z');
                 INSERT INTO schema_migrations VALUES (10, '2026-01-01T00:00:00.000Z');
                 CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);
                 INSERT INTO notes (body) VALUES ('preserve');",
            )
            .expect("seed lookalike schema");
    }
    let before = std::fs::read(&foreign).expect("read before");

    let error = import(&options(envelope, foreign.clone())).expect_err("lookalike refused");
    assert!(
        error.contains("not a supported ContractorProject database"),
        "{error}"
    );
    assert_eq!(std::fs::read(&foreign).expect("read after"), before);
    let connection =
        rusqlite::Connection::open_with_flags(&foreign, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("reopen unchanged file");
    let notes: i64 = connection
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .expect("notes table unchanged");
    assert_eq!(notes, 1);
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
          "opportunity": { "id": "opportunity-side-gate", "name": "Side yard gate", "futureField": true },
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
        r#"{ "schemaVersion": 1, "kind": "opportunity_handoff", "opportunity": { "id": "empty-name", "name": "  " } }"#,
    );
    let error = import(&options(nameless, temp.path().join("none.sqlite3")))
        .expect_err("blank name refused");
    assert!(error.contains("opportunity.name"), "{error}");
}
