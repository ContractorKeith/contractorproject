# ADR 0002: Gantt rendering structure

Status: accepted
Date: 2026-08-14

## Context

ContractorProject needs a responsive 1,000-task work-breakdown and timeline surface with hierarchy, dependencies, baselines, variance, critical state, keyboard operation, and platform accessibility. The renderer must remain a projection of schedule data owned by the Rust application core.

The spike compared three structures against the same deterministic fixture: a semantic virtualized table beside a viewport-cropped SVG in one scrollport, independently virtualized table and canvas panes, and an all-DOM table with embedded timeline cells.

## Decision

Build the production Gantt as one shared-scroll split view:

- A native HTML table with `role="treegrid"` is the authoritative work-breakdown surface.
- `@tanstack/react-virtual` virtualizes visible rows while preserving stable logical row metadata.
- An adjacent SVG renders only visible bars, baselines, milestones, row rules, and dependency paths.
- Current dates, baseline dates, variance, critical or milestone state, float, and predecessor identities remain available as table text; the SVG adds no unique schedule fact.
- Rust owns scheduling calculations and validation. The React projection and SVG adapter receive a versioned read model and never calculate or persist schedule truth.

Use one vertical scroll owner and preserve the date at the viewport center when zoom changes. Keep dependency paths in SVG unless measured production connector density exceeds the accepted interaction budget.

## Evidence

The throwaway comparison is preserved, but intentionally not merged, at [`spike/gantt-comparison` commit `a8a5db0`](https://github.com/ContractorKeith/contractorproject/tree/a8a5db0fe80538e938eba4fd594a7ebc2c4e38cb). Its optimized five-pass median for the selected variant on an Apple M2 at device-pixel ratio 2 was:

- 22.9 ms initial paint
- 18.3 ms scroll p95 and 71.8 ms p99
- 2.8% of scroll frames above 25 ms
- 62.7 ms collapse p95 and 46.3 ms zoom p95
- 0 px measured vertical drift
- 115 task-related DOM/SVG nodes and no median long tasks above 50 ms

The selected variant met the spike envelope: initial paint at or below 750 ms; scroll p95 at or below 25 ms; fewer than 5% of frames above 25 ms; p99 at or below 100 ms; collapse and zoom p95 at or below 100 ms; drift at or below 1 px; fewer than 500 task-related nodes; and no median long-task count above zero. Production targets remain 16.7 ms p95 and 33.3 ms p99.

The spike also proves stable hierarchy metadata after collapse, offscreen keyboard focus recovery, horizontal ruler synchronization, zoom anchoring, reduced motion, forced colors, and axe-core checks in Chromium. Packaged WKWebView with VoiceOver and WebView2 with NVDA remain production acceptance gates. GitHub Actions [run 31823456713](https://github.com/ContractorKeith/contractorproject/actions/runs/31823456713) passed frontend, macOS, and Windows gates for the evidence commit.

## Alternatives

- Split table and canvas panes mounted fewer nodes, but produced a 224 ms median scroll p99 and up to 307 px of transient vertical drift. Canvas also makes hit testing, inspection, forced-color adaptation, and future interactions harder.
- The all-DOM control produced a 121.4 ms scroll p95, 928.2 ms collapse p95, 1,326.1 ms zoom p95, and 5,358 task-related nodes.
- DHTMLX Gantt Community v10 is MIT, but omits critical path, auto-scheduling, resources, and baseline/custom timeline capabilities needed by v1. Versions before v10 used a different license.
- Frappe Gantt is MIT but eagerly renders SVG bars and dependencies and has no suitable treegrid keyboard contract.
- Bryntum Gantt has strong performance and accessibility support, but its proprietary redistribution terms conflict with a clone-and-build open-source core.

## Consequences

- Add `@tanstack/react-virtual` only with the production Gantt slice and preserve its MIT notice in shipped third-party notices.
- Promote the read-model projection, treegrid contract, viewport state, and SVG adapter as production code; do not merge the spike switcher, benchmark panel, fixture, canvas renderer, or all-DOM control.
- Treat the spike envelope as a regression floor, not the final performance target.
- Every drag interaction must have a keyboard and non-drag alternative.
- Completion requires packaged-app keyboard and assistive-technology evidence on both supported platforms.
