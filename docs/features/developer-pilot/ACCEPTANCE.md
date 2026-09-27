# Developer pilot acceptance

Status: automated and installed clean candidate gates passed; merge pending.
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
