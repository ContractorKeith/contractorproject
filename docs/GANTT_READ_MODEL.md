# Versioned Gantt read model

Status: implemented contract v1
Updated: 2026-08-14

The Gantt read model is the only schedule shape consumed by the production
work-breakdown table and supplemental timeline. Rust joins canonical task
metadata, deterministic scheduler output, predecessor identities, and an
optional baseline into this projection. React may filter collapsed rows and
virtualize them, but it does not calculate dates, hierarchy positions,
variance, float, or critical state.

The Rust contract lives in `src-tauri/src/gantt.rs`. Its TypeScript mirror is
`src/types/gantt.ts`. Both currently use `contractVersion: 1`; a breaking
field or semantic change requires a new version.

## Top-level projection

- `contractVersion`: read-model schema version
- `jobId`, `jobVersion`: owning aggregate and the version of the source
  snapshot
- `scheduleStart`, `scheduleFinish`: job-local civil timestamps
- `baselineId`: selected immutable baseline or `null`
- `rowCount`: total logical rows before React collapse/virtualization
- `criticalTaskIds`: every critical leaf and derived summary ID
- `criticalPath`: the scheduler's deterministic representative leaf path
- `rows`: deterministic hierarchy pre-order

Every timestamp serializes as `YYYY-MM-DDTHH:mm:ss` without a UTC offset. The
job timezone remains separate and must be paired with these values by the
application adapter.

## Row contract

Each row exposes:

- stable `taskId`, nullable `parentTaskId`, `sortKey`, and display `name`
- zero-based `logicalIndex` that never changes when React collapses rows
- zero-based `depth`, one-based `positionInSet`, `setSize`, and `wbs`
- `kind`, `hasChildren`, and explicit milestone/summary flags
- calculated duration, current start/finish, total float, and critical state
- stable, sorted predecessor task IDs
- an optional baseline comparison with baseline start/finish/duration and
  signed start/finish variance minutes derived by Rust from the current and
  baseline local civil timestamps

A baseline may omit a row when that task was created after the immutable
snapshot. The baseline identity remains present at the top level while the
row's `baseline` value is `null`.

Positive variance means the current date is later than its baseline date;
negative variance means it is earlier. Contract v1 reports the exact local
civil-time difference in minutes. React displays that value and never
recalculates it.

## Query boundary

`build_gantt_read_model` accepts authoritative scheduler output plus metadata
owned by Rust application adapters. It validates exact task/schedule joins,
hierarchy pre-order, unique sibling positions, baseline references, and
predecessor references before returning a serializable projection. Stable
error codes identify malformed joins instead of sending partial schedule data
to React.

Durations, dependencies, calendars, and baselines are not yet canonical
SQLite records, so this slice does not invent persistence defaults or mutation
commands for them. The future application adapter will load those records,
call `calculate_schedule`, and pass the result to this query. The contract and
visible-row projection are ready for the production treegrid without making
React an alternate domain owner.

## Executable fixtures

`src-tauri/tests/gantt_read_model.rs` covers:

- an empty schedule and exact camel-case serialization
- a nested summary with current dates, baseline variance, and predecessor IDs
- a rejected metadata/schedule join
- a deterministic 1,000-row hierarchy with stable WBS, logical index, depth,
  sibling position, and JSON row count

`src/gantt/visibleRows.test.ts` proves collapse hides descendants while the
Rust-provided logical indices remain unchanged.

The production consumer and its keyboard/accessibility contract are specified
in [`GANTT_TREEGRID.md`](GANTT_TREEGRID.md).
