# Local agent API

Status: v1 helper shipped (`contractorproject-mcp`); proposal, risk, resource, and cost tools deferred
Updated: 2026-10-06

## Interface

The MCP helper is a second binary, `contractorproject-mcp`, that speaks MCP over
stdio. The agent client launches the helper; ContractorProject does not open a
network listener for normal single-user use.

```
contractorproject-mcp [--read-only] [--db <path>] [--client-name <name>]
```

- **Mode.** Read-write by default, so an agent can build and manage jobs.
  `--read-only` lists the read tools only; calling a write tool then returns
  `read_only`. `--read-write` is still accepted for older client configs.
- **Database.** `--db` wins, then `CONTRACTORPROJECT_APP_DATA_DIR`, then the app's
  own `contractorproject.sqlite3` in the platform app-data directory. The helper
  never creates a database: a missing path, a directory, or a foreign SQLite file
  is refused after a read-only probe and left untouched.
- **Migration.** Read-write may migrate (storage writes its pre-migration backup)
  and says so on stderr. `--read-only` never migrates; a database behind this build
  is refused. A database written by a newer build is always refused.
- **Protocol.** JSON-RPC 2.0 over newline-delimited stdio, MCP revision
  `2025-06-18`. `initialize` reports the product version and `apiVersion` `1.0`.
  Messages over 1 MiB are discarded at the frame boundary. The structure mirrors
  `contractorbooks-mcp`.
- **Stderr.** Before serving, the helper prints its mode, database path, draft and
  archived job counts, and client name. Never job names.

The MCP adapter calls the same Rust application interface as the desktop UI. It never opens SQLite directly and cannot bypass validation, record-version checks, or audit logging.

Example client configuration (Claude Code, Codex, or any MCP client):

```json
{
  "mcpServers": {
    "contractorproject": {
      "command": "/path/to/contractorproject-mcp",
      "args": ["--client-name", "claude"]
    }
  }
}
```

Add `"--read-only"` to `args` for a client that should not change jobs. Build the helper with
`cargo build --release --bin contractorproject-mcp --manifest-path src-tauri/Cargo.toml`.
Bundling it inside the app (as ContractorBooks does with a Tauri `externalBin`)
is a follow-up.

## Tools (shipped)

### Read (both modes)

- `list_jobs(status?, limit?, offset?)` — draft by default; `{ items, totalCount, limit, offset }`, default 50, max 200
- `get_job(jobId, includeTasks?)`
- `list_tasks(jobId)` — flat pre-order hierarchy, dependencies, job version
- `get_schedule(jobId)` — the desktop Gantt read model
- `list_baselines(jobId)`

### Write (default; hidden with `--read-only`)

Every write takes a caller-supplied `commandId` (see below) plus the same
fields as the matching desktop command:

- `create_job(name, timezone)`
- `archive_job(jobId, expectedJobVersion)`
- `restore_job(jobId, expectedJobVersion)`
- `create_task(jobId, parentTaskId?, name, expectedJobVersion)`
- `update_task(taskId, name, expectedVersion)`
- `reorder_task(taskId, newParentTaskId?, newSiblingIndex, expectedVersion, expectedJobVersion)`
- `add_dependency(jobId, predecessorTaskId, successorTaskId, dependencyType?, lagMinutes, expectedJobVersion)` — both tasks must be scheduled leaf tasks
- `remove_dependency(jobId, predecessorTaskId, successorTaskId, dependencyType?, expectedJobVersion)`
- `update_schedule(jobId, scheduleStart?, calendar, expectedJobVersion)`
- `update_task_duration(taskId, durationMinutes?, expectedVersion, expectedJobVersion)`
- `update_task_constraint(taskId, kind, value?, expectedVersion, expectedJobVersion)`
- `update_job_data_date(jobId, dataDate?, expectedJobVersion)`
- `add_calendar_exception(jobId, date, expectedJobVersion)`
- `remove_calendar_exception(jobId, date, expectedJobVersion)`
- `update_task_progress(taskId, clear, percentComplete?, actualStart?, actualFinish?, expectedVersion, expectedJobVersion)`
- `create_baseline(jobId, name, expectedJobVersion)`
- `set_baseline_comparison_default(jobId, baselineId, expectedJobVersion)`

## Deferred contract tools

These were in the proposed contract but have no application-seam support yet.
Add each one to the helper when its Rust service method exists:

- Read: `list_resources`, `get_job_risks`, and the `window` / `includeBaseline`
  options on `get_schedule`
- Propose/explain: `propose_work_breakdown`, `propose_schedule_change`,
  `explain_schedule`, `explain_variance`, and `apply_proposal` (no proposal store yet)
- Write: `record_actual_cost` (no cost model yet)

List tools page with `limit`/`offset`, matching ContractorBooks, rather than an
opaque cursor.

Write tools are available only in read-write mode. The default agent onboarding experience should make the selected mode visible and reversible.

Every write carries a required `commandContext` through the shared Rust
application interface:

- `commandId`: a non-empty, client-stable ID of at most 128 characters.
- `actor`: one of `user`, `agent`, or `import`.
- `clientName`: a non-empty caller label of at most 120 characters.

The application stores one `command_log` row atomically with a successful
write: command ID, actor, client name, UTC timestamp, and a server-generated
non-secret summary of at most 240 characters. Summaries never include request
bodies, credentials, or user-entered job/task text. Reusing a successfully
applied command ID returns `duplicate_command` and does not repeat the domain
mutation; callers generate a new ID only for a deliberate new command.

## Error contract

Return stable machine-readable error kinds:

- `not_found`
- `invalid_input`
- `validation_failed`
- `dependency_cycle`
- `version_conflict`
- `read_only`
- `proposal_expired`
- `provider_unavailable`
- `duplicate_command`

Validation failures include field paths and safe remediation details. Version conflicts return the current version and require an intentional refresh; they never silently overwrite newer work.

Task hierarchy queries return the owning job version plus a deterministic flat pre-order list. Parent IDs and sort keys preserve nesting without making the UI or agent adapter an alternate source of hierarchy truth.

Schedule-input mutations also use the job version: changing a duration checks
both the task and job versions; calendar, start, and dependency changes check
the job version. A mutation and its non-secret audit summary commit together.

`update_task_constraint` accepts exactly `start_no_earlier_than` or
`finish_no_later_than` as `kind` and an ISO date-only `value`, or `null` to
clear that kind. It checks task and job versions, changes only the named kind,
validates the complete proposed schedule, and returns `TaskMutation`.
Constraints are leaf-only; its fixed audit summary excludes task names, dates,
and request bodies.

`get_schedule` returns the same versioned Gantt read model used by the desktop
UI, optionally bounded to the requested window. The contract is defined in
[`GANTT_READ_MODEL.md`](GANTT_READ_MODEL.md); adapters do not expose SQLite
rows or calculate schedule facts independently.

The desktop command has the same `get_schedule(jobId)` shape. It reads the
job's persisted start, weekly calendar, durations, hierarchy, and FS links in
one SQLite snapshot, then invokes the pure Rust scheduler and Gantt projection.
Invalid or incomplete inputs return a validation error and never mutate state.

The normal desktop `list_jobs` call returns draft jobs by default; callers may
request `archived` explicitly for the recovery view. `archive_job` changes a
draft job to `archived`, and `restore_job` returns it to `draft`. Both commands
check the expected job version and commit exactly one audit row atomically.
Archived jobs are immutable through the current task, hierarchy, schedule,
duration, and dependency mutation commands.

The desktop-only `create_verified_backup()` command opens the operating
system's native Save dialog in Rust, suggests a dated `.sqlite3` filename, and
returns `null` when cancelled. On success it returns only bounded metadata:
`destination`, `createdAtUtc`, `byteSize`, and `verified`. The selected
destination is passed to `ApplicationService` for the online SQLite backup and
read-only verification; the browser client neither chooses a path nor receives
database contents.

`ApplicationService::verify_restore_into_fresh_app_data()` is intentionally a
developer-facing service operation, not a desktop command or normal-app UI.
It accepts a backup path and a required non-existing target app-data directory,
validates the backup read-only before any target exists, atomically reserves
the non-existing target without following symlinks, restores through SQLite's
online-backup API into an owned staging directory, verifies again, publishes
with a no-clobber operation, retains the staging file as an ownership token
through post-open verification, and opens the result through
`ApplicationService`.
Its bounded result reports only verification plus job, task, dependency, and
audit-row counts. It never replaces a running database, migrates the selected
backup, returns database contents, or writes a command-log row.

## Hand-off import (ContractorCRM envelope)

The `handoff-import` binary turns a ContractorCRM hand-off envelope into a job:

```
handoff-import --envelope <path> --database <path> [--timezone <tz>]
```

It validates the envelope's `schemaVersion` (1) and `kind`
(`opportunity_handoff`), refuses a newer major version with a message telling
the user to update ContractorProject, and ignores unknown fields — the envelope
contract is additive within a major version. The job is created through
`ApplicationService::create_job` with actor `import`, so it is validated and
audited like any other write. On success it prints one JSON line,
`{"jobId","jobName","createdAt"}`; errors go to stderr with a nonzero exit.

`--database` is created when the path does not exist yet. An **existing** file
must already be a ContractorProject database at a schema version this build
knows: opening a file migrates it, so a foreign SQLite file (a CRM database, a
browser profile) is refused after a read-only probe and left byte-for-byte
untouched rather than having a ContractorProject schema stamped into it.

The envelope file is the whole interface: ContractorProject does not link
against CRM code, does not read a CRM database, and does not deduplicate
imports (the same envelope imported twice creates two jobs).

## Context and privacy

- Read tools return bounded projections selected by job and requested fields.
- Agent responses omit attachment bodies and provider credentials.
- AI provider calls are separate from MCP access. Local MCP reads do not imply permission to send job data to a model provider.
- Each mutation records actor, client name, command ID, timestamp, and a concise non-secret summary.
- Tool results use cursor pagination and explicit size limits.

## Future local network mode

If LAN/team use ships later, add an opt-in authenticated Streamable HTTP adapter around the same application interface. It must bind to an explicitly selected interface, validate request origin, use per-device credentials, and be disabled by default. Do not make HTTP a prerequisite for the desktop app or stdio helper.

## Versioning

- The helper reports product and API versions during initialization.
- Tool input schemas are additive within a major version.
- Breaking changes require a new major API version and a migration guide.
- Export archive versions and MCP API versions are independent.
