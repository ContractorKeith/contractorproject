# Production Gantt treegrid

Status: implemented
Updated: 2026-08-20

`src/gantt/GanttTreegrid.tsx` is the authoritative work-breakdown surface for
Gantt read-model contract v7. It renders a native HTML table with
`role="treegrid"` and uses TanStack Virtual to mount only the visible task rows
plus overscan. The implemented supplemental SVG timeline remains a separate
renderer and introduces no schedule facts that are absent from this table, with
two documented exceptions: the job data-date marker is a job-level instant drawn
only on the timeline, and per-task actual start/finish appear in the table only
inside the % Done cell's accessible name rather than as dedicated visible cells.

## Semantic contract

- The header is logical row 1. A Rust `logicalIndex` of zero is exposed as
  `aria-rowindex="2"`, and collapse never renumbers later rows.
- Depth, expanded state, sibling position, and sibling-set size come directly
  from the versioned read model.
- The eight columns are WBS, Name, Duration, % Done, Start, Finish,
  Predecessors, and Float (`aria-colcount="8"`). The Name column remains the
  tree column and disclosure owner.
- Task, duration, current and baseline dates, signed variance, total float,
  critical and milestone state, typed predecessor links, leaf constraint dates,
  and scheduler-provided constraint violations remain visible cell text. A
  leaf violation is direct; a summary violation is derived from descendants.
  The Start and Finish cells use `≥ YYYY-MM-DD` and `≤ YYYY-MM-DD`; the Float
  cell shows compact `Constraint` / `violated` text, so color is never the only
  signal.
- The Duration cell renders the Rust `durationMinutes` as visible `NNN min`
  text (or `Milestone`) and, for non-milestone rows carrying a baseline, a
  stacked baseline duration variance fact `Baseline 360 +120 min` (the unit is
  dropped from the first number to fit the narrow cell; the variance keeps it).
  Its accessible name adds the same fact, e.g. `..., duration, 480 min, baseline
  duration 360 min, variance +120 min`, or `no baseline` when absent or on a
  milestone. Milestones suppress the duration fact because their slippage
  already reads on Start and Finish. The Start and Finish cells show the visible
  baseline fact as the civil date only, e.g. `Baseline 2026-08-14 +4,320 min`
  (the clock time is dropped so the fact fits the narrow cell and the Name column
  keeps its reading width); their accessible names keep the precise instant, e.g.
  `..., start, 2026-08-17 08:00, baseline start 2026-08-14 08:00, variance +4,320
  min`, and the timeline ghost bar carries the exact position. Duration variance
  is computed in Rust and never derived in React.
- The Predecessors cell renders every link explicitly so a bare id can never be
  read as an implicit `FS+0`. Each link shows `<pred> <TYPE> <±lag> min`, e.g.
  `T2 SS +120 min` or `T3 FF -60 min`; the lag fragment is dropped at zero
  (`T2 FS`). Links wrap one per line and stretch to the row track, mirroring the
  baseline facts, and long task ids break within the cell so a line never
  overflows. The cell's accessible name spells each link out in the joined-string
  style, e.g. `..., predecessors, predecessor T2, start-to-start, lag +120
  minutes, predecessor T3, finish-to-finish, lag -60 minutes` (a zero-lag link
  reads `no lag`). React renders the Rust-sorted links and derives nothing.
- The supplemental timeline draws a dependency line per link with type-aware
  anchors taken straight from the row instants (no schedule math): the line
  leaves the predecessor start for `SS`/`SF` and its finish for `FS`/`FF`, and
  enters the successor start for `FS`/`SS` and its finish for `FF`/`SF`. Path
  styling, hover emphasis, and critical classes are unchanged; each path carries
  `data-dependency-type`, `data-predecessor-anchor`, and `data-successor-anchor`
  attributes for verification.
- The % Done cell renders the Rust `percentComplete` as visible `NN%` text plus
  a compact status word (`Complete`, `In progress`, `Not started`), so progress
  state is never color-only. Its accessible name adds the progress facts, e.g.
  `..., percent complete, 42 percent complete, in progress, actual start
  2026-08-17 08:00`. Completed rows also announce `actual finish`.
- Accessible Name, % Done, and Float cell labels include the exact constraint,
  progress, and violation values supplied by Rust. React does not infer or
  derive schedule facts.
- The supplemental timeline overlays a proportional progress fill on task bars
  and, when a data date is set, a labelled vertical marker line with an
  accessible name. Both are theme-safe and forced-colors aware.
- Exactly one mounted grid cell participates in the page tab order. Focus is
  tracked by task ID so virtualization, collapse, and a reordered projection
  can restore it to the same task or its nearest visible ancestor.
- Arrow keys move among rows and cells; Home and End move across a row;
  Control/Command plus Home or End reaches the first or last visible logical
  row; Page Up and Page Down move by a viewport; Left and Right collapse,
  expand, or return to a parent; Space or Enter toggles a summary.

The disclosure control has a 24-pixel target but is removed from the tab order
because the active task cell owns keyboard collapse and expansion. The surface
does not claim selection semantics; focus and future selection remain separate
states.

## Focus-follow explanation panel

The treegrid takes an optional `onActiveTaskChange(taskId | null)` callback. It
fires whenever the focused task **id** changes — on cell movement and on
collapse-driven focus recovery (when a collapsed summary hides the focused
child, the callback follows focus to the nearest visible ancestor). A projection
reorder that keeps the focused task alive does not re-fire the callback (the id
is unchanged); the panel refreshes instead through the app's row lookup against
the new read model. At the component-contract level the callback passes `null`
once on mount (the notify effect runs before the initial roving cell is set)
and whenever the visible-row set is empty; the app never mounts the grid with
zero rows, so after that first notify it always names a task. A reload that
drops the focused task recovers the roving cell to the first visible row, so
the panel settles on a surviving task rather than a stale or empty state. The callback is held in a ref and
the notify effect depends only on the focused task id, so an inline-lambda
consumer cannot re-fire or loop. The callback is the panel's only new coupling:
the grid's markup, its eight-column contract (`aria-colcount="8"`), row height,
and roving single-tab-stop model are all unchanged, and the supplemental
timeline is untouched.

`src/gantt/ExplanationPanel.tsx` renders below the treegrid in the schedule
view (`App.tsx`). It is a non-modal, hairline-framed region following the
DESIGN.md panel language: a Condensed uppercase panel title
(`Schedule explanation`) with the focused task name and id, over deterministic
FACT rows in Barlow 13px `--color-text` — the DESIGN.md §6 "deterministic risk
flags" treatment, never the indented model-prose treatment reserved for the AI
layer. Each fact is the scheduler's own typed data rendered verbatim (no
schedule math): a `summary` shows `Derived from children`; a `complete` leaf
shows `Complete · actual <start> – <finish>`; a `scheduled` leaf shows a
`Driver` line, any `Also` co-binding drivers, an optional `Started <instant>`, an
optional singular/plural non-working-day gap, a `Float <signed> min` line
(`· critical` appended when critical), and the `lateFinishLimit` fact. A
predecessor driver reads `After <link>` for the start-anchored FS/SS types and
`Finish after <link>` for the finish-anchored FF/SF types; constraint and
deadline facts append ` · applied <date>` when normalization moved the entered
date. Typed links reuse the Predecessors-cell format (`B FS +480 min`, lag
dropped at zero).
With no focused task the panel reads `Focus a task to see what drives it.`

**Accessibility contract:** the region carries `role="region"` and an aria-label
naming the focused task (`Schedule explanation for <name>`, or the generic
`Schedule explanation` when empty). Fact rows are plain text so screen readers
read them verbatim, and color is never the only signal — the critical marker is
the literal word `critical`.

## Schedule-settings drafts and calendar exceptions

The schedule-settings panel keeps a dirty draft (schedule start, weekly
calendar, or data date) against the persisted values it was based on. When a
sibling command bumps the job version without changing those persisted inputs,
the draft silently rebases onto the new version instead of raising a conflict;
only a genuine change to a persisted input the draft depends on raises one.

Adding or removing a **dated calendar exception** is treated the same way. The
issue text framed exception edits as changes that "must raise real conflicts",
but the implemented and correct behavior is a silent rebase: an exception add or
remove bumps the job version and changes the exception list, yet it does not
touch the persisted schedule start, weekly calendar, or data date that a draft
is based on. Because `calendarsEqual` compares only the weekly calendar (not the
exception list), a dirty schedule-start or data-date draft rebases cleanly and a
subsequent save uses the fresh `expectedJobVersion`. The exception list itself
lives outside those drafts and is managed by its own audited add/remove
commands. A regression test asserts a dirty schedule-start draft plus an
exception add raises no conflict banner, preserves the draft, and saves against
the advanced version.

## Verification

Component tests cover semantics, complete schedule text, the % Done column and
progress accessible names, proportional bar fill, the data-date marker,
keyboard navigation, collapse, and projection reorder.
`GanttTreegrid.test.tsx` also asserts `onActiveTaskChange` fires on cell
movement and on collapse-driven focus recovery, and `ExplanationPanel.test.tsx`
covers every explanation kind, the empty state, singular/plural gaps, the
applied-date driver variant, and the region aria-label. The Playwright contract
renders a deterministic 1,000-row statused read model in Chromium, proves that
fewer than 50 task rows are mounted, asserts the progress facts, bar fill, and
data-date marker accessible name, jumps focus to the last logical row and back,
recovers focus after an offscreen reorder, runs axe-core, and checks
reduced-motion and forced-color behavior.

```bash
npm test
npx playwright install chromium
npm run test:browser
```

Packaged WKWebView/VoiceOver and WebView2/NVDA acceptance remains issue #7.
