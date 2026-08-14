# ContractorProject

Local-first, AI-native project and job management for contractors. ContractorProject is the first standalone module in a planned suite of offline business tools.

Status: v0.1 foundation implementation

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

The application foundation is a Tauri 2 desktop app with a React/TypeScript UI, a Rust application core, and SQLite storage. The first vertical slice creates a job through the Rust application interface, persists it locally, and lists it in the desktop UI.

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
