# Versioned Gantt read model

Status: implemented contract v4
Updated: 2026-08-18

The Gantt read model is the only schedule shape consumed by the production
work-breakdown table and supplemental timeline. Rust joins canonical task
metadata, deterministic scheduler output, predecessor identities, and an
optional baseline into this projection. React may filter collapsed rows and
virtualize them, but it does not calculate dates, hierarchy positions,
variance, float, or critical state.

The Rust contract lives in `src-tauri/src/gantt.rs`. Its TypeScript mirror is
`src/types/gantt.ts`. Both currently use `contractVersion: 4`; a breaking
field or semantic change requires a new version.

## Top-level projection

- `contractVersion`: read-model schema version
- `jobId`, `jobVersion`: owning aggregate and the version of the source
  snapshot
- `scheduleStart`, `scheduleFinish`: job-local civil timestamps
- `dataDate`: normalized job-local data-date instant, or `null` when the job is
  unstatused. Rust normalizes the persisted civil date through the scheduler
  (start of the first working day on or after it) before projecting it.
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
- calculated duration, current start/finish, signed total float, and critical state
- `percentComplete` (0–100): a leaf's canonical value or a summary's
  duration-weighted rollup, taken directly from scheduler output.
- nullable `actualStart` and `actualFinish` normalized instants. A leaf reports
  its own actuals; summaries never fabricate actual dates and always report
  `null`.
- `progressStatus` of `completed`, `inProgress`, or `notStarted`, derived in Rust.
  A leaf is completed at 100 percent, not started at 0 percent with no actual
  start, and in progress otherwise. A summary aggregates its descendant leaves:
  all completed is completed, all not started is not started, and any mix is in
  progress. This keeps a summary whose duration-weighted percent floors to zero
  (for example a partially-started set of milestones) reporting `inProgress`
  rather than `notStarted`. React renders this fact and never derives it.
- nullable leaf `startNoEarlierThan` and `finishNoLaterThan` local-civil dates,
  plus explicit `constraintViolated` from the scheduler. A leaf reports its
  direct violation; a summary derives this state from a violated descendant.
  Neither is inferred from propagated negative float.
- stable, sorted predecessor task IDs
- an optional baseline comparison with baseline start/finish/duration and
  signed start/finish/duration variance minutes derived by Rust from the current
  and baseline values. `durationVarianceMinutes` is the current row duration
  minus the baseline duration (added in contract v4).

A baseline may omit a row when that task was created after the immutable
snapshot. The baseline identity remains present at the top level while the
row's `baseline` value is `null`.

Positive start/finish variance means the current date is later than its baseline
date; negative variance means it is earlier. Positive duration variance means
the current task runs longer than its baseline. Contract v2+ reports the exact
local civil-time difference in minutes; v4 adds the duration difference. React
displays these values and never recalculates them.

The application adapter loads the job's comparison-default baseline leaf
snapshot into `GanttBaselineSource`. Only tasks that are still leaves at read
time contribute a comparison: a snapshot row whose task later became a summary
(gained children) or was deleted is filtered out before the join, so summaries
stay derived-only and the builder's defensive `gantt_baseline_task_unknown`
invariant is never tripped by that legal editing history. The comparison-default
baseline's `id` is surfaced as the top-level `baselineId`.

## Query boundary

`build_gantt_read_model` accepts authoritative scheduler output plus metadata
owned by Rust application adapters. It validates exact task/schedule joins,
hierarchy pre-order, unique sibling positions, baseline references, and
predecessor references before returning a serializable projection. Stable
error codes identify malformed joins instead of sending partial schedule data
to React. A scheduled summary with either leaf constraint is rejected with
`gantt_summary_constraint_invalid`; valid summary rows have null constraint
values, while `constraintViolated` remains the scheduler-derived descendant
state.

The application adapter loads canonical SQLite job schedule inputs, tasks,
durations, FS dependencies, weekly calendar, leaf constraints, the job data
date, and per-leaf progress; it passes that snapshot to the pure scheduler, then
joins its result through this builder. Progress uses the canonical mapping
(percent NULL/0 with no actuals are unstatused and omitted), identical to the
storage validation seam. React consumes only the resulting projection and never
becomes an alternate schedule owner. Baselines remain a future canonical feature.

## Progress and data-date rendering facts

The supplemental timeline draws these Rust-provided facts without any schedule
math beyond proportioning:

- The task-bar progress overlay width is strictly `barWidth * percentComplete /
  100`. It never recomputes remaining work. Milestones render as a diamond only
  and never receive a progress overlay, including when complete.
- A vertical data-date marker is drawn at `dataDate` when set as a 1.5px
  `--color-accent` line with a small flag label at the header and an accessible
  name; it scrolls with the bars. The timeline domain always includes the data
  date, so the marker stays inside the drawn ruler and grid even when it falls
  beyond the last finish (an all-complete job with early actuals).
- A not-started milestone driven by the data date reports the prior working
  day's finish instant (the same working instant as the data-date start) and
  therefore draws immediately before the marker line. This is the scheduler's
  truthful instant and is never adjusted in TypeScript.
- When every leaf is complete with pre-start actuals, `scheduleFinish` can fall
  before `scheduleStart`; the timeline clamps bar widths to a positive minimum so
  no NaN or negative-width bar is drawn.

## Executable fixtures

`src-tauri/tests/gantt_read_model.rs` covers:

- an empty schedule and exact camel-case serialization
- a nested summary with current dates, baseline start/finish/duration variance,
  and predecessor IDs, asserting the exact v4 camel-case baseline serialization
- a partial baseline whose later-added task reports a `null` comparison
- the baseline error paths (`gantt_baseline_task_unknown`,
  `gantt_baseline_task_duplicate`)
- a statused job with a data date and complete, in-progress, and not-started
  rows, asserting the exact v4 progress facts and camel-case serialization
- rejected metadata/schedule joins and constrained summaries

`src-tauri/tests/baseline_persistence.rs` additionally proves the application
`get_schedule` path loads the comparison-default baseline into the projection
(exact duration variance) and drops the comparison for a snapshot task that has
since become a summary.
- a deterministic 1,000-row hierarchy with stable WBS, logical index, depth,
  sibling position, and JSON row count

`src/gantt/visibleRows.test.ts` proves collapse hides descendants while the
Rust-provided logical indices remain unchanged.

The production consumer and its keyboard/accessibility contract are specified
in [`GANTT_TREEGRID.md`](GANTT_TREEGRID.md).
