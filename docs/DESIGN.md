# ContractorProject design guide

Status: v0.1 baseline
Updated: 2026-08-14
Companions: `PRODUCT_BRIEF.md`, `ARCHITECTURE.md`, `LogoMockups.html`

This document defines the visual language for the ContractorProject desktop app. It covers principles, tokens, the Gantt and work-breakdown surfaces, dark mode, iconography, and logo usage. It is derived from the **Industry** design system: steel-blue accent on a light technical ground, Barlow Condensed over Barlow, square corners, hairline borders, blueprint registration marks.

Styling in the app is CSS variables plus a small local component layer, per `ARCHITECTURE.md`. There is no third-party UI framework. Every color, font, space, and radius comes from a token.

---

## 1. Principles

**1. The data is the interface.** Dates, durations, float, and costs are the product. Chrome recedes; type, rules, and alignment carry the hierarchy. No decorative panels, no gradients, no illustration inside working views.

**2. Drawn, not filled.** Cards, figures, and panels are line drawings — square-cornered, hairline-bordered, transparent. The one deliberately solid object on screen is the primary action. This keeps a dense schedule readable: fills are reserved for meaning (critical path, selection, variance), never for decoration.

**3. Approachable precision.** The audience is a solo contractor or a three-person office, not a scheduling analyst. Precision belongs in the numbers; the copy, labels, and empty states stay plain and human. Say "This task can't start before its predecessor finishes," not "FS constraint violation."

**4. Local-first is visible.** The app never implies a cloud. Sync-like language is out. Storage, backup, and export state are shown as facts about the user's machine — a path, a timestamp, a file size.

**5. AI is a proposal, never a fact.** Anything the model produced is visually marked as unapplied until the user accepts it. Proposed values are shown alongside current values, never in place of them. Deterministic risk flags and model prose are typographically distinguishable.

**6. Keyboard and screen reader first on the table.** The work-breakdown table is the accessible spine of the schedule. The timeline may be primarily visual, but nothing is only available in the timeline.

**7. Density is a setting, not a default opinion.** The spacing scale runs at 0.85×. Row height is tokenized so a comfortable and a compact mode differ by one variable, not a rewrite.

---

## 2. Color tokens

Roles carry a 100–900 ramp generated in OKLCH on one shared lightness scale, so the same step of any role has the same visual weight. Use 100–300 for tinted fills, hovers, and subtle borders; 500 as the base; 700–900 for text on tinted fills and pressed states. Prefer ramp steps over ad-hoc `color-mix()`.

### Base roles (light)

```css
:root {
  --color-bg: #f2f2f3;
  --color-surface: #e9e9ea;
  --color-text: #1d1f20;
  --color-accent: #5980a6;
  --color-divider: color-mix(in srgb, #1d1f20 16%, transparent);
}
```

### Ramps

```css
:root {
  --color-neutral-100: #f5f5f8;  --color-neutral-200: #e7e7ea;
  --color-neutral-300: #d4d4d7;  --color-neutral-400: #b7b7ba;
  --color-neutral-500: #98989b;  --color-neutral-600: #7a7a7d;
  --color-neutral-700: #5d5d60;  --color-neutral-800: #424244;
  --color-neutral-900: #2b2b2d;

  --color-accent-100: #eef6ff;   --color-accent-200: #d6ebff;
  --color-accent-300: #b5d9fd;   --color-accent-400: #94bce3;
  --color-accent-500: #749dc4;   --color-accent-600: #597ea3;
  --color-accent-700: #416180;   --color-accent-800: #2c455d;
  --color-accent-900: #1d2d3d;
}
```

The palette is mono: one steel accent. `--color-accent-2-*` exists in the system as a machine-derived stand-in that resolves to the same role — treat it as accent. Do not introduce a second brand hue.

### Semantic aliases

Define these once in the app and reference them everywhere. They are the only place scheduling meaning is bound to color.

```css
:root {
  --sched-critical:      var(--color-accent-800); /* critical path bar */
  --sched-normal:        var(--color-accent-500); /* non-critical bar  */
  --sched-summary:       var(--color-neutral-800); /* summary rollup    */
  --sched-baseline:      var(--color-neutral-400); /* baseline ghost    */
  --sched-milestone:     var(--color-text);
  --sched-slack:         var(--color-accent-200); /* free float band    */
  --sched-actual:        var(--color-accent-700); /* progress overlay   */

  --state-risk:          var(--color-accent-800); /* deterministic flag */
  --state-proposed:      var(--color-accent-600); /* unapplied AI value */
  --state-variance-late: var(--color-neutral-900);
  --state-selected-bg:   var(--color-accent-100);
  --state-focus-ring:    var(--color-accent);
}
```

**Never encode meaning in hue alone.** The palette is monochromatic by design, so every status uses a second channel: a hatch pattern, a border weight, a glyph, or a text label. Critical path is the darkest bar *and* carries a critical badge in the table. Late variance is a ghosted bar *and* a signed day count.

### Contrast rules

The accent-to-ground pair is tuned to ≥3:1 — enough for icons, chrome, and large text, not for body copy. For paragraph-size accent text on the light ground use `--color-accent-700`. Table numerics use `--color-text`. Muted metadata bottoms out at `--color-neutral-700` on `--color-bg`; never `-500` or lighter for text.

---

## 3. Type tokens

```css
:root {
  --font-heading: "Barlow Condensed", system-ui, sans-serif;
  --font-heading-weight: 600;
  --font-body: "Barlow", system-ui, sans-serif;
  --font-numeric: "Barlow", system-ui, sans-serif; /* tabular figures */
}
```

Barlow Condensed sets headings, panel titles, column headers, and the wordmark. Barlow sets body, labels, and inputs.

| Role | Family | Size | Weight | Notes |
| --- | --- | --- | --- | --- |
| View title | Condensed | 24px | 600 | uppercase optional, tracking +0.02em |
| Panel title | Condensed | 15px | 600 | uppercase, tracking +0.08em |
| Column header | Condensed | 12px | 600 | uppercase, tracking +0.08em |
| Table cell | Barlow | 13px | 400 | line-height 1.25 |
| Numeric cell | Barlow | 13px | 500 | `font-variant-numeric: tabular-nums`, right-aligned |
| Label | Barlow | 12px | 500 | |
| Metadata | Barlow | 11px | 400 | `--color-neutral-700` |
| Body prose | Barlow | 14px | 400 | line-height 1.5, `text-wrap: pretty`, max 68ch |

All dates, durations, currency, and float values use `font-variant-numeric: tabular-nums` so columns align across rows. Currency is rendered from integer minor units; the UI never does float math.

### Spacing, radius, elevation

Density 0.85× and 4px radius are baked in. Use the variables, not raw px.

```css
--space-1: 3.4px;  --space-2: 6.8px;  --space-3: 10.2px;
--space-4: 13.6px; --space-6: 20.4px; --space-8: 27.2px;

--radius-sm: 2px;  --radius-md: 4px;  --radius-lg: 7px;

--shadow-sm: 0 1px 2px color-mix(in srgb, #2b2b2d 14%, transparent);
--shadow-md: 0 3px 10px color-mix(in srgb, #2b2b2d 16%, transparent);
--shadow-lg: 0 12px 32px color-mix(in srgb, #2b2b2d 22%, transparent);
```

Elevation is for dialogs and popovers only. Working surfaces are flat and separated by hairlines.

### Row rhythm (schedule surfaces)

```css
--row-h: 28px;          /* comfortable — default */
--row-h-compact: 24px;
--row-indent: 16px;     /* per WBS level */
--bar-h: 14px;          /* task bar inside the row */
--bar-h-summary: 8px;
```

`--row-h` is the single source of truth for both the table and the timeline. Nothing in either pane may hard-code a row height; vertical sync depends on this.

---

## 4. Work breakdown table

The table is a semantic, virtualized `<table>` — real `<th scope>`, real rows — not a div grid.

- **Header:** Condensed 12px uppercase on `--color-surface`, bottom rule `1px solid var(--color-divider)`. Sticky. No vertical rules between columns; alignment does the work.
- **Rows:** hairline bottom rule only. No zebra striping — striping fights the timeline's own banding. Hover tints `--color-accent-100`; selection fills `--state-selected-bg` with a 2px left edge in `--color-accent`.
- **Hierarchy:** indent by `--row-indent` per level. Summary tasks are Condensed 600 in `--color-neutral-900`; leaf tasks are Barlow 400. Disclosure is a Lucide `chevron-right` / `chevron-down` at 14px, rotating, not two different glyphs.
- **Columns, default order:** WBS ID · Name · Duration · Start · Finish · Predecessors · Float · Assigned · % Complete. Text left, all numerics and dates right, tabular figures.
- **Float column:** negative float renders in `--state-risk` with an explicit minus sign and a Lucide `alert-triangle` at 12px. Zero float renders a `critical` tag.
- **Editing:** cells edit in place. The input is borderless until focus, then takes the 2px accent `:focus-visible` ring inset within the cell. Invalid entry keeps the value, shows the ring in `--color-accent-800`, and puts the reason in a hint row below — never a modal.
- **Keyboard:** arrow keys move the cell cursor, `Tab` moves within a row, `Enter` commits and moves down, `Esc` reverts, `Alt`+arrows outdent/indent. Every row exposes an accessible name of `"{wbs} {name}, {start} to {finish}, {duration}"`.

---

## 5. Gantt timeline

A virtualized semantic table on the left, an SVG (or canvas) timeline layer on the right, sharing one scroll container and one `--row-h`. The renderer is a rendering adapter only; it never owns the domain model.

### Frame

- **Time header:** two tiers — a coarse tier (month or week) in Condensed 12px uppercase over a fine tier (day or week) in Barlow 11px. Sticky, on `--color-surface`, bottom rule `--color-divider`.
- **Grid:** vertical rules at fine-tier boundaries in `--color-neutral-200`; period boundaries (month starts) in `--color-neutral-300`. Non-working days get a fill of `--color-neutral-200` at 50%, no diagonal hatch — hatch is reserved for float.
- **Data date:** a 1.5px vertical line in `--color-accent` full height with a small flag label at the header. Today, if different from the data date, is a 1px dashed line in `--color-neutral-500`.

### Bars

| Object | Form |
| --- | --- |
| Leaf task | `--bar-h` rect, `--radius-sm`, fill `--sched-normal` |
| Critical task | same, fill `--sched-critical` + 1px `--color-accent-900` border |
| Summary | `--bar-h-summary` bar in `--sched-summary` with downward end caps |
| Milestone | rotated square (diamond), side `--bar-h`, fill `--sched-milestone` |
| Progress | inset overlay of `--sched-actual` at the completed fraction, full bar height, left-aligned |
| Baseline | `6px` ghost bar in `--sched-baseline` offset below the live bar, never overlapping it |
| Free float | open band in `--sched-slack` with a 1px dotted right terminator |

Bar labels sit outside the bar to the right in Barlow 11px `--color-neutral-700`, and are suppressed when zoom makes them collide.

### Dependency lines

1px orthogonal polylines in `--color-neutral-600`, 3px corner radius, with a 4px solid arrowhead at the successor. Lines on the critical path draw in `--color-accent-800` at 1.5px. Lines route around bars, never through them. On row hover, that task's in- and out-edges raise to `--color-text`; all others drop to 35% opacity.

### Interaction

- Drag-to-reschedule shows a live ghost of the moved bar plus a header tooltip of the new start and finish. On drop, the recalculated result is what renders — the optimistic position is discarded.
- A drag that would violate a dependency shows the ghost in `--color-accent-800` with a dotted outline and does not commit.
- Zoom levels are discrete: day / week / month / quarter. Zoom is keyboard-reachable and preserves the row under the cursor.
- 1,000 visible rows must remain responsive while scrolling and zooming; this is a rendering requirement, not a styling one.

### Variance view

Baseline and current bars stack in the same row: current above, baseline ghost below. The variance column shows a signed day count in tabular figures — late in `--state-variance-late` with a filled triangle glyph, early in `--color-neutral-700`. Cost variance follows the same signed pattern in minor-unit-formatted currency.

---

## 6. AI assistant surfaces

- The assistant panel is a right-side drawer with the same hairline frame as any panel. It is never modal and never blocks the schedule.
- **Provider disclosure is always visible**, not in a settings screen: a line at the top of the panel naming the provider that will receive context, and what scope of job data goes with the request. Local models read `Local · no data leaves this machine`.
- **Deterministic risk flags** render as table-like rows: a Lucide glyph, the fact, the affected task link. They are facts, set in Barlow 13px `--color-text`.
- **Model prose** renders indented under the flag it explains, in `--color-neutral-700`, prefixed with a hairline left rule in `--color-accent-300`. The typographic difference between fact and explanation is load-bearing.
- **Proposals** render as a diff list: each changed field on one line as `label · current → proposed`, current in `--color-neutral-700` with a strikethrough, proposed in `--state-proposed` weight 500. A proposal card carries an `unapplied` tag until accepted, and Accept / Discard as primary and ghost buttons.
- Applying a proposal is one undoable transaction; the UI confirms with an inline `Applied · Undo` affordance, not a toast that disappears.

---

## 7. Dark mode

Dark mode is a token override on `[data-theme="dark"]`, not a second stylesheet. The ground is a warm-neutral dark, not the accent — the accent field is a brand device, not an app background.

```css
[data-theme="dark"] {
  --color-bg: #1f2124;
  --color-surface: #2b2b2d;
  --color-text: #ececed;
  --color-accent: #94bce3;          /* accent-400 becomes the base */
  --color-divider: color-mix(in srgb, #ececed 18%, transparent);

  --sched-critical: var(--color-accent-300);
  --sched-normal:   var(--color-accent-600);
  --sched-summary:  var(--color-neutral-300);
  --sched-baseline: var(--color-neutral-600);
  --sched-slack:    var(--color-accent-800);
  --sched-actual:   var(--color-accent-200);
  --state-selected-bg: color-mix(in srgb, #94bce3 14%, transparent);

  --shadow-sm: 0 0 0 1px color-mix(in srgb, #ececed 10%, transparent);
  --shadow-md: 0 3px 10px rgb(0 0 0 / 0.45), 0 0 0 1px color-mix(in srgb, #ececed 10%, transparent);
  --shadow-lg: 0 12px 32px rgb(0 0 0 / 0.55), 0 0 0 1px color-mix(in srgb, #ececed 12%, transparent);
}
```

Rules:

- Ramps do not invert; **usage** inverts. Light steps carry text and emphasis on dark; dark steps carry tinted fills.
- Elevation on dark is a hairline edge plus ambient darkness, never a lighter fill alone.
- Non-working-day fill becomes `--color-neutral-800` at 50%; grid rules become `--color-neutral-700`.
- Pressed states step toward `--color-accent-400`/`-300` (lighter) rather than darker.
- Follow the OS by default (`prefers-color-scheme`), with an explicit three-way override: System / Light / Dark. Both themes ship at parity; neither is the afterthought.

---

## 8. Iconography

Lucide, stroke-width **1.5**, `currentColor`, never filled. Sizes: 14px in table rows, 16px in buttons and panel headers, 20px in the toolbar. Icons are vendored as inline SVG, not loaded from a CDN — the app works offline.

Every icon in a working view either has a visible text label or an `aria-label`; icon-only buttons additionally carry a tooltip. Never use an icon as the sole carrier of status.

Canonical set:

| Meaning | Lucide icon |
| --- | --- |
| Job | `folder` |
| Task / summary | `square` / `chevron-right` |
| Milestone | `diamond` |
| Dependency | `link-2` |
| Critical path | `zap` |
| Crew / resource | `users` |
| Cost code | `tag` |
| Baseline | `layers` |
| Risk flag | `alert-triangle` |
| Assistant | `sparkle` |
| Backup / export | `hard-drive-download` |
| Local storage | `hard-drive` |

---

## 9. Logo usage

The identity is settled: **three offset bars with two connector stubs**, a schedule with a dependency running through it, set on a filled field. It is drawn in every required application in `LogoMockups.html`.

### The mark

Constructed on a 32-unit grid:

| Element | Geometry |
| --- | --- |
| Bar 1 | x 2, y 4, w 16, h 6 — ink or paper |
| Bar 2 | x 8, y 13, w 20, h 6 — **accent** |
| Bar 3 | x 4, y 22, w 12, h 6 — ink or paper |
| Connector A | x 16.4, y 10, w 1.6, h 3 — accent |
| Connector B | x 8, y 19, w 1.6, h 3 — accent |

Bar height 6u, gutter 3u, connectors 1.6u wide bridging the gutter. Bar ends never align — 18u, 28u, 16u. The accent carries the middle bar and both connectors: the dependency is the colored element, the tasks are not.

The mark occupies **60% of the tile edge**, optically centered. Tile radius is **22% of the edge on macOS** and **0 everywhere else**.

### Two forms

- **Horizontal lockup** — tile plus the full wordmark, gap equal to 1/3 of the tile edge. The primary application.
- **Square** — tile and mark alone, no name. Rounded on macOS, square-cornered elsewhere.

Both forms exist on the light ground and the dark ground; neither is a recolor of the other.

### Construction rules

- **Clear space:** equal to the wordmark's cap height on all sides (lockup), or 1/6 of the edge (square). Nothing enters it — no tagline, no rule, no registration mark.
- **Minimum sizes:** lockup 120px wide on screen, 1in in print. Square 24px.
- **Optical sizing:** the icon is drawn per size, not scaled. At **16px the mark drops to two bars with no connectors** — a 1.6u stub cannot survive the raster. Every size from 32px up keeps the full mark. Ship 16 / 32 / 48 / 128 / 256 / 512 / 1024.
- **Wordmark:** Barlow Condensed 600, tracking −0.012em, set as one word — two capitals, no space, no hyphen. `Contractor` in ink, `Project` in the accent.
- **Permitted color pairs**, one accent plus one ink per instance:
  1. Steel field `#1d2d3d` · paper mark `#f2f2f3` · accent `#94bce3` — default, light UI
  2. Ink field `#1d1f20` · paper mark, single color — one-color print, favicons, embossing
  3. Paper field `#ececed` · ink mark `#1d2d3d` · accent `#5980a6` — default, dark UI
  4. Accent-900 field carrying reversed type — installer and marketing banners
- **Registration marks** (the system's  crosshairs) belong to the layout, not the logo. Never fuse them into the mark.

### Don't

- No hue outside the steel palette.
- No stretching or condensing the tile, mark, or lockup.
- No outlined tile, no drop shadow, no gradient.
- No rotation. The bars are level.
- No mark on a photograph without a solid field behind it.
- No rebuilding the wordmark in another face, and no tagline inside the clear space.

---

## 10. Open items

- Comfortable vs. compact as the shipped default row height.
- Whether the timeline layer is SVG or canvas at 1,000 rows — decided by the Gantt spike, may affect how bar labels and dependency lines are drawn.
- Print and PDF treatment of the Gantt, which is out of v1 scope but will constrain bar and label geometry when it arrives.
