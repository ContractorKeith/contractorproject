# Developer pilot acceptance

Status: engineering acceptance complete; PR #77 merged with all checks green.
Date: 2026-09-27, America/New_York.

## Automated evidence

Primary verified TypeScript, the production frontend build and
all 32 browser tests. The 760px detailed-date layout initially failed its spare
width assertion; the cell-padding correction passes with 6.2% spare width.

After schema-v10 assertion corrections and the reviewed recovery cleanup fix,
the full Rust all-target test run passed: 261 passing, zero failing, one existing
ignored performance test across 13 test binaries. This includes the injected
post-publication sync failure, occupied-target preservation, corrupt-backup
rejection, retry/restart/restore identity and atomic audit failure regressions.
After the native export fix, Luna reran all gates: 98 Vitest tests, 32 browser
tests, 265 passing Rust tests with one existing ignored performance test, Clippy
with warnings denied, formatting, typecheck and production build. Primary reran
the two export component tests after the final help sentence; both passed.

The populated recovery fixture contains three jobs, three tasks, a dependency,
actual progress, a named Recovery baseline and a CRM source mapping. Automated
restore compares the complete schedule projection and verifies the imported
source identity still resolves to the same job.

## Review and pending native evidence

Primary reviewed storage migration, importer, recovery CLI and tests. Independent
Luna review found a post-link sync-failure cleanup edge; the fix removes only a
target proven to be the staged regular file's own hard link. Contender files and
symlinks retain their protections.

## Installed recovery drill

Installed isolated Pilot with AI unavailable. Restored the populated fixture with
the bundled recovery operator into fresh app data, then reopened the native app.
Verified three jobs, three tasks, the dependency, 50% progress, actual start August
17 at 08:00, Recovery baseline and milestone forecast August 18 at 12:30.

The UI created a verified snapshot. Increasing Activity duration from one to two
working days shifted the milestone to August 19 at 08:30 and showed baseline
variance. The edit survived restart. With the app quit, corrupt input was rejected
without creating a target or changing the existing database. Retained the entire
changed data directory, restored the UI snapshot into a fresh default directory,
and reopened: duration returned to 480 minutes, baseline variance to zero and the
milestone to August 18 at 12:30; actual progress remained intact.

The real cross-process CRM handoff test passed: repeat and re-export returned the
same Project job identity. Native CSV export exposed a WebKit download hang (#76).
The reviewed fix uses the async system Save dialog and a bounded new-file writer;
existing files and active database paths are protected.

## Pinned native export acceptance

Source: `672b645291878c617490636ba663f20d3747a9b4`, clean worktree at build.
Version 0.1.0; macOS 26.6.2; aarch64-apple-darwin.
Archive: `ContractorProject-Pilot-672b64529187.zip`.
SHA-256: `d9e9756407dc463403babb91eef01d06bd7e30cbd31952e08bab31fea9438e6a`.

Installed this exact build and verified its complete ad-hoc signature, including
both bundled operators. Reopened the restored schedule and checked duration,
actual progress, baseline variance and forecast again. Native CSV and HTML exports
opened Save dialogs and created readable files with all three rows, dependency,
progress and baseline comparisons. The HTML rendered without clipped content in
Safari through a temporary local-only preview server (Safari refused a /tmp file
URL under its own sandbox). The preview server was stopped afterward.

Canceling a named export created no file. Choosing an existing synthetic export,
even accepting the system replace prompt, returned a visible File exists error;
the original SHA-256 stayed unchanged and the app remained responsive.

Retained synthetic profile, snapshot and exports separately in Downloads before
initializing a fresh working Pilot profile. Later evidence-only commits do not
change this runtime. Integration is tracked in PR #77 and issue #72.

This is single-operator schedule testing. Crew and cost planning, cloud sharing,
client-data acceptance and public release remain outside this signoff.

## Integration and handoff

PR #77 merged as `193001c209107fa392a44eb52e6e2564b83c974a`. All final PR checks passed. Original main was fast-forwarded while preserving unrelated files. Independently extracted the retained ZIP, verified its signature and matched the installed executable hash. Fresh Project Pilot opens with zero jobs. Retained snapshots passed standalone integrity and foreign-key checks. The installed packaged importer passed cross-app retry/re-export acceptance.

## 2026-10-01 jobs UI update

Source: `d22529f0306c7ad6b09bb3780e6de274d2115937`, clean at build.
Archive: `ContractorProject-Pilot-d22529f0306c.zip` in `pilot-artifacts/`.
SHA-256: `d624a26c8c04e7e4066bfda56a86c2292c5542d2e6a75ba072b0c6830a754911`.
Version 0.1.0, macOS 26.6.2, aarch64-apple-darwin, ad-hoc signed local Pilot.

Installed in `~/Applications/ContractorProject Pilot.app`. Deep strict signature
verification passed and the installed executable hash matches the staged build.
The prior installed app bundle is retained in
`pilot-artifacts/installed-before-ui-20261001.app`. The Pilot data directory was
not replaced or edited during installation; the existing job remains visible.

Native UI inspection confirmed the compact local job list, moon/gear controls,
both theme directions, remembered dark appearance after restart, and Settings
with job creation and verified backups. The backup action opened the system Save
dialog. Cancel returned visible feedback and focus to the re-enabled backup
button; Escape closed Settings. No test jobs were added to the working profile.
Creation, draft protection, dialog keyboard behavior and responsive layout passed
in the synthetic browser suite. All 102 unit tests, 36 browser tests, TypeScript,
production build and local Rust gates passed. Canonical docs are in
opencontractoros PR #9; app integration is PR #81.

## 2026-10-01 schedule setup and days update

Source: `c3be2561d1c18ea25a39eec63d80bc9d811e837c`, clean at build.
Archive: `ContractorProject-Pilot-c3be2561d1c1.zip` in `pilot-artifacts/`.
SHA-256: `8bd0e469878e2b7349679c1494267c2ad90e3385b7416c2a6b3ac2e1122c3257`.
Version 0.1.0, macOS 26.6.2, aarch64-apple-darwin, ad-hoc signed local Pilot.

Installed in `~/Applications/ContractorProject Pilot.app`. Deep strict signature
verification passed; the installed executable hash matches the staged bundle.
Retained archive checksum matches its metadata. The previous installed bundle
is retained in `pilot-artifacts/installed-before-day-planning-20261001.app`.
The Pilot data directory was left in place; the existing job remains visible
and no synthetic tasks, schedule settings or progress records were saved there.

Native inspection confirmed readable setup guidance in dark and light themes,
start/data-date and weekday controls without workday clock/minute fields, and
Settings with creation and Backups but no SQLite status. The backup Save dialog
opens; Cancel returns feedback and focus to its button, and Escape closes
Settings. Dark appearance and the existing job survive a fresh restart.

Day inputs, custom saved calendars, signed dependency lag, precise CSV numbers,
Gantt/report dates and day-based variance were verified using synthetic tests.
All 106 unit tests, 37 browser tests, TypeScript/build, Rust fmt/Clippy and
265 Rust tests (one existing ignored) pass. Setup contrast exceeds 4.5:1 and
axe checks pass in both themes; baseline facts fit the 760px window.
Canonical docs merged in opencontractoros PR #10. Integration is PR #86,
closing #82–#85. This evidence entry changes no runtime code.
