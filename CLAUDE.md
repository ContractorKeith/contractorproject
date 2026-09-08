# ContractorProject

## Runtime routing

- Claude Code main sessions read `../dotfiles/claude/ORCHESTRATION.md`.
- Other runtimes must not apply Claude's model assignments.

ContractorProject is a local-first, AI-native project and job management tool for contractors. It is the first standalone module in a planned suite of offline business applications.

## Status

v0.1 scheduled-job tracer — persisted work breakdown, schedule inputs,
SNET/FNLT constraints, audited data-date/progress statusing, immutable named
baselines, all four dependency types (FS/SS/FF/SF with signed lag), dated
non-working calendar exceptions, and deterministic schedule explanations now
flow through the pure Rust scheduler into the production Gantt (read-model v7
with calendar facts and non-working shading, typed link annotations and
geometry, progress facts, a data-date marker, timeline pan (drag + keyboard),
a shell-injected deterministic today marker, baseline ghost bars,
start/finish/duration variance, and per-task explanation facts surfaced in a
focus-following panel) in the normal desktop workflow. The scheduling-core
MVP section is complete. The contractor workspace now opens directly to a
compact schedule with finish/progress/attention facts, hierarchy-aware search,
one selected task editor with working-day/hour/minute entry, and full-job CSV
and printable HTML downloads. Advanced scheduling controls remain available
through disclosures. Last touched 2026-09-08.

## Commands

```bash
# install
npm install

# build
npm run build

# typecheck
npm run typecheck

# test the UI and Rust core
npm test
npm run test:browser
cargo test --manifest-path src-tauri/Cargo.toml --all-targets

# launch the desktop app
npm run tauri dev
```

## Planning baseline

- `docs/PRODUCT_BRIEF.md` is the product scope.
- `docs/ARCHITECTURE.md` is the recommended Tauri, React, Rust, and SQLite architecture.
- `docs/DESIGN.md` is the accepted visual language and application identity.
- `docs/DATA_MODEL.md` owns the initial domain language and persistence model.
- `docs/SCHEDULING.md` owns the implemented deterministic scheduling contract.
- `docs/GANTT_READ_MODEL.md` owns the versioned Rust-to-React schedule projection.
- `docs/GANTT_TREEGRID.md` owns the production work-breakdown interaction contract.
- `docs/LOCAL_API.md` owns the local agent interface.
- `docs/MVP_PLAN.md` is the issue-ready delivery sequence.
- `docs/MERIDIANPLAN_REVIEW.md` records the comparative audit and bounded UX milestone.
- `src/` owns the React UI and its narrow Tauri command client.
- `src-tauri/src/application.rs` is the public application seam used by UI commands and future agents.
- `src-tauri/src/storage.rs` owns SQLite access and migrations.

## Conventions & Gotchas

- Treat `job` as the contractor-facing aggregate; do not use generic project-management language when a construction term is clearer.
- Keep schedule calculations deterministic and independent from UI, storage, and AI adapters.
- Route all writes through the Rust application interface; the UI and agents never write SQLite directly.
- Preserve local-first ownership: no network dependency for core job planning and no model call without an explicit user action.
- Keep future suite integrations behind versioned interfaces; do not build a shared platform or event bus for v1.

## Out of Scope

- Full estimating, CRM, invoicing, inventory, payroll, mobile apps, real-time cloud collaboration, and heavy ERP workflows.

## Documentation

Canonical user docs for ContractorProject live in the website repo:
**`ContractorKeith/opencontractoros` → `src/content/docs/project/`** (served at
opencontractoros.com/docs/project/).

**Hard rule:** any PR or commit that changes user-facing behavior must include
a matching docs update at that exact path, committed and pushed in the same
working session. Every PR must carry a `docs-updated` or `docs-n/a` marker in
its body or labels — `.github/workflows/docs-reminder.yml` fails it otherwise.

<!-- kodade:kodmem-project:v1:start -->
Follow the managed KödMem project-context rule in `AGENTS.md`.
<!-- kodade:kodmem-project:v1:end -->
