# Production Gantt treegrid

Status: implemented
Updated: 2026-09-08

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
- The contractor workspace defaults to six columns: WBS, Name, Duration,
  % Done, Start and Finish (`aria-colcount="6"`). **Show schedule details** adds
  Predecessors and Float (`aria-colcount="8"`), baseline and constraint text,
  and day-based variance. Standalone verification defaults to this detailed
  mode. The Name column remains the tree column and disclosure owner.
- Current and baseline dates render civil dates. Duration, dependency lag,
  total float and duration variance render working days using the saved job
  calendar; start/finish variance renders elapsed calendar days. The internal
  read model retains exact timestamps and minute values for schedule geometry.
- Leaf constraint dates use `≥ YYYY-MM-DD` and `≤ YYYY-MM-DD`; violated
  constraints and critical/milestone state remain text as well as color.
- Duration cells show days (or `Milestone`) with baseline duration and signed
  variance when present. Accessible names include the same day-based facts.
  Baseline Start and Finish cells include the civil date and signed calendar-day
  variance; the timeline ghost bar retains its exact position. Rust computes
  all variance values; React only converts their presentation units.
- Predecessor cells spell out every link: `T2 SS +2 days` or `T3 FF -1 day`;
  zero lag is omitted (`T2 FS`). Accessible names spell out the relationship
  and signed day value. Links wrap one per line, long ids break within the
  cell, and Rust supplies the sorted links.
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

Name/WBS search and All / Needs attention / In progress / Completed filters
preserve matching rows' ancestors. Match counts exclude contextual ancestors.
Filtering temporarily expands the matching hierarchy; summary toggles are
disabled until the filter clears, at which point saved collapse state returns.
Filtering preserves Rust logical row indices, and recovers focus after an
offscreen or empty result. Hiding details clamps an active diagnostic column to
Finish while preserving the active task.

The disclosure control has a 24-pixel target but is removed from the tab order
because the active task cell owns keyboard collapse and expansion. The grid
exposes focus rather than `aria-selected`; the sibling editor can retain its
prior task when its unsaved-draft guard rejects a switch.

`initialActiveTaskId` optionally carries the already edited task into a newly
mounted projection. After CSS row height is measured, the grid positions that
row in the virtual viewport without taking document focus. This preserves the
editor when saving the last missing duration first makes the schedule calculable.

## Focus-follow explanation panel

The treegrid takes an optional `onActiveTaskChange(taskId | null)` callback. It
fires whenever the focused task **id** changes — on cell movement and on
collapse-driven focus recovery (when a collapsed summary hides the focused
child, the callback follows focus to the nearest visible ancestor). A projection
reorder that keeps the focused task alive does not re-fire the callback (the id
is unchanged); the panel refreshes instead through the app's row lookup against
the new read model. At the component-contract level the callback reports the
initial active row (the supplied task when present, otherwise the first row),
or `null` when filtering leaves no visible rows. A reload that
drops the focused task recovers the roving cell to the first visible row, so
the panel settles on a surviving task rather than a stale or empty state. The callback is held in a ref and
the notify effect depends only on the focused task id, so an inline-lambda
consumer cannot re-fire or loop. The app also uses this callback to open its
single task editor, subject to unsaved-draft and pending-write guards. The
roving single-tab-stop model and supplemental timeline remain intact.

`src/gantt/ExplanationPanel.tsx` renders below the treegrid in the schedule
view (`App.tsx`). It is a non-modal, hairline-framed region following the
DESIGN.md panel language: a Condensed uppercase panel title
(`Schedule explanation`) with the focused task name, over deterministic
FACT rows in Barlow 13px `--color-text` — the DESIGN.md §6 "deterministic risk
flags" treatment, never the indented model-prose treatment reserved for the AI
layer. Each fact is the scheduler's own typed data rendered verbatim (no
schedule math): a `summary` shows `Derived from children`; a `complete` leaf
shows `Complete · actual <start> – <finish>`; a `scheduled` leaf shows a
`Driver` line, any `Also` co-binding drivers, an optional `Started <date>`, an
optional singular/plural non-working-day gap, a `Float <signed> days` line
(`· critical` appended when critical), and the `lateFinishLimit` fact. A
predecessor driver reads `After <link>` for the start-anchored FS/SS types and
`Finish after <link>` for the finish-anchored FF/SF types; constraint and
deadline facts append ` · applied <date>` when normalization moved the entered
date. Typed links reuse the Predecessors-cell format (`B FS +1 day`, lag
dropped at zero). The workspace resolves relationship IDs to task names;
unknown IDs and standalone verification retain the identity as a fallback.
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
