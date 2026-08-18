# Production Gantt treegrid

Status: implemented
Updated: 2026-08-18

`src/gantt/GanttTreegrid.tsx` is the authoritative work-breakdown surface for
Gantt read-model contract v3. It renders a native HTML table with
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
  critical and milestone state, full predecessor IDs, leaf constraint dates,
  and scheduler-provided constraint violations remain visible cell text. A
  leaf violation is direct; a summary violation is derived from descendants.
  The Start and Finish cells use `≥ YYYY-MM-DD` and `≤ YYYY-MM-DD`; the Float
  cell shows compact `Constraint` / `violated` text, so color is never the only
  signal.
- The Duration cell renders the Rust `durationMinutes` as visible `NNN min`
  text (or `Milestone`) and, when the row carries a baseline, a stacked baseline
  duration variance fact `Baseline 360 min +120 min`. Its accessible name adds
  the same fact, e.g. `..., duration, 480 min, baseline duration 360 min,
  variance +120 min`, or `no baseline` when absent. The Start and Finish cell
  labels likewise append `baseline <instant>, variance <signed> min`. Duration
  variance is computed in Rust and never derived in React.
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

## Verification

Component tests cover semantics, complete schedule text, the % Done column and
progress accessible names, proportional bar fill, the data-date marker,
keyboard navigation, collapse, and projection reorder. The Playwright contract
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
