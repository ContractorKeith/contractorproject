# ContractorProject

## Runtime routing

- Claude Code main sessions read `../dotfiles/claude/ORCHESTRATION.md`.
- Other runtimes must not apply Claude's model assignments.

ContractorProject is a local-first, AI-native project and job management tool for contractors. It is the first standalone module in a planned suite of offline business applications.

## Status

v0.1 scheduled-job tracer — persisted work breakdown, schedule inputs,
SNET/FNLT constraints, audited data-date/progress statusing, and immutable
named baselines now flow through the pure Rust scheduler into the production
Gantt (read-model v4 with progress facts, a data-date marker, baseline ghost
bars, and start/finish/duration variance) in the normal desktop workflow.
Last touched 2026-08-19.

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

<!-- kodade:kodmem-project:v1:start -->
Follow the managed KödMem project-context rule in `AGENTS.md`.
<!-- kodade:kodmem-project:v1:end -->
