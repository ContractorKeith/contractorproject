# Deterministic scheduling contract

Status: implemented FS slice with leaf constraints and data-date/progress
Updated: 2026-08-18

The scheduling core is a pure Rust projection. It accepts canonical task,
dependency, and weekly-calendar inputs and returns calculated dates, total
float, critical flags, and one deterministic driving path. It does not read or
write SQLite and it does not depend on React.

The executable contract lives in `src-tauri/tests/scheduling.rs` and exercises
the public `contractorproject_lib::scheduling::calculate_schedule` seam.

## Time and calendar

- Durations, lag, and float are integer working minutes. Seconds and
  fractional minutes are not represented.
- Calculated timestamps are job-local civil times. The application adapter is
  responsible for pairing them with the job's validated IANA timezone.
- The initial calendar has one half-open working interval per selected weekday:
  `[workday start, workday finish)`. At least one weekday and a positive
  interval that fits inside one local day are required.
- A non-working schedule-start date advances to the next working day at the
  configured start time.
- A positive-duration task consumes working minutes in `[start, finish)`.
  Overnight and non-working gaps consume no duration.
- A finish or milestone at a working-day boundary is reported at that day's
  finish time. A positive-duration FS successor resumes at the next available
  working instant. A milestone can occur exactly at its predecessor's finish.
- Positive FS lag consumes working minutes after the predecessor finish.
  Negative lag is rejected by this slice and remains part of the expanded
  dependency work.

The core intentionally accepts the weekly calendar explicitly. Persisted
default-calendar selection, dated holidays/exceptions, split shifts, and
timezone/DST resolution are separate application slices.

## Task and hierarchy rules

- A leaf task has `durationMinutes: Some(value)` where `value >= 0`.
- A zero-duration leaf is a milestone.
- A summary has children and `durationMinutes: None`. Entered summary duration
  is rejected rather than silently ignored.
- Parent IDs must exist and the hierarchy must be acyclic.
- Hierarchy depth is limited to 256 levels so malformed imports cannot exhaust
  the native stack while summaries are derived.
- Summaries are not dependency-graph vertices. A dependency incident to a
  summary is rejected.

Summary values are derived recursively after leaf scheduling:

- early start: minimum child early start
- early finish: maximum child early finish
- late start: minimum child late start
- late finish: maximum child late finish
- duration: working minutes across the early-date envelope, not the sum of
  child durations
- total float: minimum descendant total float
- critical: true when the derived total float is zero or negative
- constraint violated: true when any descendant leaf violates its own finish-no-later-than

## Finish-to-start graph

Dependencies in this slice are FS links with non-negative working-minute lag.
Unknown endpoints, self-links, duplicates, summary endpoints, and cycles are
rejected before a schedule is returned. Persistence adapters must call the
same validation before committing future dependency mutations.

For every leaf task `t`, the forward pass calculates:

```text
ES(t) = max(schedule start, EF(predecessor) + lag for every predecessor)
EF(t) = ES(t) + duration(t)
```

The unconstrained project finish is the maximum leaf early finish. The
backward pass anchors terminal tasks there and calculates:

```text
LF(t) = min(LS(successor) - lag for every successor)
LS(t) = LF(t) - duration(t)
total float(t) = LS(t) - ES(t)
```

A leaf may have optional job-local civil-date `startNoEarlierThan` (SNET) and
`finishNoLaterThan` (FNLT) constraints. Constraints are supplied separately to
the pure scheduler until the persistence slice adds canonical task fields.
Blank, duplicate, missing, and summary-task constraints are rejected; summary
constraint state is always derived.

SNET normalizes to the start of the first working day on or after its date and
lower-bounds early start. FNLT normalizes to the finish of the last working day
on or before its date and upper-bounds late finish without changing early
dates. Constraints before schedule start remain valid. Signed working offsets
and explicit civil boundary instants preserve prior-workday finishes without
aliasing them to the normalized schedule start. The passes are:

```text
ES(t) = max(0, SNET(t), EF(predecessor) + lag)
LF(t) = min(project finish, FNLT(t), LS(successor) - lag)
LS(t) = LF(t) - duration(t)
TF(t) = LS(t) - ES(t)
```

Negative float is valid. A leaf is critical when `TF <= 0`; summaries derive
criticality from their minimum descendant float. A leaf's `constraintViolated`
is true only when its early finish exceeds its own normalized FNLT, while a
summary derives that state from descendants. Results provide lexically sorted
directly violated leaf IDs.

Working-time offsets collapse non-working gaps. Therefore an SNET milestone at
the next workday start and an FNLT at the prior workday finish can have zero
working-minute float while still being a civil-time violation. The scheduler
preserves the SNET milestone instant and reports the violation explicitly; it
never invents a working-day duration for a zero-duration task.

Topological ties and equal driving branches use lexical task ID order. With no
negative float, the primary-path rule is unchanged. With negative float, the
path starts at the directly violated leaf with the smallest float (then lexical
ID) and walks backward through lexical driving predecessors with equal float.
Summary rows are never included in that path.

## Data date and progress

Progress is supplied separately from the task list, mirroring the constraint
seam. A status update carries an optional job-local civil `dataDate` and a list
of per-leaf entries; each entry has an integer `percentComplete` in `[0, 100]`
and optional `actualStart`/`actualFinish` civil dates. The widest seam is
`calculate_schedule_with_progress(input, constraints, progress)`; the existing
entry points delegate to it with an empty status update, so a job with no data
date and no entries is byte-identical to the unstatused schedule.

Validation rejects rather than partially interprets. Progress entries require a
data date; unknown, duplicate, and summary task IDs are rejected. Percent 0
forbids actuals; `0 < percent < 100` requires an actual start and forbids an
actual finish; percent 100 requires both. Milestones accept only 0 or 100, and
a complete milestone requires equal actual start and finish dates. Actual start
must not follow actual finish, and every actual must be on or before the data
date (civil comparison). A data date with no entries is valid.

Normalization mirrors SNET/FNLT. The data-date instant is the start of the
first working day on or after the date. An actual start normalizes to the start
of the first working day on or after it; an actual finish to the finish of the
last working day on or before it; the normalized start must not follow the
normalized finish.

Scheduling retains the remaining work:

- Remaining duration is `duration - floor(duration * percent / 100)`.
- Every incomplete leaf's remaining work starts no earlier than the data-date
  instant and still honors predecessor finishes plus lag and its own SNET.
- A complete leaf anchors its early and late instants at the normalized actuals,
  ignores SNET, evaluates its FNLT violation against the actual finish, reports
  total float 0, is never critical, and is excluded from the primary driving
  path. A successor of a complete predecessor takes `max(actual finish + lag,
  data-date instant)` as that predecessor's finish-to-start lower bound.
- An in-progress leaf reports its early start at the normalized actual start and
  its early finish at the end of remaining work scheduled from `max(data-date
  instant, predecessor early finishes + lag, SNET)`. The backward pass and float
  are computed over remaining work; negative-float and FNLT reporting are
  otherwise unchanged. Float is measured from the remaining-work start, which is
  not surfaced as a date, so consumers must not derive float by differencing the
  reported early and late starts.
- The project finish that anchors the backward pass is the latest early finish
  over incomplete leaves, falling back to the latest actual finish only when
  every leaf is complete.
- The driving path traces incomplete leaves only. When every leaf is complete
  the path is empty.

Schedule start no longer bounds the projection: a complete leaf whose actuals
fall before schedule start reports an early start before the schedule-start
instant.

Summary percent is the floor of the duration-weighted percent over the
positive-duration leaf descendants. When every leaf descendant is zero-duration
the summary reports 100 if all are complete and 0 otherwise. Summary float and
criticality derive from incomplete descendants only: a summary whose descendants
are all complete reports zero float and is not critical, mirroring the
complete-leaf rule. Summaries do not fabricate actual dates; they expose only the
derived percent. Existing date and violation rollups are unchanged.

## Executable examples

All examples use Monday-Friday, 08:00-16:00, starting Monday 2026-01-05.

| Fixture | Expected result |
| --- | --- |
| `A(960) -> B(480) -> C(0)` | A runs Mon-Tue, B runs Wed, milestone C occurs Wed 16:00; all have zero float and path `A, B, C`. |
| `A -> B/C -> D -> M` | The shorter B branch has 240 minutes float; `A, C, D, M` are critical. |
| Equal B/C branches supplied in different orders | Both branches are critical; primary path remains `A, B, D`. |
| One-day A on Friday followed by one-day B | B starts Monday; the weekend consumes no working time. |
| `A ->(960 lag) M -> B` | M occurs Wednesday 16:00 and B starts Thursday 08:00. |
| `A(960)` with FNLT Monday | A finishes Tuesday, has -480 float, and is directly violated. |
| Weekend SNET/FNLT | Normalize to the following start/prior finish respectively. |
| Summary S1 containing critical A/B and S2 containing floating C | S1 rolls up to zero float; S2 rolls up to 480 minutes float. |
| `A -> B -> C -> A` | Calculation returns `dependency_cycle` and no projection. |
| Data date Wednesday, no entries, on `A(480) -> B(480)` | Both leaves are pushed to start no earlier than Wednesday; A runs Wed, B runs Thu. |
| Complete `A(480)` then 50% `B(960)`, data date Wednesday | A anchors Mon 08:00-16:00; B reports its Tuesday actual start, reschedules 480 remaining minutes from the data date to Wed 16:00, and is the only driving-path leaf. |
| `A(1440) -> B(480)`, data date Tuesday, no entries | A's remaining work runs Tue-Thu; B honors the predecessor finish and starts Friday, past the data date. |
| Complete `A(480)` finishing Thursday with FNLT Wednesday | A reports zero float, is not critical, yet is directly violated against its actual finish. |
| Complete milestone with equal Tuesday actuals | The milestone anchors at Tuesday 16:00 with zero float and is not critical. |
| Summary over 100% `A(480)` and 50% `B(1440)` | Duration-weighted rollup reports 62 percent complete. |
| Summary over two milestones | Reports 100 percent when both are complete, 0 percent otherwise. |
| All-complete `A(480) -> B(480)` | The driving path and critical-task list are empty. |
| Complete `A` with a Saturday data date and Friday/Saturday actuals | The data date rolls to Monday 08:00 and the actuals normalize onto Friday's boundaries. |
| Complete milestone with Friday actuals before a Monday schedule start | It anchors at Friday 16:00, before the schedule-start instant. |
| Complete `A(480)` finishing Wednesday and 90% `B(480)` (48 remaining) | The project finish ignores A; B has zero float and is the sole driving leaf. |
| Summary over complete `A(480)` and not-started `B(480)` beside a long chain | The summary reports B's 2400 float; an all-complete summary reports zero float and is not critical. |
| Complete `A` with equal Saturday actuals under a Monday-Friday calendar | Normalization inverts the civil window and returns `progress_normalized_order`. |
| Data date before schedule start | It clamps to the first working day's start. |

## Deferred semantics

SS, FF, and SF links; negative lag; manual scheduling; dated calendar
exceptions; baselines; progress persistence columns and read-model projection;
multiple daily intervals; and resource calendars are outside this slice.
Unsupported inputs must be rejected by the adapter that introduces them, never
partially interpreted.
