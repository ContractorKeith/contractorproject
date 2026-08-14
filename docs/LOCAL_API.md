# Local agent API

Status: proposed v1 contract
Updated: 2026-08-14

## Interface

Ship an MCP helper with the desktop application and use stdio as the v1 transport. The agent client launches the helper; ContractorProject does not open a network listener for normal single-user use.

The MCP adapter calls the same Rust application interface as the desktop UI. It never opens SQLite directly and cannot bypass validation, record-version checks, or audit logging.

## Initial tools

### Read

- `list_jobs(status?, limit?, cursor?)`
- `get_job(jobId, include?)`
- `get_schedule(jobId, window?, includeBaseline?)`
- `list_resources(jobId)`
- `get_job_risks(jobId, dataDate?)`

### Propose

- `propose_work_breakdown(jobId, objective, constraints?)`
- `propose_schedule_change(jobId, request, expectedVersions)`
- `explain_schedule(jobId, taskIds?)`
- `explain_variance(jobId, baselineId?)`

Proposal tools return a typed diff, warnings, affected versions, and an opaque proposal ID. They do not mutate job data.

### Write

- `apply_proposal(proposalId, expectedVersions)`
- `create_task(jobId, task, expectedJobVersion)`
- `update_task(taskId, patch, expectedVersion)`
- `add_dependency(jobId, dependency, expectedJobVersion)`
- `record_actual_cost(jobId, taskId, costCodeId, amount, expectedVersion)`

Write tools are available only in read-write mode. The default agent onboarding experience should make the selected mode visible and reversible.

## Error contract

Return stable machine-readable error kinds:

- `not_found`
- `invalid_input`
- `validation_failed`
- `dependency_cycle`
- `version_conflict`
- `read_only`
- `proposal_expired`
- `provider_unavailable`

Validation failures include field paths and safe remediation details. Version conflicts return the current version and require an intentional refresh; they never silently overwrite newer work.

## Context and privacy

- Read tools return bounded projections selected by job and requested fields.
- Agent responses omit attachment bodies and provider credentials.
- AI provider calls are separate from MCP access. Local MCP reads do not imply permission to send job data to a model provider.
- Each mutation records actor, client name, command ID, timestamp, and a concise non-secret summary.
- Tool results use cursor pagination and explicit size limits.

## Future local network mode

If LAN/team use ships later, add an opt-in authenticated Streamable HTTP adapter around the same application interface. It must bind to an explicitly selected interface, validate request origin, use per-device credentials, and be disabled by default. Do not make HTTP a prerequisite for the desktop app or stdio helper.

## Versioning

- The helper reports product and API versions during initialization.
- Tool input schemas are additive within a major version.
- Breaking changes require a new major API version and a migration guide.
- Export archive versions and MCP API versions are independent.
