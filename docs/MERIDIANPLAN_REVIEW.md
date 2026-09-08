# MeridianPlan comparison and contractor-first milestone

Reviewed 2026-09-08. ContractorProject baseline: `3a2c7d9`.
Reference: [MeridianPlan source at `3704572`](https://github.com/wieslawsoltes/MeridianPlan/tree/3704572d1d193f431eb652c660ce5e85486813e6)
and its [live application](https://wieslawsoltes.github.io/MeridianPlan/).
Delivery is tracked in [milestone 1](https://github.com/ContractorKeith/contractorproject/milestone/1)
and [issue #66](https://github.com/ContractorKeith/contractorproject/issues/66).

## Recommendation

Keep ContractorProject's scheduling engine and local data ownership. Change the
default experience from a collection of forms to a schedule that answers the
contractor's immediate questions. Borrow Meridian's integrated work area,
search, compact status facts and ordinary exports. Avoid its enterprise
navigation and breadth of configuration.

The reference's model authorship was supplied in the request; this review does
not independently establish which model produced its code. Implementation and
observed behavior are the basis for the comparison.

## Evidence and limits

The live app was inspected in an isolated Chromium session at 1440 × 1000.
It opened a populated Northline Transit Hub schedule with 31 activities, a finish
date, critical count, progress and resource alerts above the table/Gantt, and a
selected-task inspector below it. Searching for `equipment` left three matching
activities with their WBS groups visible. The export action opened a dialog
offering workbook/project JSON, activity/resource CSV and a printable HTML
report. These are direct observations. Broader scheduling, editing and storage
capabilities below were checked in source and tests; this audit does not certify
every edge case or native accessibility of the reference app.

ContractorProject was inspected in source and through the production `App` with
an explicitly in-memory JobClient browser fixture. The baseline fixture has
26 scheduled rows: it rendered 27 `.task-editor` elements, a 6,644-pixel page,
and a table starting at y=875 on the same 1440 × 1000 viewport. Browser fixture
dates are canned read-model facts; engine correctness and persistence are
covered by the Rust tests, not this UI fixture.

## Comparison at the baseline

| Area | MeridianPlan | ContractorProject | Decision |
| --- | --- | --- | --- |
| Main job workflow | Table, Gantt, metrics and inspector in one workspace | Job disclosure opens setup forms, then Gantt, then another complete task-editor tree | Put the schedule first; one selected editor |
| Immediate answers | Finish, progress, critical work, resource overloads | Task-level facts exist; no job summary | Show forecast finish, next unfinished work, completion and attention without leaving the schedule |
| Finding work | Name/code search, scope/status/critical filters | Hierarchy collapse and keyboard navigation; no search/filter | Add name/WBS search and a small set of useful filters |
| Task editing | Inline edits and one inspector; durations in days | Name, raw minutes, actuals, constraints and hierarchy controls repeated per task | Use working days/hours/minutes and disclose advanced controls |
| Direct Gantt edits | Move/resize/link gestures mutate scheduling inputs | Pan and zoom exist; schedule editing occurs through forms | Defer gestures until a Rust-backed preview/acceptance workflow is designed |
| Scheduling | Calendar-aware CPM, four dependency types, signed lag, constraints, actuals and traces | Deterministic Rust CPM with the same major classes of scheduling inputs and typed explanation facts | Preserve the engine; this is not the main usability gap |
| Calendars | Multiple calendars, split shifts, working overrides | One job calendar, dated non-working exceptions | Keep the simpler model |
| Baselines/scenarios | Immutable baselines, scenarios and calculated comparisons | Immutable named baselines and variance implemented; scenarios absent | Keep baselines; defer scenarios |
| Undo | Transactional in-memory undo/redo | Audited, version-checked writes; no general undo | Defer until persistence/actuals reversal semantics are specified |
| Crews/resources | Assignments, capacity, overloads, actual hours | Planned, not implemented | Real capability gap; separate crew milestone |
| Costs | Resource rates and time-phased cost analysis | Simple planned/actual job costs are planned | Separate cost milestone; do not add an empty screen |
| Sharing | JSON, CSV and self-contained HTML reports | Verified database backup; no ordinary schedule export | Direct CSV and printable HTML downloads |
| Storage/recovery | IndexedDB with localStorage fallback; JSON export | SQLite, optimistic version checks, audit records, verified backup | Preserve local-first ownership; keep backup distinct from reports |
| AI | No assistant workflow found in the reviewed UI/source | AI/local MCP intent documented; no user-facing assistant implemented | Do not imply that deterministic facts require AI |
| Accessibility/performance | Virtualized WebGPU with Canvas fallback; browser tests | Virtualized semantic treegrid, keyboard/focus contracts, contrast and performance tests | Preserve existing contracts and verify filters against them |

Source anchors: Meridian's
[workspace/editors/exports](https://github.com/wieslawsoltes/MeridianPlan/blob/3704572d1d193f431eb652c660ce5e85486813e6/src/app.js),
[scheduler](https://github.com/wieslawsoltes/MeridianPlan/blob/3704572d1d193f431eb652c660ce5e85486813e6/src/core/scheduler.js),
[resources](https://github.com/wieslawsoltes/MeridianPlan/blob/3704572d1d193f431eb652c660ce5e85486813e6/src/core/resources.js),
[storage/CSV](https://github.com/wieslawsoltes/MeridianPlan/blob/3704572d1d193f431eb652c660ce5e85486813e6/src/core/storage.js),
and [browser smoke tests](https://github.com/wieslawsoltes/MeridianPlan/blob/3704572d1d193f431eb652c660ce5e85486813e6/tests/browser_smoke.py).
Local anchors: `src/App.tsx`, `src/gantt/`, `src/api/jobs.ts`,
`src-tauri/src/application.rs`, `src-tauri/src/scheduling.rs`,
`docs/MVP_PLAN.md`, `docs/LOCAL_RECOVERY_MILESTONE.md`.

## Deliberate subtraction

- Remove the large promotional introduction while working inside a job and
  claims about crews/costs/files that the current app cannot deliver.
- Replace the duplicate all-task editor tree with one selected editor. Keep
  existing functionality available without mounting every task's full form.
- Put calendar administration, baseline management, dependencies, constraints
  and hierarchy operations behind clearly labeled disclosures.
- Keep one schedule workspace. Do not copy the portfolio rail, separate WBS
  screen, resource dashboards, report center, scenario screens or inspector tabs.
- Keep technical precision where it matters, but use working days by default
  for duration entry. Changing a display unit must not round stored minutes.

## Bounded delivery and success checks

1. [#67](https://github.com/ContractorKeith/contractorproject/issues/67): schedule-first
   workspace, direct new-job flow, one selected editor, safe draft handling and
   exact contractor-unit duration entry.
2. [#68](https://github.com/ContractorKeith/contractorproject/issues/68): summary,
   hierarchy-aware search/filter, honest progress context and keyboard behavior.
3. [#52](https://github.com/ContractorKeith/contractorproject/issues/52): progress
   percentage remains visible when status text must shrink.
4. [#69](https://github.com/ContractorKeith/contractorproject/issues/69): formula-safe
   CSV and escaped, self-contained printable HTML schedule, accessible from the
   same workspace and covering the entire job.
5. [#70](https://github.com/ContractorKeith/contractorproject/issues/70): readable
   task names in a six-column default view, with optional diagnostic columns
   and preserved keyboard focus. Added after screenshot review showed the old
   seven-percent name column still made the new workspace hard to scan.

The summary uses existing facts, never a second scheduling engine. “Needs
attention” means an unfinished leaf with a violated constraint or negative
float. Critical work is identified separately; neither criticality nor missing
actuals proves a task is late. “Next” means the earliest scheduled unfinished
task, not a claim that prerequisites or site conditions have been verified.
Data-date context explains how current the recorded progress is.

Each worktree receives tests and a parent review before a scoped commit.
Integration receives independent standards and issue-acceptance reviews, with
findings returned for fixes and re-review. Final gates cover TypeScript, unit
tests, browser behavior, build and Rust quality. Canonical user documentation
ships in `opencontractoros/src/content/docs/project/` in the same session.

This milestone intentionally excludes crew/cost models, scenarios, general
undo, destructive task deletion, drag previews, archive import/restore, new
calendar semantics and AI. Existing unrelated issues remain open; recording a
gap here does not promise it is implemented.

## Delivered workspace and review evidence

The same 26-row browser fixture now mounts two task-editor components: the
small add-task field and one selected editor. Its full page is 1,225 pixels tall
and the grid starts at y=485 at 1440 × 1000 (baseline: 27 editors, 6,644 pixels,
y=875). These are fixture measurements, not a claim about every job size.

[Before](review-evidence/workspace-before.png) ·
[After](review-evidence/workspace-after.png)

Parent and independent reviews returned fixes for progress numerals, cramped
names, lost selection after saves and when a schedule first becomes calculable,
unsaved draft handling, precise minute/unit
round trips, summary duration controls, startup guidance, duplicate schedule
fetches, accessible labels/headings/contrast and stale exports. Each accepted
fix received another review. Original command/persistence scenarios remain
covered; the full-App browser fixture adds the new contractor workflows.

TypeScript checking, the production web build, 97 Vitest tests and 32 Playwright
browser tests pass on the integrated changes. The native Tauri build without
bundling also passes. The 1,000-row performance gate passes with scroll p95
14.7 ms, p99 15.6 ms, zero row drift and zero long tasks. An initial run during
other projects' CPU-heavy builds missed the unchanged 25 ms p95 threshold;
independent review and an isolated rerun resolved that verification failure.
GitHub's wider fallback font also exposed date headroom and percent glyph
clipping at 760 pixels. The final CSS reserves more width for progress and
tightens date padding, preserving table/timeline widths and test thresholds.
[Final GitHub quality checks](https://github.com/ContractorKeith/contractorproject/actions/runs/34199789920)
pass on the reviewed code merged in [PR #71](https://github.com/ContractorKeith/contractorproject/pull/71).
Rust formatting, Clippy with
warnings denied, and all-target tests pass: 251 tests passed, one pre-existing
performance test ignored. Browser fixtures
exercise the production React UI through an in-memory application seam;
SQLite/versioning/scheduler behavior is verified separately by Rust tests.
Packaged VoiceOver/WKWebView and NVDA/WebView2 manual acceptance remains the
separate platform work in issue #7.

[Canonical user docs](https://github.com/ContractorKeith/opencontractoros/commit/1a346be)
were updated and pushed in the same session. Their Astro build produces
38 pages with valid internal links and a Pagefind index of 32 English pages.
This 16 KiB-page ARM64 host required a local, same-version glibc Pagefind binary;
no website dependency, lockfile or deployment configuration changed.
