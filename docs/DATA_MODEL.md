# Initial data model

Status: planning baseline
Updated: 2026-08-18

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
- `data_date` nullable canonical `YYYY-MM-DD`; `NULL` while the job is
  unstatused. Set and cleared through an audited, job-version-checked command.
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
- `start_no_earlier_than` and `finish_no_later_than`, independently nullable
  ISO date-only inputs for leaf tasks
- `percent_complete` nullable integer in `[0, 100]`; `NULL` is unstatused
- `actual_start` and `actual_finish`, independently nullable canonical
  `YYYY-MM-DD` inputs for leaf tasks
- calculated start, finish, total float, and critical flag as a replaceable projection
- `created_at`, `updated_at`, `version`

Summary tasks are ordinary tasks with children. Their calculated dates, duration, progress, and critical state are derived and cannot be edited directly.

Persisted input rules: leaf duration is either zero (milestone) or positive
(normal work); `NULL` is used by summaries and permitted while schedule setup
is incomplete. Finish-to-start dependency endpoints must be scheduled leaves.
Summary tasks cannot carry constraints or progress. Schedule-setting,
constraint, data-date, and task-progress edits all validate the complete
proposed schedule (including every persisted constraint and progress entry plus
the job data date, with the candidate edit substituted) through the pure
scheduler before commit; failed validation leaves canonical rows, versions, and
`command_log` unchanged. Progress requires a duration, so reporting progress on a
duration-less leaf is rejected. A schedule start may be left unset only while no
data date or task progress is stranded behind it; clearing the schedule start
under persisted progress or a data date is rejected. A schedule-setting edit is
not blamed for pre-existing invalidity (for example duration-less leaves during
setup): it is rejected only when the current stored inputs validate but the
proposed calendar and schedule start do not.

Progress rules: progress applies only to leaf tasks and is supplied as a
`percent_complete` with optional `actual_start`/`actual_finish`. A cleared or
unstatused row nulls all three columns. The canonical scheduling mapping omits
unstatused rows entirely: a persisted row with `percent_complete` `NULL` or `0`
and no actuals is equivalent to an absent progress entry, matching how the
scheduler treats absent and zero-without-actuals progress identically. Every
other cross-field rule (progress requires a data date, actuals must be on or
before it, milestone percents, actual ordering, and complete-milestone equality)
is owned by the scheduler and surfaced at the persistence boundary rather than
duplicated in storage.

Migration v5 adds the two nullable constraint columns. Migration v6 adds the
nullable `jobs.data_date` and the nullable `tasks.percent_complete`,
`tasks.actual_start`, and `tasks.actual_finish` columns. Existing null values
retain their unstatused meaning. Verified-backup preflight accepts exact v4, v5,
or v6 snapshots without migration; new databases and verified backups use v7.

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

A baseline has an ID, job ID, name, creation timestamp, and immutable task
snapshot rows. Each `baseline_tasks` row snapshots one leaf task's calculated
start and finish instants (canonical `YYYY-MM-DDTHH:MM:SS` text) and duration
minutes, keyed uniquely by baseline and task. Summary rollups are not snapshotted;
they are re-derived from the leaf snapshot. Planned cost is not stored yet: a
`planned_cost` column joins the snapshot once task costs exist. One baseline per
job can be the comparison default, enforced by a partial unique index, and the
default flag flips without mutating any snapshot row.

Creation is an audited, job-version-checked command that requires the current
schedule to calculate through the same progress-aware projection `get_schedule`
uses; blank or duplicate-per-job names are rejected, and the first baseline for a
job becomes the comparison default automatically. Baselines have no update or
delete command — the snapshot is immutable.

Migration v7 adds the `baselines` and `baseline_tasks` tables and the
single-default-per-job index. Verified-backup preflight accepts exact v4, v5, v6,
or v7 snapshots without migration; new databases and verified backups use v7.

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
  weekly calendar, ordered hierarchy, leaf durations, FS dependencies, leaf
  constraints, the job data date, and leaf progress; it is not persisted and
  reads never mutate the canonical inputs.
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

### Verified local backups

A local backup is a consistent whole-database SQLite snapshot made through the
SQLite online-backup API while the application remains open. The destination
must not already exist; the application writes to a unique same-directory
incomplete file, verifies it, and publishes it with an atomic no-clobber
operation. It never overwrites a selected file. The live database is not
modified and backup creation is not a domain command, so it has no
`command_log` row.

Before reporting success, the completed snapshot is opened read-only without
running migrations. Verification requires `integrity_check` to return exactly
`ok`, no `foreign_key_check` rows, an exact supported schema migration version
(4, 5, 6, or 7), the required canonical tables, and bounded count reads from the
job, task, dependency, and audit tables. For v5 and later the preflight also
read-checks that constraint and progress values are canonical leaf inputs, and
for v7 that baseline snapshot rows carry parseable instants, non-negative
durations, references to existing baselines, and at most one default per job. A
failed backup removes only the newly reserved incomplete
destination; it never changes the live database or an existing file.

### Clean-directory restore verification

Restore verification is a developer-facing recovery check, not a normal-app
import flow. It first opens the selected backup read-only and applies the same
integrity, foreign-key, schema-version, required-table, and bounded domain-read
checks as backup creation. The verifier requires a complete supported migration
sequence (through v4, v5, v6, or v7), supported
column/type/nullability/primary-key layouts, required
foreign keys, and exact normalized supported DDL signatures for every required
table and named index, including constraints and partial-index predicates. It
also executes canonical read queries; a database that merely reuses table names
or widens a constraint is rejected without migration. Only then does it use
SQLite's online-backup API to copy that snapshot into a uniquely owned sibling
staging directory containing
`contractorproject.sqlite3`. It atomically reserves the required non-existing
app-data directory first (including rejecting dangling symlinks), re-verifies
the staging database, then publishes it into that reservation with a
no-clobber hard link. The staging hard link is retained as the ownership token
until the result opens and passes final verification; rollback removes a
published target only when its filesystem identity still matches that token.

The source backup is never migrated or changed. A corrupt, truncated,
foreign-schema, or otherwise invalid backup creates no target. An existing
target is rejected without replacement. Failure removes only owned staging or
reserved target artifacts; an unexpected entry in a contended target is never
removed and causes a bounded failure. The operation does not create an audit
record because it does not mutate a job aggregate.

## Archive contract

The portable job archive is a versioned ZIP containing:

- `manifest.json` with archive version, product version, job ID, and checksums
- canonical job data as JSON
- attachments under a confined `assets/` directory
- optional human-readable CSV exports

Import validates paths, checksums, schema version, IDs, and dependency integrity before writing anything. The whole import is transactional.
