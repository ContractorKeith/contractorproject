# ContractorProject

ContractorProject is a local-first, AI-native project and job management tool for contractors. The initial goal is to establish the product and technical foundations without committing prematurely to a UI framework or application runtime.

## Status

skeleton — last touched 2026-08-14

## Commands

```bash
# install
npm install

# build
npm run build

# typecheck
npm run typecheck
```

## Architecture

- `src/` contains the TypeScript application source.
- `dist/` is generated build output and is not committed.

## Conventions & Gotchas

- TypeScript project; keep the initial scaffold framework-neutral until product architecture is decided.
- Preserve local-first ownership of project and job data in architectural decisions.

## Out of Scope

- Selecting a UI framework, desktop shell, synchronization model, or AI provider during initial scaffolding.
