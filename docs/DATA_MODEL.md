# Initial data model

Status: planning baseline
Updated: 2026-08-14

## Domain language

- A **job** is the top-level contractor-facing record.
- A **task** is a schedulable unit of work inside a job.
- A **summary task** groups child tasks and derives its dates and progress from them.
- A **milestone** is a zero-duration task.
- A **dependency** constrains the relationship between two tasks.
- A **resource** is a crew, person, subcontractor, or equipment placeholder assignable to work.
- A **cost code** classifies planned and actual job cost values; it is not an estimating or accounting ledger.
- A **baseline** is an immutable snapshot used for variance.
- A **data date** is the date through which actual progress is considered current.

Use `job` in the product and schema. Reserve `project` for the ContractorProject product or future suite-level coordination.

## Core records

### `jobs`

- `id`
- `name`
- `job_number` optional, user-controlled
- `status` (`draft`, `active`, `on_hold`, `complete`, `archived`)
- `timezone`
- `start_constraint`
 - `schedule_start` nullable ISO date; incomplete setup remains valid
 - default working calendar: Monday-Friday, 08:00, 480 continuous working minutes
- `data_date`
- `currency_code`
- `created_at`, `updated_at`, `version`

### `tasks`

- `id`, `job_id`
- `parent_task_id` nullable
- `sort_key`
- `name`, `description`
- `kind` (`task`, `milestone`)
- `scheduling_mode` (`auto`, `manual`)
- `duration_minutes`
- optional start/finish constraints
- `percent_complete`
- calculated start, finish, total float, and critical flag as a replaceable projection
- `created_at`, `updated_at`, `version`

Summary tasks are ordinary tasks with children. Their calculated dates, duration, progress, and critical state are derived and cannot be edited directly.

Persisted input rules: leaf duration is either zero (milestone) or positive
(normal work); `NULL` is used by summaries and permitted while schedule setup
is incomplete. Finish-to-start dependency endpoints must be scheduled leaves.

The implemented FS scheduling semantics, including working-minute boundaries,
summary rollups, float, and deterministic path selection, are defined in
[`SCHEDULING.md`](SCHEDULING.md). Calculated schedule fields remain a
replaceable projection rather than canonical persisted inputs.

Task hierarchy writes use two concurrency levels: each task has its own `version`, while the owning job's `version` is the aggregate token for the ordered hierarchy. Creating or editing a task increments the job version. Reordering checks both the moved task and job versions, rewrites affected sibling positions atomically, and increments the version of every task whose parent or order changed.

### `task_dependencies`

- `id`, `job_id`
- `predecessor_task_id`, `successor_task_id`
- `dependency_type` (`FS`, `SS`, `FF`, `SF`)
- `lag_minutes`
- unique predecessor/successor/type tuple

The application rejects self-links, cross-job links, duplicate links, and dependency cycles before commit.

The first scheduling slice calculates FS links with non-negative working-time
lag. Other relationship types and negative lag remain deferred.

### `calendars`

- `id`, optional `job_id`
- `name`, `timezone`
- weekly working intervals
- dated exceptions and holidays

Jobs have one default calendar. Resource-specific calendars are later unless the basic assignment workflow demonstrates a need.

### `resources` and `task_assignments`

Resources hold a display name, kind, optional capacity, and active state. Assignments join a resource to a task with allocation percentage and optional planned minutes. v1 flags over-allocation but does not perform automatic resource leveling.

### `cost_codes` and `task_costs`

Cost codes are job-local in v1 with a code, name, and optional parent. Task costs store planned and actual integer minor currency units by task and cost code. They do not model proposals, purchase orders, invoices, payroll, or accounting periods.

### `baselines` and `baseline_tasks`

A baseline has an ID, job ID, name, creation timestamp, and immutable task snapshot rows containing planned start, finish, duration, and planned cost. One baseline can be marked as the comparison default without mutating the snapshot.

### `job_notes` and `attachments`

Notes are Markdown text owned by a job or task. Attachments store metadata and a managed relative file path. File content stays outside SQLite and is included in the portable job archive.

## Persistence support

- `schema_migrations` records forward-only migrations.
- `command_log` records a unique command ID (at most 128 characters), actor
  (`user`, `agent`, `import`), client name (at most 120 characters), UTC
  timestamp, and a server-generated non-secret summary (at most 240
  characters). The row is committed atomically with its domain mutation;
  duplicate IDs are rejected without rerunning that mutation.
- `app_settings` stores non-secret preferences.
- Provider credentials are never stored in these tables.

## Invariants

- Every child record belongs to exactly one job.
- Task parent links stay within a job and cannot form a cycle.
- All application writes include an expected record version where concurrent or agent edits could overwrite newer work.
- Money uses integers plus an ISO currency code; no floating-point currency values.
- Deletes are recoverable archives inside the app unless the user explicitly purges data.
- A baseline is immutable.
- Calculated schedule fields are reproducible from canonical inputs.
- The schedule read projection is rebuilt from one SQLite snapshot of the job,
  weekly calendar, ordered hierarchy, leaf durations, and FS dependencies; it
  is not persisted and reads never mutate the canonical inputs.
- Imports use stable external IDs or an explicit mapping table so retries do not duplicate records.

### Recoverable job archive

Archiving is a reversible job status transition, not deletion. The normal job
list includes only `draft` jobs; archived jobs are available through a separate
recovery query. Archive changes `draft` to `archived`, while restore always
changes `archived` to `draft`. Each transition checks the job version and
commits one bounded command-log row in the same transaction.

Archived jobs are immutable through normal task, hierarchy, scheduling-input,
and dependency commands. Their child rows, audit history, and derived schedule
inputs remain untouched, so restore reconstructs the same schedule projection.

## Archive contract

The portable job archive is a versioned ZIP containing:

- `manifest.json` with archive version, product version, job ID, and checksums
- canonical job data as JSON
- attachments under a confined `assets/` directory
- optional human-readable CSV exports

Import validates paths, checksums, schema version, IDs, and dependency integrity before writing anything. The whole import is transactional.
