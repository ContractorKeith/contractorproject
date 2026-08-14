# ContractorProject

ContractorProject is a local-first, AI-native project and job management tool for contractors. It is the first standalone module in a planned suite of offline business applications.

## Status

v0.1 planning — last touched 2026-08-14

## Commands

```bash
# install
npm install

# build
npm run build

# typecheck
npm run typecheck
```

## Planning baseline

- `docs/PRODUCT_BRIEF.md` is the product scope.
- `docs/ARCHITECTURE.md` is the recommended Tauri, React, Rust, and SQLite architecture.
- `docs/DATA_MODEL.md` owns the initial domain language and persistence model.
- `docs/LOCAL_API.md` owns the local agent interface.
- `docs/MVP_PLAN.md` is the issue-ready delivery sequence.
- `src/` is only the framework-neutral TypeScript scaffold until the architecture spike is accepted.

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
