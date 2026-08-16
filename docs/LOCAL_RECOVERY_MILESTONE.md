# Local recovery milestone

Status: ready for issue creation
Updated: 2026-08-16

This milestone follows the persisted scheduled-job tracer. It proves local
recovery in three sequential slices without starting baselines or the later
portable job archive/import format.

## 1. Recoverably archive and restore a local job

Depends on issue #21.

Acceptance:

- Add `archived` to the executable job status; archive is a reversible status
  transition, never a delete.
- Route `archive_job` and `restore_job` through `ApplicationService` with typed
  command context, optimistic job versioning, and one atomic audit record.
- Hide archived jobs from the normal list and provide an accessible archived
  view with Restore.
- Preserve tasks, hierarchy, durations, dependencies, schedule inputs, and
  audit history unchanged.
- Prove archive, close, reopen, restore, and an identical schedule projection.
- Reject unknown IDs, stale versions, repeated archive, and invalid restore
  without changing canonical rows, versions, or `command_log`.
- Prove forced audit failure rolls the status/version change back.

For this slice, restore always returns the job to `draft`, the only executable
non-archived status. Preserve a prior status only after another normal status
ships.

## 2. Create a verified online SQLite backup

Depends on the archive/recovery slice.

Acceptance:

- Back up the whole application database to a validated, user-selected,
  non-existing destination using rusqlite's online backup API. Do not copy the
  database file or use `VACUUM INTO`.
- Return bounded metadata: destination, UTC timestamp, byte size, and
  verification result.
- Verify the completed snapshot read-only with `PRAGMA integrity_check`,
  `PRAGMA foreign_key_check`, required schema version/tables, and a bounded
  domain read. Verification must not migrate the backup.
- Never overwrite an existing file. On failure, leave the active database
  unchanged and remove only a newly created incomplete target.
- Use a native Save dialog and show clear cancel, success, and failure states.
- Prove a populated WAL-mode database containing active and archived jobs,
  nested tasks, schedule inputs, dependencies, and audit rows has no partial
  state in the backup.

The recommended UI suggests a dated filename and lets the user choose the
destination. It does not schedule or prune backups. Backups contain plaintext
job data; never log their contents.

## 3. Verify restore into a clean app database

Depends on the verified-backup slice.

Acceptance:

- Restore a verified backup into an isolated fresh app-data directory, then
  open it through `ApplicationService`.
- Re-run integrity and foreign-key checks and prove jobs, statuses, versions,
  hierarchy, dependencies, schedule inputs, audit rows, and the issue #21
  schedule projection match the backup snapshot.
- Mutate the source after backup creation and prove the restored target remains
  at the backup point in time.
- Reject corrupt, truncated, and foreign-schema inputs without activating a
  target or leaking SQL/customer data in errors.
- Keep in-place replacement of the running app database out of scope. That
  later UX requires restart, rollback-copy, and explicit confirmation design.

## Expected boundaries

Core work stays in `domain.rs`, `application.rs`, `storage.rs`, `error.rs`, and
the Tauri command layer, with focused `job_workflow.rs` coverage. UI work stays
in the typed job client, job types, normal React workflow, and focused tests. A
native destination chooser may add the official Tauri dialog plugin and its
minimum capability permission.

Do not modify the pure scheduler, Gantt builder, verification fixtures, or
performance harnesses.

## Exclusions

- baselines, variance, or baseline rendering;
- portable per-job ZIP import/export, CSV, attachments, or manifests;
- purge/delete, automatic schedules, retention, cloud sync, or encryption;
- MCP/agent restore, cross-machine transfer, or schema rollback engines;
- schedule semantics, crews, resources, or costs.
