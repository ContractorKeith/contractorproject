# ContractorProject product brief

Status: v0.1 planning baseline
Updated: 2026-08-14

## Product

ContractorProject is a local-first, AI-native project and job management desktop app for contractors. It is the first independently useful module in a future suite of offline, AI-augmented business applications.

The app serves solo contractors, small trades businesses, and small general contractors who want modern scheduling and job visibility without forced cloud storage, subscriptions, or vendor lock-in. The initial domain remains broad; fence, gate, remodeling, and specialty-trade examples are useful test cases, not product limits.

## Principles

- Core job planning works fully offline with data stored on the user's machine.
- macOS and Windows are first-class release targets.
- AI is assistive, explicit, and replaceable: BYOK providers and local models use the same application interface.
- The user reviews proposed AI changes before they affect job data.
- Every module can stand alone. Suite interoperability comes from stable IDs, versioned data contracts, and documented interfaces rather than a shared monolith.
- The open-source core must not prevent optional paid team or hosted capabilities later.

## v1 outcome

A contractor can create a job, build a hierarchical work plan, connect task dependencies, calculate the critical path, assign crews, establish a baseline, compare current dates and simple planned-versus-actual costs, back up or export the job, and ask an AI assistant to explain risks or propose schedule changes.

### Required

- Job-centric organization
- Hierarchical tasks and summary tasks
- All four dependency types (FS, SS, FF, SF) with signed lag
- Deterministic scheduling, float, and critical path
- Gantt view with baseline and variance
- Basic resource and crew assignment
- Simple cost codes and planned-versus-actual costs
- Local SQLite database with explicit backup and export
- Native macOS and Windows packages
- Natural-language assistance and useful schedule/cost risk flags
- Documented local MCP interface for agents

### If capacity remains

- Working-time calendars and exceptions
- Milestones, notes, and file attachments
- CSV import/export beyond the required backup and job archive
- Opt-in local network mode

## Non-goals

- Full estimating, CRM, invoicing, inventory, payroll, or timekeeping
- Complex multi-project portfolio management
- Real-time cloud collaboration or automatic cloud sync
- Mobile applications
- A plugin marketplace or generalized workflow engine
- Autonomous AI edits without validation and user acceptance

## Product decisions still open

- Public website domain
- Open-source license
- Minimum supported macOS and Windows versions
- Whether v1 ships only the custom project archive and CSV or also imports a third-party project format
- Which Gantt rendering approach passes the interaction and licensing spike

These decisions do not block the first vertical slice except for the Gantt spike and distribution-license audit.

## Accepted identity

ContractorProject is the product name. The three-bar dependency mark, steel palette, Barlow typography, and application visual language are defined in `DESIGN.md` and `LogoMockups.html`.
