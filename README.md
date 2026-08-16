# ContractorProject

Local-first, AI-native project and job management for contractors. ContractorProject is the first standalone module in a planned suite of offline business tools.

Status: v0.1 persisted scheduled-job tracer

## Planning baseline

- [Product brief](docs/PRODUCT_BRIEF.md)
- [Architecture and stack](docs/ARCHITECTURE.md)
- [Design guide](docs/DESIGN.md)
- [Logo mockups](docs/LogoMockups.html)
- [Data model](docs/DATA_MODEL.md)
- [Local agent API](docs/LOCAL_API.md)
- [MVP plan](docs/MVP_PLAN.md)
- [Application foundation ADR](docs/adr/0001-application-foundation.md)
- [Gantt treegrid contract](docs/GANTT_TREEGRID.md)
- [Gantt timeline contract](docs/GANTT_TIMELINE.md)
- [Packaged Gantt platform verification](docs/GANTT_PLATFORM_VERIFICATION.md)

The Tauri desktop app now creates and edits a persisted work breakdown, stores
weekly-calendar and finish-to-start schedule inputs, computes the deterministic
schedule in Rust, and renders the production Gantt in the normal job workflow.
The same projection is reproduced after reopening the SQLite database.

## Development

```bash
npm install
npm run build
npm run typecheck
npm test
npx playwright install chromium
npm run test:browser
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
npm run tauri dev
```
