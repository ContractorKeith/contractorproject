# Hand-off with ContractorBooks

Status: consumed by ContractorBooks; CP adoption pending milestone
**4. Crews and simple job costs**
Updated: 2026-08-19

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

## What ContractorProject can consume from ContractorBooks

| envelope | shape | status |
| --- | --- | --- |
| Job financial status v1 | `{ envelopeVersion, jobRef, jobId, jobName, asOf, budgetTotalMinor, costTotalMinor, billedTotalMinor, receivedMinor, retainageHeldMinor, openArMinor }` | shipped in Books as the `job_financial_status` command and MCP read tool |

Books owns every field; a consumer treats it as read-only. `jobRef` echoes the job
reference above (`{ tool, projectId, label }`), so a CP job can find its own money by
matching `jobRef.projectId` to its job id.

## Current state (2026-08-19)

ContractorProject **produces none of these envelopes yet**: it has no cost codes and no
budgets, so there is nothing here to export into them. Books' side is built and
tested, and a real CP job can already be handed over by passing its job id and name
into Books' `link_project_job` and keying a budget envelope on that job as
`externalRef`.

Adoption on this side belongs to milestone **4. Crews and simple job costs**, where
hierarchical job cost codes and planned/actual costs land (`docs/MVP_PLAN.md`); the
agent-facing surface that would publish them belongs to milestone
**5. Local AI and agent interface**. Nothing here blocks Books' v1.
