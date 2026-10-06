//! `contractorproject-mcp`: the local agent interface, MCP over stdio.
//!
//! The helper opens the same SQLite database through the same
//! `ApplicationService` the desktop app uses — no second write path, no direct
//! SQL. It never creates a database: the app owns creation, so a missing or
//! foreign file is refused with nothing written.
//!
//! Two modes, chosen at launch. Read-write (the default) lists every tool so an
//! agent can build and manage jobs; every write is audited with actor `agent`,
//! the helper's client name, and the caller's command ID. --read-only lists the
//! read tools only, for a client that should look but not change anything.
//! The structure mirrors `contractorbooks-mcp` so the suite's helpers behave alike.

pub mod catalog;
pub mod protocol;
pub mod reads;
pub mod writes;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::application::{ApplicationService, JobStatus};
use crate::error::ApplicationError;
use crate::storage::LATEST_SCHEMA_VERSION;

use catalog::Access;

/// The bundle identifier the app stores its data under (tauri.conf.json).
pub const APP_IDENTIFIER: &str = "com.contractorkeith.contractorproject";
/// The database file name inside the app-data directory (lib.rs).
pub const DATABASE_FILE_NAME: &str = "contractorproject.sqlite3";
/// Client name recorded on audit rows when the launcher does not pick one.
pub const DEFAULT_CLIENT_NAME: &str = "mcp";
/// Said on stderr before any tool runs, and again in `initialize`.
pub const DATA_NOTICE: &str = "This tool exposes job data: job and task names, schedules, \
                               progress, and baselines. Read-write mode can change jobs.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    ReadOnly,
    ReadWrite,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadWrite => "read-write",
        }
    }

    fn allows(self, access: Access) -> bool {
        access == Access::Read || self == Self::ReadWrite
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub mode: Mode,
    pub database_path: Option<PathBuf>,
    pub client_name: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: Mode::ReadWrite,
            database_path: None,
            client_name: DEFAULT_CLIENT_NAME.into(),
        }
    }
}

pub const USAGE: &str = "contractorproject-mcp [--read-only] [--db <path>] [--client-name <name>]

  --read-only         expose the read tools only (default: read-write)
  --read-write        the default; accepted for older client configs
  --db <path>         the database to open (default: the app's own)
  --client-name <n>   the client name recorded on every audit row (default: mcp)
  --help              print this and exit";

/// What the command line asked for.
#[derive(Clone, Debug)]
pub enum Launch {
    Serve(Box<Options>),
    Help,
}

/// Parse the command line. Unknown flags are refused, so a misspelled
/// --read-only never silently starts a server in the wrong mode.
pub fn parse_options<I: IntoIterator<Item = String>>(args: I) -> Result<Launch, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--read-only" => options.mode = Mode::ReadOnly,
            "--read-write" => options.mode = Mode::ReadWrite,
            "--db" => {
                options.database_path = Some(PathBuf::from(args.next().ok_or("--db needs a path")?))
            }
            "--client-name" => {
                let name = args.next().ok_or("--client-name needs a name")?;
                if name.trim().is_empty() || name.chars().count() > 60 {
                    return Err("--client-name must be 1–60 characters".into());
                }
                options.client_name = name.trim().to_string();
            }
            "--help" | "-h" => return Ok(Launch::Help),
            other => return Err(format!("unknown argument {}\n\n{USAGE}", bounded(other))),
        }
    }
    Ok(Launch::Serve(Box::new(options)))
}

/// Where the database lives: --db wins, then CONTRACTORPROJECT_APP_DATA_DIR,
/// then the platform app-data directory Tauri gives the app.
pub fn resolve_database_path(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(dir) = std::env::var_os("CONTRACTORPROJECT_APP_DATA_DIR") {
        return Ok(PathBuf::from(dir).join(DATABASE_FILE_NAME));
    }
    Ok(app_data_dir()?.join(DATABASE_FILE_NAME))
}

fn app_data_dir() -> Result<PathBuf, String> {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set; pass --db".to_string())
    };
    let base = if cfg!(target_os = "macos") {
        home()?.join("Library").join("Application Support")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "APPDATA is not set; pass --db".to_string())?
    } else {
        match std::env::var_os("XDG_DATA_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => home()?.join(".local").join("share"),
        }
    };
    Ok(base.join(APP_IDENTIFIER))
}

/// Read the schema version without writing. Refuses a missing file, a
/// directory, and any SQLite file that is not a ContractorProject database.
pub fn probe_schema_version(path: &Path) -> Result<i64, String> {
    if !path.exists() {
        return Err(format!(
            "no ContractorProject database at {} — open the app once to create it, or pass --db",
            path.display()
        ));
    }
    if !path.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let stored: Option<i64> = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|_| format!("{} is not a ContractorProject database", path.display()))?;
    stored.ok_or_else(|| format!("{} is not a ContractorProject database", path.display()))
}

/// The open database, the mode, and the name this client audits under.
pub struct Server {
    service: ApplicationService,
    mode: Mode,
    client_name: String,
}

impl Server {
    /// Open an existing database. Read-only never migrates: a database behind
    /// this build is refused rather than upgraded. Read-write may migrate (the
    /// storage layer backs the file up first) and says so on `notices`.
    pub fn open(
        path: &Path,
        mode: Mode,
        client_name: &str,
        notices: &mut impl Write,
    ) -> Result<Self, String> {
        let applied = probe_schema_version(path)?;
        if applied > LATEST_SCHEMA_VERSION {
            return Err(format!(
                "{} was written by a newer ContractorProject (schema v{applied}, this build \
                 supports v{LATEST_SCHEMA_VERSION}) — update the app",
                path.display()
            ));
        }
        if applied < LATEST_SCHEMA_VERSION {
            match mode {
                Mode::ReadOnly => {
                    return Err(format!(
                        "this database needs migration to v{LATEST_SCHEMA_VERSION} (it is at \
                         v{applied}); open it in the app first, or run without --read-only"
                    ))
                }
                Mode::ReadWrite => {
                    let _ = writeln!(
                        notices,
                        "migrating database v{applied} -> v{LATEST_SCHEMA_VERSION} (backup written)"
                    );
                }
            }
        }
        let service = ApplicationService::open(path).map_err(|error| error.to_string())?;
        Ok(Self {
            service,
            mode,
            client_name: client_name.to_string(),
        })
    }

    /// What stderr shows before any tool runs: mode, file, job counts. Never job names.
    pub fn context_preview(&self, path: &Path) -> String {
        let count = |status| {
            self.service
                .list_jobs_by_status(status)
                .map(|jobs| jobs.len().to_string())
                .unwrap_or_else(|_| "unknown".into())
        };
        format!(
            "ContractorProject MCP helper\n  mode: {}\n  database: {}\n  jobs: {} draft, {} archived\n  client: {}\n  {DATA_NOTICE}\n",
            self.mode.label(),
            path.display(),
            count(JobStatus::Draft),
            count(JobStatus::Archived),
            self.client_name,
        )
    }

    pub fn instructions(&self) -> String {
        format!(
            "ContractorProject local jobs, {} mode. {DATA_NOTICE} Read before you write: every \
             write needs the version you last read (expectedJobVersion / expectedVersion) and a \
             unique commandId. A version_conflict means re-read and retry with the current \
             version; never guess. Durations are working minutes (480 = one 8-hour day); dates \
             are ISO YYYY-MM-DD. Every write is audited as actor \"agent\" under client \"{}\".",
            self.mode.label(),
            self.client_name,
        )
    }

    /// The tools this mode exposes. Write tools are not listed in read-only.
    pub fn tool_descriptors(&self) -> Vec<Value> {
        catalog::tools()
            .iter()
            .filter(|tool| self.mode.allows(tool.access))
            .map(catalog::descriptor)
            .collect()
    }

    /// Run one tool by name.
    pub fn call_tool(&mut self, name: &str, args: &Value) -> Result<Value, ApplicationError> {
        let tool = catalog::find(name).ok_or_else(|| ApplicationError::NotFound {
            resource: "tool",
            id: bounded(name),
        })?;
        if !self.mode.allows(tool.access) {
            return Err(protocol::read_only_error());
        }
        if let Some(answer) = reads::call(&self.service, name, args) {
            return answer;
        }
        match writes::call(&self.service, &self.client_name, name, args) {
            Some(answer) => answer,
            None => Err(ApplicationError::NotFound {
                resource: "tool",
                id: bounded(name),
            }),
        }
    }

    /// One request in, at most one response out.
    pub fn handle(&mut self, line: &str) -> Option<Value> {
        let request: protocol::Request = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(_) => {
                return Some(protocol::error(
                    Value::Null,
                    protocol::PARSE_ERROR,
                    "the message was not a JSON-RPC request",
                    None,
                ))
            }
        };
        let id = match request.id.clone() {
            None => {
                let _ = self.respond(&request);
                return None;
            }
            Some(id) => id.unwrap_or(Value::Null),
        };
        match self.respond(&request) {
            Ok(value) => Some(protocol::result(id, value)),
            Err(failure) => Some(protocol::error(
                id,
                failure.code,
                &failure.message,
                failure.data,
            )),
        }
    }

    fn respond(&mut self, request: &protocol::Request) -> Result<Value, Failure> {
        match request.method.as_str() {
            "initialize" => Ok(json!({
                "protocolVersion": protocol::PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": {
                    "name": "contractorproject-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                    "apiVersion": API_VERSION,
                },
                "instructions": self.instructions(),
            })),
            "notifications/initialized" | "notifications/cancelled" => Ok(Value::Null),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.tool_descriptors() })),
            "tools/call" => self.respond_to_call(&request.params),
            other => Err(Failure {
                code: protocol::METHOD_NOT_FOUND,
                message: format!("{} is not a method this server implements", bounded(other)),
                data: None,
            }),
        }
    }

    fn respond_to_call(&mut self, params: &Value) -> Result<Value, Failure> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| Failure {
                code: protocol::INVALID_REQUEST,
                message: "tools/call needs a tool name".into(),
                data: None,
            })?
            .to_string();
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(arguments) => arguments.clone(),
        };
        // A panic in a tool is a bug, not a reason to drop the session; SQLite
        // rolls back an open transaction as it unwinds.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.call_tool(&name, &arguments)
        }));
        match outcome {
            Ok(Ok(value)) => Ok(protocol::tool_content(&value)),
            Ok(Err(error)) => Err(Failure {
                code: protocol::error_code(&error),
                message: bounded(&error.to_string()),
                data: Some(protocol::error_data(&error)),
            }),
            Err(_) => Err(Failure {
                code: protocol::INTERNAL_ERROR,
                message: format!("{} failed unexpectedly", bounded(&name)),
                data: Some(json!({ "kind": "internal_error" })),
            }),
        }
    }

    /// Read newline-delimited JSON-RPC from `input` and answer on `output`.
    /// Oversized and non-UTF-8 messages are dropped without ending the session.
    pub fn serve(
        &mut self,
        mut input: impl BufRead,
        mut output: impl Write,
        mut notices: impl Write,
    ) -> std::io::Result<()> {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            buffer.clear();
            match read_line_capped(&mut input, &mut buffer)? {
                Line::Eof => break,
                Line::Oversized => {
                    let answer = protocol::error(
                        Value::Null,
                        protocol::INVALID_REQUEST,
                        &format!("a message larger than {MAX_LINE_BYTES} bytes was discarded"),
                        None,
                    );
                    writeln!(output, "{answer}")?;
                    output.flush()?;
                    continue;
                }
                Line::Ready => {}
            }
            let Ok(line) = std::str::from_utf8(&buffer) else {
                let _ = writeln!(notices, "skipped a message that was not valid UTF-8");
                continue;
            };
            if line.trim().is_empty() {
                continue;
            }
            if let Some(answer) = self.handle(line) {
                writeln!(output, "{answer}")?;
                output.flush()?;
            }
        }
        Ok(())
    }
}

/// The agent API version reported at initialize (docs/LOCAL_API.md "Versioning").
pub const API_VERSION: &str = "1.0";

/// The most a single JSON-RPC message may weigh.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

enum Line {
    Ready,
    Oversized,
    Eof,
}

/// Read one newline-terminated message, keeping at most MAX_LINE_BYTES. An
/// oversized line is consumed to its end and discarded at the frame boundary.
fn read_line_capped(input: &mut impl BufRead, buffer: &mut Vec<u8>) -> std::io::Result<Line> {
    let mut oversized = false;
    let mut saw_bytes = false;
    loop {
        let (newline_at, consumed) = {
            let available = input.fill_buf()?;
            if available.is_empty() {
                break;
            }
            saw_bytes = true;
            let newline_at = available.iter().position(|byte| *byte == b'\n');
            let take = newline_at.unwrap_or(available.len());
            if !oversized && buffer.len() + take <= MAX_LINE_BYTES {
                buffer.extend_from_slice(&available[..take]);
            } else {
                oversized = true;
                buffer.clear();
            }
            (newline_at, take)
        };
        match newline_at {
            Some(_) => {
                input.consume(consumed + 1);
                return Ok(if oversized {
                    Line::Oversized
                } else {
                    Line::Ready
                });
            }
            None => input.consume(consumed),
        }
    }
    Ok(match (saw_bytes, oversized) {
        (false, _) => Line::Eof,
        (true, true) => Line::Oversized,
        (true, false) => Line::Ready,
    })
}

struct Failure {
    code: i64,
    message: String,
    data: Option<Value>,
}

/// Deserialize a tool's arguments; bad shapes are `invalid_input` on `arguments`.
pub(crate) fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ApplicationError> {
    serde_json::from_value(value).map_err(|error| ApplicationError::InvalidInput {
        field: "arguments",
        message: bounded(&error.to_string()),
    })
}

/// Serialize a service result for the wire.
pub(crate) fn to_json<T: Serialize>(
    result: Result<T, ApplicationError>,
) -> Result<Value, ApplicationError> {
    result.and_then(|value| {
        serde_json::to_value(value)
            .map_err(|error| ApplicationError::InvalidStoredData(error.to_string()))
    })
}

/// Cap anything that echoes caller input, so a huge argument cannot come back huge.
pub(crate) fn bounded(message: &str) -> String {
    const MAX: usize = 200;
    match message.char_indices().nth(MAX) {
        Some((index, _)) => format!("{}…", &message[..index]),
        None => message.to_string(),
    }
}

/// The binary's whole body: parse, open, announce, serve.
pub fn run() -> Result<(), String> {
    let options = match parse_options(std::env::args().skip(1))? {
        Launch::Help => {
            println!("{USAGE}");
            return Ok(());
        }
        Launch::Serve(options) => *options,
    };
    let path = resolve_database_path(options.database_path.clone())?;
    let mut stderr = std::io::stderr();
    let mut server = Server::open(&path, options.mode, &options.client_name, &mut stderr)?;
    let _ = write!(stderr, "{}", server.context_preview(&path));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    server
        .serve(stdin.lock(), stdout.lock(), &mut stderr)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serve_options(args: &[&str]) -> Options {
        match parse_options(args.iter().map(|arg| arg.to_string())).expect("options") {
            Launch::Serve(options) => *options,
            Launch::Help => panic!("expected options"),
        }
    }

    #[test]
    fn read_write_is_the_default_and_read_only_is_explicit() {
        assert_eq!(serve_options(&[]).mode, Mode::ReadWrite);
        assert_eq!(serve_options(&["--read-write"]).mode, Mode::ReadWrite);
        let options = serve_options(&["--read-only", "--client-name", "claude"]);
        assert_eq!(options.mode, Mode::ReadOnly);
        assert_eq!(options.client_name, "claude");
    }

    #[test]
    fn an_unknown_flag_is_refused() {
        let error = parse_options(["--readonly".to_string()]).expect_err("refused");
        assert!(error.contains("unknown argument"), "{error}");
    }

    #[test]
    fn an_oversized_message_is_discarded_at_the_frame_boundary() {
        let huge = format!("{}\n{{\"ok\":true}}\n", "x".repeat(MAX_LINE_BYTES + 10));
        let mut input = std::io::Cursor::new(huge.into_bytes());
        let mut buffer = Vec::new();
        assert!(matches!(
            read_line_capped(&mut input, &mut buffer).unwrap(),
            Line::Oversized
        ));
        buffer.clear();
        assert!(matches!(
            read_line_capped(&mut input, &mut buffer).unwrap(),
            Line::Ready
        ));
        assert_eq!(String::from_utf8_lossy(&buffer), "{\"ok\":true}");
    }

    /// Every catalog tool must have a dispatch arm, so a tool added to the
    /// catalog without a handler fails here instead of in a client.
    #[test]
    fn every_catalog_tool_has_a_dispatch_arm() {
        const READ_NAMES: &[&str] = &[
            "list_jobs",
            "get_job",
            "list_tasks",
            "get_schedule",
            "list_baselines",
        ];
        for tool in catalog::tools() {
            let known = match tool.access {
                Access::Read => READ_NAMES.contains(&tool.name),
                Access::Write => writes::WRITE_NAMES.contains(&tool.name),
            };
            assert!(known, "{} has no dispatch arm", tool.name);
        }
        let write_count = catalog::tools()
            .iter()
            .filter(|t| t.access == Access::Write)
            .count();
        assert_eq!(write_count, writes::WRITE_NAMES.len());
    }
}
