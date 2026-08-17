# Local agent API

Status: proposed v1 contract
Updated: 2026-08-14

## Interface

Ship an MCP helper with the desktop application and use stdio as the v1 transport. The agent client launches the helper; ContractorProject does not open a network listener for normal single-user use.

The MCP adapter calls the same Rust application interface as the desktop UI. It never opens SQLite directly and cannot bypass validation, record-version checks, or audit logging.

## Initial tools

### Read

- `list_jobs(status?, limit?, cursor?)`
- `get_job(jobId, include?)`
- `get_schedule(jobId, window?, includeBaseline?)`
- `list_resources(jobId)`
- `get_job_risks(jobId, dataDate?)`

### Propose

- `propose_work_breakdown(jobId, objective, constraints?)`
- `propose_schedule_change(jobId, request, expectedVersions)`
- `explain_schedule(jobId, taskIds?)`
- `explain_variance(jobId, baselineId?)`

Proposal tools return a typed diff, warnings, affected versions, and an opaque proposal ID. They do not mutate job data.

### Write

- `archive_job(jobId, expectedJobVersion)`
- `restore_job(jobId, expectedJobVersion)`
- `apply_proposal(proposalId, expectedVersions)`
- `create_task(jobId, task, expectedJobVersion)`
- `update_task(taskId, patch, expectedVersion)`
- `reorder_task(taskId, parentTaskId?, siblingIndex, expectedVersion, expectedJobVersion)`
- `add_dependency(jobId, dependency, expectedJobVersion)`
 - `remove_dependency(jobId, predecessorTaskId, successorTaskId, expectedJobVersion)`
 - `update_schedule(jobId, scheduleStart?, calendar, expectedJobVersion)`
 - `update_task_duration(taskId, durationMinutes?, expectedVersion, expectedJobVersion)`
- `record_actual_cost(jobId, taskId, costCodeId, amount, expectedVersion)`

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
