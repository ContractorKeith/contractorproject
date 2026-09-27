//! Import a ContractorCRM hand-off envelope as a new ContractorProject job.
//!
//! The envelope file is the entire interface: ContractorProject never links
//! against CRM code and never reads a CRM database. The envelope is versioned
//! (`schemaVersion`), additive within a major version, so unknown fields are
//! ignored on purpose.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::{ApplicationService, CommandActor, CommandContext, CreateJobRequest};
use crate::storage::{verify_existing_project_database, LATEST_SCHEMA_VERSION};

/// The only envelope major version this build understands.
pub const SUPPORTED_SCHEMA_VERSION: i64 = 1;
/// The envelope discriminator ContractorCRM writes for opportunity hand-offs.
pub const ENVELOPE_KIND: &str = "opportunity_handoff";
/// Fallback when neither the flag nor the environment names a time zone.
pub const DEFAULT_TIMEZONE: &str = "UTC";
/// Client label recorded in the command log for imported jobs.
pub const CLIENT_NAME: &str = "handoff-import";

pub const USAGE: &str = "\
ContractorProject hand-off import (ContractorCRM envelope -> job)

Usage:
  handoff-import --envelope <path> --database <path> [--timezone <tz>]

Options:
  --envelope <path>  A ContractorCRM hand-off envelope JSON file.
  --database <path>  The ContractorProject SQLite file. Created if the path does
                     not exist; an existing file must already be a
                     ContractorProject database.
  --timezone <tz>    Time zone for the new job. Default: $TZ, else UTC.
  -h, --help         Show this message.
";

/// One line of stdout on success.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedJob {
    pub job_id: String,
    pub job_name: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    pub envelope: PathBuf,
    pub database: PathBuf,
    pub timezone: Option<String>,
}

// Unknown fields are ignored deliberately: the envelope contract is
// additive-only within a major version.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    schema_version: Option<i64>,
    kind: Option<String>,
    opportunity: Option<EnvelopeOpportunity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvelopeOpportunity {
    id: Option<String>,
    name: Option<String>,
}

/// Parse the command line. `Ok(None)` means "help was asked for".
pub fn parse_arguments(arguments: &[String]) -> Result<Option<Options>, String> {
    let mut envelope: Option<PathBuf> = None;
    let mut database: Option<PathBuf> = None;
    let mut timezone: Option<String> = None;
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].as_str();
        match argument {
            "-h" | "--help" => return Ok(None),
            "--envelope" | "--database" | "--timezone" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| format!("{argument} needs a value"))?;
                assign(argument, value, &mut envelope, &mut database, &mut timezone);
            }
            other => {
                let split = other.split_once('=').filter(|(name, _)| {
                    matches!(*name, "--envelope" | "--database" | "--timezone")
                });
                match split {
                    Some((name, value)) => {
                        assign(name, value, &mut envelope, &mut database, &mut timezone)
                    }
                    None => return Err(format!("unknown option: {other}")),
                }
            }
        }
        index += 1;
    }

    Ok(Some(Options {
        envelope: envelope.ok_or_else(|| "--envelope <path> is required".to_owned())?,
        database: database.ok_or_else(|| "--database <path> is required".to_owned())?,
        timezone,
    }))
}

fn assign(
    name: &str,
    value: &str,
    envelope: &mut Option<PathBuf>,
    database: &mut Option<PathBuf>,
    timezone: &mut Option<String>,
) {
    match name {
        "--envelope" => *envelope = Some(PathBuf::from(value)),
        "--database" => *database = Some(PathBuf::from(value)),
        _ => *timezone = Some(value.to_owned()),
    }
}

/// Read + validate an envelope and create the matching job. Errors are
/// user-facing strings; the caller prints them to stderr and exits nonzero.
pub fn import(options: &Options) -> Result<ImportedJob, String> {
    let (source_id, name) = read_opportunity(&options.envelope)?;
    let timezone = resolve_timezone(options.timezone.as_deref());

    guard_database(&options.database)?;
    let service = ApplicationService::open(&options.database)
        .map_err(|error| format!("could not open the database: {error}"))?;
    let job = service
        .import_handoff_job(
            command_context(),
            CreateJobRequest {
                name: name.clone(),
                timezone,
            },
            "ContractorCRM".to_owned(),
            source_id,
        )
        .map_err(|error| format!("could not create the job: {error}"))?;

    Ok(ImportedJob {
        job_id: job.id,
        job_name: job.name,
        created_at: job.created_at,
    })
}

/// Refuse to write into a database file this tool did not create.
///
/// `--database` is documented as "created if missing", so a path that does not
/// exist is still made fresh. But an *existing* file is opened and migrated,
/// and that would stamp a ContractorProject schema into somebody else's SQLite
/// file (a CRM database, a browser profile, anything). So an existing file has
/// to already carry our schema, at a version this build understands.
fn guard_database(database_path: &Path) -> Result<(), String> {
    if !database_path.exists() {
        return Ok(());
    }
    if !database_path.is_file() {
        return Err(format!(
            "{} is not a file; point --database at a ContractorProject database",
            database_path.display()
        ));
    }

    // Read-only so a refusal never touches the file.
    let connection = rusqlite::Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("{} could not be read: {error}", database_path.display()))?;
    let stored: Option<i64> = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|_| {
            format!(
                "{} has no ContractorProject schema (no readable schema_migrations table); \
                 point --database at a ContractorProject database, or at a path that does \
                 not exist yet to create one",
                database_path.display()
            )
        })?;
    let Some(stored) = stored else {
        return Err(format!(
            "{} has an empty schema_migrations table; not a ContractorProject database",
            database_path.display()
        ));
    };
    if stored > LATEST_SCHEMA_VERSION {
        return Err(format!(
            "{} was written by a newer ContractorProject (schema v{stored}, this build knows \
             v{LATEST_SCHEMA_VERSION}); update ContractorProject before importing",
            database_path.display()
        ));
    }
    verify_existing_project_database(database_path).map_err(|_| {
        format!(
            "{} is not a supported ContractorProject database (minimum schema v4); refusing to migrate it",
            database_path.display()
        )
    })?;
    Ok(())
}

/// Envelope validation: version gate, kind gate, required opportunity name.
fn read_opportunity(envelope_path: &Path) -> Result<(String, String), String> {
    if !envelope_path.is_file() {
        return Err(format!(
            "envelope file not found: {}",
            envelope_path.display()
        ));
    }
    let raw = std::fs::read_to_string(envelope_path)
        .map_err(|error| format!("could not read {}: {error}", envelope_path.display()))?;
    let envelope: Envelope = serde_json::from_str(&raw).map_err(|error| {
        format!(
            "{} is not a valid hand-off envelope: {error}",
            envelope_path.display()
        )
    })?;

    match envelope.schema_version {
        Some(SUPPORTED_SCHEMA_VERSION) => {}
        Some(version) if version > SUPPORTED_SCHEMA_VERSION => {
            return Err(format!(
                "envelope schemaVersion {version} is newer than this build supports \
                 (schemaVersion {SUPPORTED_SCHEMA_VERSION}). Update ContractorProject."
            ))
        }
        Some(version) => {
            return Err(format!(
                "envelope schemaVersion {version} is not supported (expected {SUPPORTED_SCHEMA_VERSION})"
            ))
        }
        None => return Err("envelope is missing schemaVersion".to_owned()),
    }

    match envelope.kind.as_deref() {
        Some(ENVELOPE_KIND) => {}
        Some(other) => {
            return Err(format!(
                "envelope kind {other:?} is not supported (expected {ENVELOPE_KIND:?})"
            ))
        }
        None => return Err("envelope is missing kind".to_owned()),
    }

    let opportunity = envelope
        .opportunity
        .ok_or_else(|| "envelope is missing opportunity".to_owned())?;
    let source_id = opportunity.id.unwrap_or_default();
    if source_id.trim().is_empty() {
        return Err("envelope is missing opportunity.id".to_owned());
    }
    let name = opportunity.name.unwrap_or_default();
    if name.trim().is_empty() {
        return Err("envelope is missing opportunity.name".to_owned());
    }
    Ok((source_id, name))
}

/// Flag wins, then the process time zone, then UTC.
fn resolve_timezone(flag: Option<&str>) -> String {
    let candidates = [
        flag.map(str::to_owned),
        std::env::var("TZ").ok(),
        Some(DEFAULT_TIMEZONE.to_owned()),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_TIMEZONE.to_owned())
}

fn command_context() -> CommandContext {
    CommandContext {
        command_id: Uuid::now_v7().to_string(),
        actor: CommandActor::Import,
        client_name: CLIENT_NAME.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags_in_both_spellings() {
        let arguments = [
            "--envelope=/tmp/e.json",
            "--database",
            "/tmp/db.sqlite3",
            "--timezone",
            "America/New_York",
        ]
        .map(str::to_owned);
        let options = parse_arguments(&arguments)
            .expect("parse")
            .expect("not help");
        assert_eq!(options.envelope, PathBuf::from("/tmp/e.json"));
        assert_eq!(options.database, PathBuf::from("/tmp/db.sqlite3"));
        assert_eq!(options.timezone.as_deref(), Some("America/New_York"));
    }

    #[test]
    fn help_and_missing_flags_are_distinguished() {
        assert!(parse_arguments(&["--help".to_owned()])
            .expect("parse")
            .is_none());
        let error = parse_arguments(&["--envelope".to_owned(), "/tmp/e.json".to_owned()])
            .expect_err("database is required");
        assert!(error.contains("--database"));
    }

    #[test]
    fn timezone_falls_back_to_utc_when_nothing_is_set() {
        assert_eq!(resolve_timezone(Some("America/Chicago")), "America/Chicago");
        assert_eq!(resolve_timezone(Some("   ")), resolve_timezone(None));
    }
}
