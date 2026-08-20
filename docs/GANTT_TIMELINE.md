# Production Gantt timeline

Status: implemented
Updated: 2026-08-20

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
scrolling. Left/Right move at least 24 pixels (or one civil day when wider),
Page Up/Down move one visible-timeline width, and Home/End reach the horizontal
extents. These Home/End/Page Up/Page Down commands are horizontal; vertical
treegrid movement remains Ctrl/Cmd+Home/End and Page Up/Page Down, while
Arrow Up/Down retain native vertical scrolling when the timeline region has
focus. Pointer
dragging on its non-interactive canvas pans the same native shared scroll plane
after a small activation threshold; it never applies a visual transform or
moves the vertical scroll owner. The region exposes no task semantics because
the adjacent treegrid is the accessible schedule spine.

The application shell acquires the user's local civil date once per loaded
schedule mount; it does not refresh across midnight while the app remains open.
When that day is inside `[domain.start, domain.finish)` and differs from the
data-date civil day, the timeline draws a labelled 1px dashed neutral marker at
civil midnight. The ruler's `RIGHT_PADDING` strip extends beyond that interval
and draws no today marker. Today never expands the domain; an omitted date draws
nothing. Its flag occupies a second label row below the data-date flag so nearby
dates remain readable. The solid accent data-date marker remains the
authoritative status instant and retains its existing domain-extension rule.

## Non-working-time shading

The timeline paints DESIGN.md §184 non-working shading behind the ruler, grid,
and bars from the read model's v6 `calendar` facts. Every non-working civil day
in the drawn domain gets a flat 50% `--color-neutral-200` fill with no hatch. A
day is non-working when its weekday is not in `workingWeekdays` or its date is in
`exceptionDates`. This is rendering from civil dates, not schedule math.

- **Civil-day anchor.** The drawn domain is floored to the civil midnight at its
  start and ceiled to the next civil midnight at its finish, so ruler ticks, grid
  rules, and shading rects share the same civil-day columns. Bars keep their exact
  instant x positions and only shift uniformly with the anchor.
- **Rect merging.** Adjacent non-working days merge into a single rect, so a
  Thursday-Friday-Monday closure that brackets a weekend draws as one rect
  instead of five. For a Monday-to-Friday week a full year is roughly 52–53
  weekend rects plus one rect per additional exception run, not one per
  non-working day; the fixture's six-month domain draws 27 rects. A merged rect
  that covers any exception day keeps the exception treatment.
- **Domain-edge clipping.** A run is clipped to the drawn domain, so a
  non-working stretch that reaches the first or last civil day renders only the
  portion inside it — a weekend at the very edge may show as a single day.
- **Weekly vs exception.** Each rect carries `data-nonworking="weekly"` or
  `data-nonworking="exception"`. Weekly runs are decorative (`aria-hidden`);
  exception-bearing runs are `role="img"` with an accessible name listing their
  dates (for example `Calendar exception 2026-11-26`, or a comma-separated list
  when a merged run spans several). The treegrid remains the schedule spine, so
  the shading adds ambient context without a color-only signal.
- **Zoom threshold.** Shading renders only when one civil day is at least 2px
  wide: day (28px), week (8px), and month (2.4px) qualify; quarter (0.8px) is too
  coarse to read a shaded day and renders none. Adjacent days always merge before
  this check, so a merged run is still drawn at the qualifying zooms.
- **Node budget.** The shading rects are timeline nodes and are counted by the
  performance node-budget selector (`[data-nonworking]`). Merging keeps the count
  low; the 1,000-row fixture measures ~131 total task-related nodes against the
  <500 ADR floor. ADR 0002 thresholds are unchanged.
- **Theme and forced colors.** The neutral fill follows the theme tokens. Under
  forced colors the fill is stripped, so runs fall back to system-colored edges:
  weekly runs get gray hairlines and exception runs a dashed `CanvasText` border
  (mirroring the baseline dashed treatment) to stay perceptible and distinct.

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
zero median long tasks. CI runs the isolated Chromium benchmark on macOS, the
target platform used for the checked-in evidence. The harness disables headless
frame-rate limiting and vsync so display throttling is not counted as Gantt
renderer work.

```bash
npm test
npm run test:browser
npm run test:performance
```

Packaged WKWebView/VoiceOver and WebView2/NVDA acceptance remains issue #7.
