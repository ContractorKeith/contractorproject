# Gantt rendering spike

Status: complete on `spike/gantt-comparison`
Date: 2026-08-14
Issues: #1 and #2

## Question

Which rendering structure keeps a deterministic 1,000-task schedule responsive, vertically synchronized, and accessible without moving schedule rules into the renderer?

## Verdict

Select a custom shared-scroll split view. Use `@tanstack/react-virtual` for the semantic work-breakdown treegrid and a viewport-cropped SVG timeline for bars, baselines, milestones, and visible dependency paths. The table is authoritative; the SVG is supplemental and `aria-hidden`.

The optimized-build median results are accepted against this spike envelope: initial paint at or below 750 ms; scroll p95 at or below 25 ms; fewer than 5% of frames above 25 ms; p99 at or below 100 ms as a guard against isolated virtual-window swaps; collapse and zoom p95 at or below 100 ms; drift at or below 1 px; fewer than 500 task-related nodes; and no median long-task count above zero for the selected variant. A 16.7 ms p95 and 33.3 ms p99 remain production optimization targets, not spike gates.

The optimized-build median results were:

| Variant | Initial paint | Scroll p95 | Missed frames | Collapse p95 | Zoom p95 | Drift | Task nodes | Long tasks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Shared viewport + SVG | 22.9 ms | 18.3 ms | 2.8% | 62.7 ms | 46.3 ms | 0 px | 115 | 0 |
| Split panes + canvas | 32.0 ms | 23.6 ms | 3.9% | 34.4 ms | 34.3 ms | 307 px | 27 | 0 |
| Row-embedded DOM | 330.4 ms | 121.4 ms | 8.3% | 928.2 ms | 1,326.1 ms | 0.5 px | 5,358 | 49 |

The shared SVG variant met the spike envelope, with a 71.8 ms median p99 below the 100 ms guardrail. Canvas reduced mounted nodes but produced a 224 ms median p99 and up to 307 px of transient inter-pane drift when sampled every animation frame; it also made hit testing, inspection, forced colors, and future interactions materially harder. The all-DOM control degraded badly under repeated interaction. SVG therefore wins on total product risk rather than the lowest isolated render cost.

Raw five-pass results and environment details are in `benchmark-2026-08-14.json`.

## Dependency and license audit

| Option | License and evidence | Disposition |
| --- | --- | --- |
| Custom + TanStack Virtual | [`@tanstack/react-virtual` 3.14.9](https://github.com/TanStack/virtual/blob/main/LICENSE), MIT; headless and locally bundled | Selected |
| DHTMLX Gantt Community | [10.0.1 is MIT](https://github.com/DHTMLX/gantt/blob/master/LICENSE.md), but the [edition comparison](https://docs.dhtmlx.com/gantt/guides/editions-comparison/) omits several v1 differentiators | Contingency only |
| Frappe Gantt | [MIT](https://github.com/frappe/gantt/blob/master/license.txt), but its [current renderer](https://github.com/frappe/gantt/blob/master/src/index.js#L889-L925) eagerly creates bars and dependencies without a treegrid keyboard contract | Rejected |
| Bryntum Gantt | [Proprietary redistribution terms](https://bryntum.com/products/license/) conflict with a clone-and-build open-source core | Rejected |

The production lockfile and notices must preserve the TanStack MIT notice. DHTMLX must never be resolved below v10 if it is tested later because its earlier line used a different license.

## Accessibility evidence

- The authoritative surface is a native HTML table with `role="treegrid"`, real row and column elements, stable logical row metadata, sibling-position metadata, and roving keyboard focus.
- ArrowDown, ArrowLeft/Right, PageDown, Ctrl/Cmd+Home/End, and Enter behavior is covered at the component or browser seam.
- All three variants pass axe-core in a real Chromium browser after keyboard movement.
- Reduced motion and forced colors retain the controls and semantic table; light and dark themes were visually inspected.
- The timeline contains no unique schedule fact; current and baseline dates, variance, duration, float, milestone/critical state, and predecessor identities remain in accessible table text.
- Canvas cannot inherit forced-color remapping for author-painted pixels, even though its table remains usable. This is a specific reason the canvas variant is rejected.

Automated checks are evidence, not a substitute for platform assistive-technology checks. Use this checklist before the production schedule surface is accepted:

| Check | macOS WKWebView + VoiceOver | Windows WebView2 + NVDA |
| --- | --- | --- |
| Record app commit, OS, webview, and assistive-technology versions | [ ] | [ ] |
| Enter the treegrid with one Tab stop; hear task, hierarchy level, sibling position, and logical row position | [ ] | [ ] |
| Navigate cells and rows with arrows, Page Up/Down, Home/End, and Ctrl/Cmd+Home/End | [ ] | [ ] |
| Hear current dates, baseline dates, variance, critical/milestone state, and predecessor identities | [ ] | [ ] |
| Collapse and expand a summary without losing focus or row position | [ ] | [ ] |
| Zoom and horizontally scroll while the focused row and date anchor remain stable | [ ] | [ ] |
| Verify visible focus and task-state distinctions in increased contrast or High Contrast | [ ] | [ ] |
| Verify reduced motion removes nonessential animation | [ ] | [ ] |
| Record actual announcements plus focus/high-contrast screenshots | [ ] | [ ] |

## Reproduction

```bash
npm ci
npx playwright install chromium
npm run typecheck
npm run prototype:gantt:test
npm run prototype:gantt:measure
```

For manual viewing, run `npm run prototype:gantt` by itself after the automated commands finish. It builds the optimized comparison and serves `gantt-prototype.html` on port 4174. Stable variants are `?variant=shared-svg`, `?variant=split-canvas`, and `?variant=row-dom`.

The fixture is projection-only and contains no persistence or scheduling calculations. Production work should promote the visible-row projection, treegrid contract, viewport state, and SVG adapter as new code; it must not merge the switcher, benchmark panel, fixture, or rejected renderers.
