# Production Gantt treegrid

Status: implemented
Updated: 2026-08-14

`src/gantt/GanttTreegrid.tsx` is the authoritative work-breakdown surface for
Gantt read-model contract v1. It renders a native HTML table with
`role="treegrid"` and uses TanStack Virtual to mount only the visible task rows
plus overscan. The supplemental SVG timeline remains a separate renderer and
must not introduce schedule facts that are absent from this table.

## Semantic contract

- The header is logical row 1. A Rust `logicalIndex` of zero is exposed as
  `aria-rowindex="2"`, and collapse never renumbers later rows.
- Depth, expanded state, sibling position, and sibling-set size come directly
  from the versioned read model.
- Task, duration, current and baseline dates, signed variance, total float,
  critical and milestone state, and full predecessor IDs remain cell text.
- Exactly one mounted grid cell participates in the page tab order. Focus is
  tracked by task ID so virtualization, collapse, and a reordered projection
  can restore it to the same task or its nearest visible ancestor.
- Arrow keys move among rows and cells; Home and End move across a row;
  Control/Command plus Home or End reaches the first or last visible logical
  row; Page Up and Page Down move by a viewport; Left and Right collapse,
  expand, or return to a parent; Space or Enter toggles a summary.

The disclosure control has a 24-pixel target but is removed from the tab order
because the active task cell owns keyboard collapse and expansion. The surface
does not claim selection semantics; focus and future selection remain separate
states.

## Verification

Component tests cover semantics, complete schedule text, keyboard navigation,
collapse, and projection reorder. The Playwright contract renders a deterministic
1,000-row read model in Chromium, proves that fewer than 50 task rows are
mounted, jumps focus to the last logical row and back, recovers focus after an
offscreen reorder, runs axe-core, and checks reduced-motion and forced-color
behavior.

```bash
npm test
npx playwright install chromium
npm run test:browser
```

Packaged WKWebView/VoiceOver and WebView2/NVDA acceptance remains issue #7.
