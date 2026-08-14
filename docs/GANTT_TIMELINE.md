# Production Gantt timeline

Status: implemented
Updated: 2026-08-14

`src/gantt/GanttTimeline.tsx` is the supplemental visual adapter for Gantt
read-model contract v1. `GanttTreegrid` owns one shared scroll plane and one
TanStack virtual window for both panes. The work-breakdown table stays pinned
at the left while horizontal movement reveals later timeline dates; there is no
second vertical scroll owner to synchronize. The pinned table scales from 460
to 720 pixels so the supported minimum desktop window still reserves a usable
timeline pane.

## Rendering contract

- The SVG is `aria-hidden`. Current and baseline dates, signed variance,
  critical and milestone state, float, and every predecessor ID remain
  authoritative table text.
- The SVG mounts only the table virtualizer's visible rows plus its bounded
  overscan. Bars, baselines, milestones, row rules, and dependency paths are
  therefore cropped to the same row window.
- `--row-h` remains the live geometry source for table rows and SVG rows at
  comfortable and compact density. Browser tests measure every mounted pair
  and permit at most one pixel of drift.
- The civil-time axis includes current and baseline bounds. Pixel conversion
  treats local timestamps as neutral schedule minutes; browser timezone and
  daylight-saving rules cannot move a bar.
- Day, week, month, and quarter zoom controls capture the visible center minute
  before changing scale and restore it to within one rendered pixel. Vertical
  scroll position does not change.
- A dependency is critical only when its endpoints are consecutive in the
  read model's representative `criticalPath`.

The focusable timeline region provides keyboard access to horizontal
scrolling. It exposes no task semantics because the adjacent treegrid is the
accessible schedule spine.

## Verification

The Chromium contract checks supplemental semantics, all supported bar forms,
multiple predecessor paths, exact mounted-ID correspondence, collapse,
comfortable and compact row alignment, ruler/grid synchronization, zoom
anchoring, forced colors, reduced motion, and axe-core.

The optimized-build performance gate runs one warm-up and five recorded passes
against the deterministic 1,000-row fixture. Its checked-in result is
`docs/evidence/gantt-timeline-performance-2026-08-14.json`. The enforced ADR
floor is initial paint at or below 750 ms, scroll p95 at or below 25 ms, p99 at
or below 100 ms, fewer than 5% of frames above 25 ms, collapse and zoom p95 at
or below 100 ms, drift at or below 1 px, fewer than 500 task-related nodes, and
zero median long tasks.

```bash
npm test
npm run test:browser
npm run test:performance
```

Packaged WKWebView/VoiceOver and WebView2/NVDA acceptance remains issue #7.
