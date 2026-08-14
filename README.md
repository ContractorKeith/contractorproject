# ContractorProject

Local-first, AI-native project and job management for contractors. ContractorProject is the first standalone module in a planned suite of offline business tools.

Status: v0.1 planning

## Planning baseline

- [Product brief](docs/PRODUCT_BRIEF.md)
- [Architecture and stack](docs/ARCHITECTURE.md)
- [Data model](docs/DATA_MODEL.md)
- [Local agent API](docs/LOCAL_API.md)
- [MVP plan](docs/MVP_PLAN.md)

The recommended implementation is a Tauri 2 desktop app with a React/TypeScript UI, a Rust application core, and SQLite storage. The repository remains a planning scaffold until the first architecture spike is accepted.

## Development

```bash
npm install
npm run build
npm run typecheck
```
