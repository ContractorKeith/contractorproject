# Packaged Gantt platform verification

Status: active verification protocol
Updated: 2026-08-14

Issue #7 validates the production `GanttTreegrid` and `GanttTimeline` inside
the packaged platform webviews. Persisted schedule inputs are a later slice,
so the verification package uses the deterministic, non-customer
`contractorproject-gantt-1000-v1` fixture. Normal application builds exclude
the fixture and continue to open the local-jobs workspace.

## Reproducible artifacts

Build the current platform locally:

```bash
npm run tauri:verification:build
```

The build embeds its Git commit, fixture ID, contract version, row count, and
webview user agent in the visible package. The manually dispatched
`Gantt platform verification` workflow produces macOS and Windows bundles from
the same commit and retains their artifact digests.

Run the packaged benchmark before enabling the screen reader. Record the five
pass median p95 and p99, percentage of frames over 25 ms, maximum vertical
drift, task-related node count, and median long tasks. Then restart the package
before assistive-technology testing.

## Keyboard and announcement protocol

Record the exact app commit, artifact digest, OS build, webview version, screen
reader version, theme or contrast mode, and actual spoken text for each step.

1. Tab to `Packaged Gantt verification schedule`. Confirm the treegrid name,
   seven columns, and 1,001 total rows including the header are conveyed.
2. Enter the first Phase 1 row. Confirm its level, expanded summary state,
   critical state, and logical row position are conveyed.
3. Move across WBS, Name, Duration, Start, Finish, Predecessors, and Float.
   Confirm current dates, baseline dates, signed variance, critical state, and
   full predecessor IDs are spoken from table semantics.
4. Move to Activity 99 and confirm milestone state. Use Control+End to reach
   the final logical row, then Control+Home to return to Phase 1.
5. Collapse and expand Phase 1 with Space or Enter. Confirm focus remains on
   Phase 1 and Phase 2 retains logical row position 102.
6. Use PageDown and PageUp across virtualized rows. Confirm focus never drops
   to the window or lands on an offscreen cell.
7. Confirm the supplemental timeline is absent from the accessibility tree and
   that the authoritative table retains every schedule fact. Return to the
   zoom group and select Day, Week, Month, and Quarter. Confirm zoom and shared
   horizontal scrolling preserve the focused task without drag input.
8. Repeat the focused-row and timeline checks in macOS Increase Contrast or
   Windows High Contrast and with Reduce Motion enabled. Capture screenshots
   showing the focus indicator and distinguishable task, critical, milestone,
   baseline, and dependency geometry.

## Required evidence

The repository evidence record must include:

- macOS WKWebView with VoiceOver actual announcements and screenshots;
- Windows WebView2 with NVDA actual announcements and screenshots;
- packaged benchmark results from each platform;
- any failures, workarounds, or untested behavior stated explicitly.

Automated Chromium/axe and native packaging CI are supporting evidence only;
they do not replace the two packaged screen-reader passes.
