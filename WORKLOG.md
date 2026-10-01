# Worklog

### 2026-09-27 America/New_York - Developer pilot planning and implementation

- Outcome: Created tracker #72 and tasks #73 recovery, #74 import retries, #75 packaged acceptance. Added an isolated Pilot build profile. Fixed detailed date-cell spacing after the 760px browser assertion reported 3% spare width; it now measures 6.2%. Verified 97 Vitest tests, all 32 browser tests, TypeScript and frontend build. Rust implementation and packaged recovery acceptance remain in progress.
- Decision: Keith operates one local Mac pilot with Google Sheets parallel records. Separate pilot identifiers protect normal app data. Public release and client/accountant acceptance remain separate.
- Execution: GPT-6 Luna medium handles bounded tasks without nested delegation; primary reviews full diffs and owns integration, installed acceptance and shipping.
- Next: finish regression gates, package exact candidates, perform native recovery drills, record evidence and merge/push each scoped branch.


## 2026-09-27 — Native recovery and export correction

The installed Pilot passed edit/restart/backup/fresh-directory restore, including
actuals, baseline and calculated dates. Corrupt restore preserved the current data.
Native export revealed a WebKit download hang (#76); the fix uses an async Save
dialog and a validated new-file writer. Luna's final gates passed: 98 UI, 32 browser,
265 Rust tests (one existing ignored), typecheck/build/fmt/Clippy. Primary reviewed
the complete implementation and reran both export component tests after the help
copy change. Real CRM importer retry/re-export E2E passed. Next: clean installed
export acceptance and merge.

## 2026-09-27 — Pinned native Project acceptance

Installed clean source 672b645291878c617490636ba663f20d3747a9b4 with both operators and verified
the complete ad-hoc signature. Restored schedule facts remained correct. Native
CSV/HTML saved successfully and matched the fixture; HTML rendering was readable.
Cancel wrote nothing; an existing filename returned a visible error and preserved
its exact hash. Synthetic data/snapshots/exports retained separately in Downloads.
Artifact hash in ACCEPTANCE.md. PR #77 pushed; CI and merge remain.

## 2026-09-27 — Pilot preparation complete

PR #77 merged as 193001c209107fa392a44eb52e6e2564b83c974a after all PR checks passed.
Original main fast-forwarded; complete branch reviewed. Retained ZIP independently
extracted/signature-checked and matched to installed runtime. Fresh Project Pilot opens with zero jobs. Retained snapshots passed standalone integrity and foreign-key checks. The installed packaged importer passed cross-app retry/re-export acceptance.
Normal app data and unrelated checkout files preserved.
Next: Keith enters the remaining-work schedule and takeover forecast baseline with Sheets in parallel.

### 2026-10-01 America/New_York - Compact pilot jobs page

- Outcome: Implemented #78, #79, and #80 with Luna 6 workers and primary integration review. Removed the jobs hero, added a persistent light/dark moon toggle and gear button, and moved job creation, local storage status, and verified backups into native Settings. Creation respects pending saves and unsaved task drafts.
- Review: Corrected dark-mode tokens, heading order, icon alignment, and legacy creation tests. Native dialog tests allow browser chrome focus while checking that the background page remains inert.
- Checks: TypeScript, 102 unit tests, all 36 browser tests, production build, Rust fmt, Clippy and all Rust tests passed. Desktop/minimum-window layout and jobs-page accessibility passed. Canonical docs merged in opencontractoros PR #9.
- Next: Build and verify the updated installed Pilot, then merge the app PR and remove the branch.
