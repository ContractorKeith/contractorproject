# MVP delivery plan

Status: proposed
Updated: 2026-08-14

The work is sequenced as tracer slices. Each slice leaves a usable path through the real desktop app and keeps scheduling, persistence, UI, and agent interfaces aligned.

## Release boundary

v1 is complete when a user can plan and track one job at a time with a hierarchical schedule, critical path, baseline variance, crew assignments, simple costs, backup/export, AI-assisted risk review, and a documented local agent interface in signed macOS and Windows packages.

Portfolio dashboards, automatic resource leveling, cloud sync, LAN mode, mobile, and the rest of the business suite do not block v1.

## 0. Prove the foundation

- [x] Record the stack decision after a Tauri 2 hello-world builds on macOS and Windows CI.
- [x] Build a Gantt interaction spike with 1,000 tasks, hierarchy collapse, synchronized scrolling, zoom, baseline bars, dependency lines, and keyboard navigation.
- [x] Audit the Gantt approach and all proposed runtime dependencies against the candidate project licenses.
- [ ] Define supported OS versions and the first signing strategy.
- [ ] Turn the data model and local API drafts into versioned schemas.

Exit: the team can build, test, and package the shell on both platforms, and the Gantt approach has measured interaction and license evidence.

## 1. First durable job

- [x] Scaffold Tauri, React, Rust workspace structure, linting, tests, and CI.
- [x] Add SQLite initialization, forward migrations, and application-data path handling.
- [ ] Implement create/list/open/archive job commands.
- [x] Implement task hierarchy create/edit/reorder with record-version conflicts.
- [ ] Render the job list and work breakdown table through the real application interface.
- [ ] Add a consistent backup command and restore verification test.

Exit: a packaged development app can create a job and nested tasks, restart without data loss, archive a job recoverably, and restore a verified backup.

## 2. Deterministic scheduling

- [ ] Specify duration, constraints, lag, calendars, data date, and summary-task rules with examples.
- [ ] Implement cycle-safe FS dependencies, forward/backward pass, total float, and critical path.
- [ ] Add SS, FF, and SF dependency types plus positive and negative lag.
- [ ] Add default working calendar and dated exceptions.
- [ ] Expose schedule calculation through application queries with fixture-based tests.
- [ ] Show validation failures and schedule explanations in the work breakdown table.

Exit: known schedule fixtures reproduce expected dates, float, and critical tasks; invalid graphs cannot be committed.

## 3. Gantt and baselines

- [ ] Build the production work-breakdown/timeline split view from the accepted spike.
- [ ] Add zoom, pan, today/data-date markers, hierarchy collapse, and dependency rendering.
- [ ] Add constrained drag-to-reschedule with a preview before commit.
- [ ] Create immutable named baselines.
- [ ] Display baseline bars and start/finish/duration variance.
- [ ] Add exportable schedule and variance views.

Exit: a user can create a realistic schedule, set a baseline, change the plan, and understand the variance without inspecting raw data.

## 4. Crews and simple job costs

- [ ] Add crew/resource records and task assignments.
- [ ] Flag over-allocation without automatic leveling.
- [ ] Add hierarchical job cost codes.
- [ ] Record planned and actual task costs in integer minor units.
- [ ] Show job and cost-code planned-versus-actual totals.
- [ ] Include assignments and costs in baseline/archive/export behavior where defined.

Exit: a user can see who is assigned and whether schedule or cost is drifting; the app still avoids estimating and accounting workflows.

## 5. Local AI and agent interface

- [ ] Implement the provider interface, OS credential storage, and a local OpenAI-compatible endpoint adapter.
- [ ] Add one BYOK cloud provider adapter without making it the domain interface.
- [ ] Implement deterministic schedule/cost/resource risk rules.
- [ ] Add assistant explanations and typed work-breakdown/schedule proposals.
- [ ] Add proposal diffs, validation, explicit apply, undo, and audit records.
- [ ] Ship the MCP stdio helper with read-only/read-write onboarding.
- [ ] Document and test every MCP tool, size limit, error kind, and version conflict.

Exit: the app remains fully useful with AI disabled; with AI enabled, no model output mutates data without validated user acceptance.

## 6. Interchange and hardening

- [ ] Finalize versioned job archive export/import with path and checksum validation.
- [ ] Add CSV export and a bounded CSV task import with mapping preview.
- [ ] Add crash recovery, migration rollback, and corrupted-database guidance.
- [ ] Verify keyboard navigation, focus behavior, contrast, and screen-reader labels.
- [ ] Test large jobs and define supported task/attachment limits.
- [ ] Complete threat modeling for attachments, imports, local model endpoints, MCP, and provider context.

Exit: users can safely back up, transfer, recover, and inspect their data without vendor infrastructure.

## 7. Release

- [ ] Freeze version and release notes at one exact commit.
- [ ] Pass full macOS and Windows build/test/package matrices.
- [ ] Sign and notarize the macOS package; sign the Windows installer.
- [ ] Run installed-app acceptance on clean user accounts on both platforms.
- [ ] Publish artifacts with checksums and license notices.
- [ ] Independently download, verify, install, launch, and run the core job workflow from public artifacts.

Exit: source checks, commit, package, signing, installed acceptance, publication, and public-download verification are independently recorded as passing.

## First implementation issue

Build the thinnest end-to-end slice: create a Tauri window, create one job through a Rust application command, persist it in SQLite, list it in React, restart, and prove it remains. Do not start the full Gantt implementation until that vertical path is green on macOS and Windows CI.

Status: complete. The durable create/list path passed local restart verification and the macOS/Windows gates in GitHub Actions run [31816538579](https://github.com/ContractorKeith/contractorproject/actions/runs/31816538579).
