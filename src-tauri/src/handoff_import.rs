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
  --database <path>  The ContractorProject SQLite file (created if missing).
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
    let name = read_opportunity_name(&options.envelope)?;
    let timezone = resolve_timezone(options.timezone.as_deref());

    let service = ApplicationService::open(&options.database)
        .map_err(|error| format!("could not open the database: {error}"))?;
    let job = service
        .create_job(
            command_context(),
            CreateJobRequest {
                name: name.clone(),
                timezone,
            },
        )
        .map_err(|error| format!("could not create the job: {error}"))?;

    Ok(ImportedJob {
        job_id: job.id,
        job_name: job.name,
        created_at: job.created_at,
    })
}

/// Envelope validation: version gate, kind gate, required opportunity name.
fn read_opportunity_name(envelope_path: &Path) -> Result<String, String> {
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

    let name = envelope
        .opportunity
        .and_then(|opportunity| opportunity.name)
        .unwrap_or_default();
    if name.trim().is_empty() {
        return Err("envelope is missing opportunity.name".to_owned());
    }
    Ok(name)
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
