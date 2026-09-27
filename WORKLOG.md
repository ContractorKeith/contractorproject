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
