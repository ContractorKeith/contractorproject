# Hand-off with ContractorBooks

Status: consumed by ContractorBooks; CP adoption pending milestone
**4. Crews and simple job costs**
Updated: 2026-09-27

ContractorProject and ContractorBooks share no database, no event bus, and no
platform code. The whole interface is a set of small, independently versioned JSON
envelopes. **`../contractorbooks/docs/HANDOFF.md` is canonical** — schemas, field
ownership, and error semantics all live there. This page records what concerns this
repository.

## What ContractorBooks consumes from ContractorProject

| envelope | shape | who owns what |
| --- | --- | --- |
| Job reference v1 | `{ tool, id, label? }` — Books stamps `linkedAt` | CP owns `tool`, `id`, `label`; Books owns the stub's lifecycle |
| Budget v1 | `{ envelopeVersion: 1, externalRef, lines: [{ costCode, budgetMinor }] }` | CP owns `externalRef` and the line amounts; Books owns cost-code resolution and idempotency on `(jobId, externalRef)` |

Notes that matter on this side:

- **Cost codes match by exact `code` string.** A code Books does not know rejects the
  **whole** envelope — no partial import. Whatever produces a budget here has to emit
  codes that exist in the user's Books cost-code list.
- `externalRef` is the idempotency key. Re-sending the same `(job, externalRef)` is a
  no-op in Books; a revised budget needs a new `externalRef`.
- Money is always integer minor units. No floats.

## What ContractorProject plans to consume from ContractorBooks

| envelope | shape | status |
| --- | --- | --- |
| Job financial status v1 | `{ envelopeVersion, jobRef, jobId, jobName, asOf, budgetTotalMinor, costTotalMinor, billedTotalMinor, receivedMinor, retainageHeldMinor, openArMinor }` | shipped in Books as the `job_financial_status` command and MCP read tool; Project consumer planned |

Books owns every field; a future consumer will treat it as read-only. `jobRef` echoes
the job reference above (`{ tool, projectId, label }`), so a Project job can find its
own money by matching `jobRef.projectId` to its job id.

## Current state

ContractorProject **produces none of these envelopes yet**: it has no cost codes and no
budgets, so there is nothing here to export into them. Books' side is built and
tested, and a real Project job can already be handed over by passing its job id and
name into Books' `link_project_job` and keying a budget envelope on that job as
`externalRef`.

Adoption on this side belongs to milestone **4. Crews and simple job costs**, where
hierarchical job cost codes and planned/actual costs land (`docs/MVP_PLAN.md`); the
agent-facing surface that would publish them belongs to milestone
**5. Local AI and agent interface**. Nothing here blocks Books' v1.

## ContractorCRM opportunity hand-off

The standalone `handoff-import` operator reads a versioned JSON envelope from a
file and creates a draft job through `ApplicationService`. It never reads CRM's
database. The stable identity is `opportunity.id` under source system
`ContractorCRM`; `exportedAt` and fields Project does not import do not affect retry
matching. The imported content today is the trimmed `opportunity.name`.

Importing the same opportunity again, including after Project restarts, returns the
same job without adding an audit row. A repeated identity with a changed imported
name fails with `source_identity_conflict`; Project does not rename or replace the
existing job. A different opportunity id creates another job. Source mapping, job,
and audit row are written atomically. When the operator points the importer at an
existing database, it first validates a complete supported Project schema v4-v10
through a read-only connection; a matching migration number alone is insufficient.

## Fresh-directory recovery verification

Build and run `contractorproject-recovery` with explicit backup and target paths:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin contractorproject-recovery -- \
  --backup /path/to/contractorproject.backup.sqlite3 \
  --target-app-data-dir /path/to/new-project-app-data
```

The target directory must not exist. The command restores and verifies the backup
through `ApplicationService`, opens the restored database, and prints bounded JSON
counts on success. Errors do not include SQL rows or customer data. It uses a new
temporary coordinator database and never opens or changes the active app database.
This operation verifies recovery into a separate directory; it does not replace the
running app's database.
