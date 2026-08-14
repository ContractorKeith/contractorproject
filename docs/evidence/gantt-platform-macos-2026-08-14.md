# Packaged Gantt verification — macOS

Status: provisional; actual VoiceOver speech transcript still required
Date: 2026-08-14

## Artifact

- Commit: `3c387bb25c9dcc7607e96843a4cfae7708fc064e`
- Fixture: `contractorproject-gantt-1000-v1`
- Contract: Gantt read model v1
- Rows: 1,000 task rows
- Package: `ContractorProject Gantt Verification_0.1.0_aarch64.dmg`
- DMG SHA-256: `2bb50574caa6debd36f7ff2d1fdfe2473b0dba2633ceec8a7c2d4f043ea13168`
- App executable SHA-256: `b472a12014fadfc9908e40c8bee4f54dfb0e64ac54d5b221301b4a566ea0f5c1`

## Platform

- Hardware: Apple silicon (`arm64`)
- OS: macOS 26.5.2 (25F84)
- Webview: WKWebView, AppleWebKit 605.1.15
- User agent: `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)`
- Screen reader: VoiceOver 10 (build 993)

## Packaged performance

The first complete in-app run used one warm-up and five recorded passes:

| Metric | Result |
| --- | ---: |
| Initial paint | 75 ms |
| Scroll p95 | 27 ms |
| Scroll p99 | 32 ms |
| Frames above 25 ms | 10.83% |
| Maximum vertical drift | 0 px |
| Task-related nodes | 160 |
| Median long tasks above 50 ms | 0 |

This does not meet the issue #6 Chromium optimization envelope of p95 at or
below 25 ms and fewer than 5% of frames above 25 ms. A second run after window
navigation was slower (p95 40 ms, p99 41 ms, 75% above 25 ms) while retaining
zero drift, 160 nodes, and no long tasks. Treat the WKWebView result as a real
platform performance finding, not a passing Chromium-equivalent gate.

## Keyboard and accessibility observations

VoiceOver and the macOS caption panel were enabled in System Settings before
the pass. The packaged accessibility surface exposed the following focused
strings while navigating entirely with the keyboard:

- Treegrid: `Packaged Gantt verification schedule`, seven columns, 1,000 task
  rows plus the header.
- Summary: `1 Phase 1, task, summary, critical`; Left collapsed the row, kept
  focus on Phase 1, and exposed the row as collapsed with an Expand action.
- Start: `1.1 Activity 1, start, 2026-08-17 08:00, baseline 2026-08-14 08:00,
  variance +4,320 min`.
- Predecessor: `1.2 Activity 2, predecessors, phase-1-task-1`.
- Float: `1.2 Activity 2, total float, 0 min, critical`.
- Offscreen milestone: Control+End, Home, Right reached `10.99 Activity 99,
  task, milestone, not critical` without dropping focus to the window.
- Zoom: Shift+Tab reached the zoom group and Space selected Month; the Month
  toggle exposed an on state.

The test driver could inspect the focused WKWebView accessibility strings while
VoiceOver was enabled, but it could not capture the actual spoken or caption
panel transcript. Row-position announcements and exact spoken phrasing are
therefore not claimed as verified. A human VoiceOver pass must record those
announcements before issue #7 can close.

The supplemental SVG remained absent from the accessibility tree; all schedule
facts stayed available in the table. No rescheduling or dependency-editing
commands exist in this read-only production slice, so those non-drag operations
were not tested.

## Contrast and screenshots

macOS Increase Contrast was enabled for the final milestone/focus check and
then restored to its original off state. VoiceOver was also restored to off.

- [Package metadata](gantt-platform/macos-package-metadata.jpg)
- [Collapsed summary with retained focus](gantt-platform/macos-voiceover-collapse-focus.jpg)
- [Control+End milestone focus](gantt-platform/macos-voiceover-offscreen-milestone.jpg)
- [Increased-contrast milestone focus](gantt-platform/macos-increased-contrast-milestone-focus.jpg)
- [Packaged benchmark result](gantt-platform/macos-packaged-benchmark.jpg)

## Remaining acceptance work

- Capture the actual VoiceOver/caption transcript, including hierarchy level,
  expanded state, logical row positions, and PageUp/PageDown announcements.
- Investigate WKWebView frame pacing against the packaged performance envelope.
- Run and record the equivalent Windows WebView2/NVDA pass.
