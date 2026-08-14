# Architecture and stack recommendation

Status: recommended planning baseline
Updated: 2026-08-14

## Recommendation

Build a Tauri 2 desktop app with a React and TypeScript frontend, a Rust application core, and SQLite persistence owned entirely by Rust.

This is the smallest credible architecture that supports native macOS and Windows packaging, a polished schedule-heavy UI, deterministic domain logic, local agent access, and future standalone suite modules without putting business rules in a webview or creating a server-first product.

## Stack

| Area | Choice | Reason |
| --- | --- | --- |
| Desktop shell | Tauri 2 | Cross-platform native packaging with a small runtime and a Rust host process. |
| UI | React, TypeScript, Vite | Fast iteration and a strong ecosystem for complex tables, forms, and timeline interactions. |
| Styling | CSS variables and a small local component layer | Keeps the native visual language under project control without adopting a large design system. |
| Native/application core | Rust | One deterministic implementation for scheduling, validation, persistence, backups, agents, and desktop commands. |
| Database | SQLite through `rusqlite` | Embedded, transactional, portable, and directly controlled by the application core. |
| IDs and money | UUIDv7-style opaque IDs; integer minor currency units | Portable suite identity and exact arithmetic. |
| Agent interface | MCP over stdio | No listener, port, or cloud dependency; tools call the same application services as the UI. |
| AI integration | Provider adapters plus a local OpenAI-compatible adapter | BYOK and local models share one narrow interface while credentials stay outside job data. |
| Testing | Rust unit/integration tests, Vitest, React Testing Library, Playwright smoke tests | Verifies calculations at the core and only uses UI tests for observable workflows. |
| CI/releases | GitHub Actions on macOS and Windows | Builds and tests on the operating system that produces each signed package. |

Do not give the frontend direct SQLite access. Tauri commands and MCP tools should translate inputs into the same Rust application requests, so validation, authorization, transactions, audit data, and error behavior stay local to one deep module.

The implemented pure-Rust FS calculation contract is documented in
[`SCHEDULING.md`](SCHEDULING.md). SQLite and UI adapters consume its inputs and
outputs; neither owns schedule truth.

## Shape

```text
React UI ───── Tauri commands ─┐
                              ├─ Application interface ─ Domain modules
MCP helper ─── tool adapter ───┘             │
                                              ├─ SQLite repository
                                              ├─ backup/export adapter
                                              └─ AI provider adapters
```

### Domain modules

- `jobs`: job identity, status, location label, timezone, and settings
- `work_breakdown`: task hierarchy, ordering, milestones, and notes
- `scheduling`: calendars, dependencies, calculated dates, float, and critical path
- `resources`: crews/resources and task assignments
- `costs`: cost codes and simple planned/actual values
- `baselines`: immutable schedule/cost snapshots and variance projection
- `application`: use cases and transactions exposed to UI and agent adapters

The scheduling module should be a pure calculation seam. Given tasks, dependencies, calendars, and a data date, it returns a validated schedule projection. It must not know about React, SQLite, Tauri, or AI.

### UI slices

- Job list and job settings
- Work breakdown table
- Gantt timeline synchronized with the work breakdown table
- Crew and cost side panels
- Baseline/variance view
- Assistant panel with previewable proposals and explainable risk flags

Keep state local to each slice. Server-state libraries are unnecessary while the Rust core is in-process; a small command/query client and focused React state are enough.

## Gantt strategy

Treat the Gantt view as the highest-risk product spike. Before selecting a dependency, prove these behaviors with realistic data:

- 1,000 visible tasks remain responsive while scrolling and zooming.
- Table and timeline rows stay vertically synchronized.
- Hierarchy collapse, dependency lines, drag-to-reschedule, milestones, baselines, and variance are readable.
- Keyboard navigation and screen-reader labels cover the work breakdown table even if the timeline is primarily visual.
- The dependency license is compatible with the chosen project license and optional future commercial distribution.

Start with a virtualized semantic table plus an SVG or canvas timeline layer. Do not make a third-party scheduler the owner of the domain model; at most it is a rendering adapter.

## Local-first data rules

- Each installation has one application database in the OS application-data directory.
- Users can create a consistent online backup while the app is open and a portable versioned job archive for transfer.
- All schema changes are forward migrations with a pre-migration backup.
- Schedule dates are stored as job-local date/time values with an explicit job timezone; audit timestamps use UTC.
- Attachments, if included, are referenced files managed in a job assets directory rather than database blobs.
- Core workflows never require network access.

SQLite's online backup API or `VACUUM INTO` should produce consistent snapshots; copying a live database file is not the user-facing backup mechanism.

## AI rules

- AI reads a bounded application projection, not the database file.
- Model output is a typed proposal or explanation, never executable SQL or an implicit write.
- Deterministic validators recalculate dates, dependencies, costs, and permissions before a proposal can be applied.
- Applying a proposal is a normal application command with a user-visible diff and one undoable transaction.
- Risk detection starts with deterministic rules such as negative float, missing predecessors, overallocated crews, baseline slippage, and cost variance. The model explains and prioritizes those facts.
- Provider calls are explicit. The UI shows which provider will receive which job context.
- API keys live in OS credential storage. Local model endpoints require no cloud key.

## Suite seam

For v1, suite readiness means:

- stable opaque IDs
- explicit entity types and timestamps
- versioned export envelopes
- idempotent application commands where practical
- documented ownership of fields

It does not mean a shared database, shared runtime, event bus, account system, or cloud synchronization service. Future modules connect through versioned local interfaces or exports.

## Packaging and distribution

The first supported release matrix should be macOS Apple Silicon and Windows x64. Add Intel macOS only after demand is demonstrated.

Release proof stays separated:

1. source build and automated tests
2. exact release commit and version
3. platform package creation
4. Apple Developer ID signing/notarization or Windows code signing
5. installed-app owner acceptance
6. publication
7. independent public-download verification

Do not enable automatic updates until both platforms have a repeatable signed-release pipeline and rollback guidance.

## Primary references

- [Tauri 2 overview](https://v2.tauri.app/start/)
- [Tauri official plugins](https://v2.tauri.app/plugin/)
- [SQLite online backup API](https://www.sqlite.org/backup.html)
- [MCP transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports)
