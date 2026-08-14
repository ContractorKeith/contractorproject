import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { createGanttFixture, SPIKE_FIXTURE_ID } from "./fixture";
import { flattenVisibleRows } from "./projection";
import { WBS_WIDTH, zoomWidths, type VariantKey, type Zoom } from "./schedule";
import { RowEmbeddedDom } from "./variants/RowEmbeddedDom";
import { SharedViewportSvg } from "./variants/SharedViewportSvg";
import { SplitPaneCanvas } from "./variants/SplitPaneCanvas";

const variants: Array<{ key: VariantKey; name: string; hypothesis: string }> = [
  {
    key: "shared-svg",
    name: "Shared viewport + SVG",
    hypothesis: "One scroll owner prevents drift while row virtualization keeps SVG inspectable.",
  },
  {
    key: "split-canvas",
    name: "Split panes + canvas",
    hypothesis: "Canvas lowers render work enough to justify synchronized scrollports and hit testing.",
  },
  {
    key: "row-dom",
    name: "Row-embedded DOM",
    hypothesis: "One all-DOM table maximizes semantic simplicity but reveals the cost of 1,000 mounted rows.",
  },
];
const zoomOrder: Zoom[] = ["day", "week", "month", "quarter"];
const fixture = createGanttFixture();

interface BenchmarkResult {
  recordedAt: string;
  fixtureId: string;
  variant: VariantKey;
  userAgent: string;
  viewport: string;
  devicePixelRatio: number;
  visibleRows: number;
  initialPaintMs: number;
  taskNodes: number;
  scrollP95Ms: number;
  scrollP99Ms: number;
  droppedFramePercent: number;
  collapseP95Ms: number;
  zoomP95Ms: number;
  maximumVerticalDriftPx: number;
  longTasksOver50Ms: number;
}

export function GanttPrototype() {
  const [variant, setVariant] = useState<VariantKey>(readVariant);
  const [zoom, setZoom] = useState<Zoom>("week");
  const [collapsedIds, setCollapsedIds] = useState<Set<string>>(() => new Set());
  const [dark, setDark] = useState(false);
  const [benchmarking, setBenchmarking] = useState(false);
  const [result, setResult] = useState<BenchmarkResult | null>(null);
  const [lastPaintMs, setLastPaintMs] = useState(0);
  const interactionStarted = useRef(performance.now());
  const initialPaintMs = useRef<number | null>(null);
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const horizontalViewportRef = useRef<HTMLDivElement | null>(null);
  const visibleRows = useMemo(
    () => flattenVisibleRows(fixture, collapsedIds),
    [collapsedIds],
  );
  const currentVariant = variants.find((candidate) => candidate.key === variant) ?? variants[0]!;

  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  }, [dark]);

  useEffect(() => {
    function handleVariantKeys(event: globalThis.KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (
        target?.matches("input, textarea, select, button, [contenteditable='true']") ||
        target?.closest("[role='treegrid']") ||
        !["ArrowLeft", "ArrowRight"].includes(event.key)
      ) {
        return;
      }
      const direction = event.key === "ArrowRight" ? 1 : -1;
      selectRelativeVariant(direction);
    }
    window.addEventListener("keydown", handleVariantKeys);
    return () => window.removeEventListener("keydown", handleVariantKeys);
  });

  useLayoutEffect(() => {
    const started = interactionStarted.current;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const duration = round(performance.now() - started);
        if (initialPaintMs.current === null) initialPaintMs.current = duration;
        setLastPaintMs(duration);
      });
    });
  }, [collapsedIds, variant, zoom]);

  function selectVariant(nextVariant: VariantKey) {
    interactionStarted.current = performance.now();
    setVariant(nextVariant);
    setResult(null);
    const url = new URL(window.location.href);
    url.searchParams.set("variant", nextVariant);
    window.history.replaceState(null, "", url);
  }

  function selectRelativeVariant(direction: number) {
    const currentIndex = variants.findIndex((candidate) => candidate.key === variant);
    const nextIndex = (currentIndex + direction + variants.length) % variants.length;
    selectVariant(variants[nextIndex]!.key);
  }

  function toggleCollapsed(id: string) {
    interactionStarted.current = performance.now();
    setCollapsedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function changeZoom(nextZoom: Zoom) {
    const horizontalViewport = horizontalViewportRef.current;
    const visibleTimelineWidth = horizontalViewport
      ? Math.max(
          1,
          horizontalViewport.clientWidth - (variant === "split-canvas" ? 0 : WBS_WIDTH),
        )
      : 1;
    const anchorDay = horizontalViewport
      ? (horizontalViewport.scrollLeft + visibleTimelineWidth / 2) / zoomWidths[zoom]
      : 0;
    const verticalScrollTop = viewportRef.current?.scrollTop ?? 0;
    interactionStarted.current = performance.now();
    setZoom(nextZoom);
    requestAnimationFrame(() => {
      if (horizontalViewport) {
        horizontalViewport.scrollLeft = Math.max(
          0,
          anchorDay * zoomWidths[nextZoom] - visibleTimelineWidth / 2,
        );
        horizontalViewport.dispatchEvent(new Event("scroll"));
      }
      if (viewportRef.current) viewportRef.current.scrollTop = verticalScrollTop;
    });
  }

  async function runBenchmark() {
    const viewport = viewportRef.current;
    if (!viewport || benchmarking) return;
    setBenchmarking(true);
    setResult(null);

    const longTasks: number[] = [];
    const observer =
      "PerformanceObserver" in window
        ? new PerformanceObserver((entries) => {
            for (const entry of entries.getEntries()) longTasks.push(entry.duration);
          })
        : null;
    try {
      observer?.observe({ type: "longtask", buffered: false });
    } catch {
      // Safari/WebKit does not expose the longtask entry type.
    }

    viewport.scrollTop = 0;
    await twoFrames();
    const scrollSample = await sampleScroll(viewport, variant);
    const frameTimes = scrollSample.frameTimes;
    const collapseTimes: number[] = [];
    for (let pass = 0; pass < 10; pass += 1) {
      collapseTimes.push(await measurePaint(() => toggleCollapsed("phase-1")));
      collapseTimes.push(await measurePaint(() => toggleCollapsed("phase-1")));
    }
    const zoomTimes: number[] = [];
    const originalZoom = zoom;
    for (let pass = 0; pass < 5; pass += 1) {
      for (const nextZoom of zoomOrder) {
        zoomTimes.push(await measurePaint(() => changeZoom(nextZoom)));
      }
    }
    await measurePaint(() => changeZoom(originalZoom));
    observer?.disconnect();

    const p95 = percentile(frameTimes, 0.95);
    setResult({
      recordedAt: new Date().toISOString(),
      fixtureId: SPIKE_FIXTURE_ID,
      variant,
      userAgent: navigator.userAgent,
      viewport: `${viewport.clientWidth}x${viewport.clientHeight}`,
      devicePixelRatio: window.devicePixelRatio,
      visibleRows: visibleRows.length,
      initialPaintMs: initialPaintMs.current ?? lastPaintMs,
      taskNodes: document.querySelectorAll(
        ".schedule-variant [data-row-id], .schedule-variant [data-timeline-row-id], .schedule-variant .inline-task, .schedule-variant .inline-baseline, .schedule-variant .dependency",
      ).length,
      scrollP95Ms: round(p95),
      scrollP99Ms: round(percentile(frameTimes, 0.99)),
      droppedFramePercent: round((frameTimes.filter((value) => value > 25).length / frameTimes.length) * 100),
      collapseP95Ms: round(percentile(collapseTimes, 0.95)),
      zoomP95Ms: round(percentile(zoomTimes, 0.95)),
      maximumVerticalDriftPx: round(scrollSample.maximumVerticalDriftPx),
      longTasksOver50Ms: longTasks.filter((value) => value > 50).length,
    });
    viewport.scrollTop = 0;
    setBenchmarking(false);
  }

  function exportResult() {
    if (!result) return;
    const blob = new Blob([JSON.stringify(result, null, 2)], { type: "application/json" });
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = `${SPIKE_FIXTURE_ID}-${variant}.json`;
    link.click();
    URL.revokeObjectURL(link.href);
  }

  return (
    <div className="prototype-shell">
      <header className="prototype-header">
        <div>
          <p className="prototype-eyebrow">Throwaway experiment · issue #1</p>
          <h1 id="variant-title">{currentVariant.name}</h1>
          <p>{currentVariant.hypothesis}</p>
        </div>
        <div className="prototype-facts" aria-label="Prototype facts">
          <span>{SPIKE_FIXTURE_ID}</span>
          <span>{visibleRows.length.toLocaleString()} visible rows</span>
          <span>{lastPaintMs}ms last painted update</span>
        </div>
      </header>

      <main>
      <div className="prototype-toolbar" aria-label="Gantt prototype controls">
        <fieldset>
          <legend>Zoom</legend>
          {zoomOrder.map((option) => (
            <button
              type="button"
              key={option}
              aria-pressed={zoom === option}
              onClick={() => changeZoom(option)}
            >
              {option}
            </button>
          ))}
        </fieldset>
        <button type="button" onClick={() => toggleCollapsed("phase-1")}>
          {collapsedIds.has("phase-1") ? "Expand" : "Collapse"} phase 1
        </button>
        <label className="prototype-checkbox">
          <input type="checkbox" checked={dark} onChange={(event) => setDark(event.target.checked)} />
          Dark theme
        </label>
        <button type="button" className="benchmark-action" onClick={runBenchmark} disabled={benchmarking}>
          {benchmarking ? "Running benchmark…" : "Run benchmark"}
        </button>
      </div>

      {variant === "shared-svg" ? (
        <SharedViewportSvg
          rows={visibleRows}
          allRows={fixture}
          collapsedIds={collapsedIds}
          toggleCollapsed={toggleCollapsed}
          zoom={zoom}
          viewportRef={viewportRef}
          horizontalViewportRef={horizontalViewportRef}
        />
      ) : variant === "split-canvas" ? (
        <SplitPaneCanvas
          rows={visibleRows}
          collapsedIds={collapsedIds}
          toggleCollapsed={toggleCollapsed}
          zoom={zoom}
          viewportRef={viewportRef}
          horizontalViewportRef={horizontalViewportRef}
        />
      ) : (
        <RowEmbeddedDom
          rows={visibleRows}
          collapsedIds={collapsedIds}
          toggleCollapsed={toggleCollapsed}
          zoom={zoom}
          viewportRef={viewportRef}
          horizontalViewportRef={horizontalViewportRef}
        />
      )}

      <section className="benchmark-panel" aria-labelledby="benchmark-title" aria-live="polite">
        <div>
          <p className="prototype-eyebrow">Repeatable local evidence</p>
          <h2 id="benchmark-title">Benchmark result</h2>
        </div>
        {result ? (
          <>
            <dl>
              <Metric label="Scroll p95" value={`${result.scrollP95Ms}ms`} />
              <Metric label="Dropped frames" value={`${result.droppedFramePercent}%`} />
              <Metric label="Collapse p95" value={`${result.collapseP95Ms}ms`} />
              <Metric label="Zoom p95" value={`${result.zoomP95Ms}ms`} />
              <Metric label="Maximum drift" value={`${result.maximumVerticalDriftPx}px`} />
              <Metric label="Task nodes" value={result.taskNodes.toLocaleString()} />
              <Metric label="Long tasks" value={String(result.longTasksOver50Ms)} />
            </dl>
            <button type="button" onClick={exportResult}>Export JSON</button>
            <output hidden data-testid="benchmark-json">
              {JSON.stringify(result)}
            </output>
          </>
        ) : (
          <p>Run one warm-up per variant, then record five optimized-build passes.</p>
        )}
      </section>
      </main>

      <nav className="prototype-switcher" aria-label="Prototype variants">
        <button type="button" onClick={() => selectRelativeVariant(-1)} aria-label="Previous variant">←</button>
        <span><strong>{currentVariant.key}</strong> · {currentVariant.name}</span>
        <button type="button" onClick={() => selectRelativeVariant(1)} aria-label="Next variant">→</button>
      </nav>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function readVariant(): VariantKey {
  const value = new URLSearchParams(window.location.search).get("variant");
  return variants.some((variant) => variant.key === value) ? (value as VariantKey) : "shared-svg";
}

async function sampleScroll(
  viewport: HTMLDivElement,
  variant: VariantKey,
): Promise<{ frameTimes: number[]; maximumVerticalDriftPx: number }> {
  const frameTimes: number[] = [];
  let maximumVerticalDriftPx = 0;
  const maximum = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
  let previous = performance.now();
  for (let frame = 0; frame < 180; frame += 1) {
    await nextFrame();
    const now = performance.now();
    frameTimes.push(now - previous);
    maximumVerticalDriftPx = Math.max(maximumVerticalDriftPx, measureVerticalDrift(variant));
    previous = now;
    const progress = frame / 179;
    viewport.scrollTop = maximum * (progress < 0.5 ? progress * 2 : (1 - progress) * 2);
  }
  return { frameTimes, maximumVerticalDriftPx };
}

async function measurePaint(update: () => void): Promise<number> {
  const started = performance.now();
  update();
  await twoFrames();
  return performance.now() - started;
}

function measureVerticalDrift(variant: VariantKey): number {
  if (variant === "split-canvas") {
    const scrollports = Array.from(document.querySelectorAll<HTMLElement>(".split-scrollport"));
    return scrollports.length === 2
      ? Math.abs(scrollports[0]!.scrollTop - scrollports[1]!.scrollTop)
      : 0;
  }
  const tableRow = document.querySelector<HTMLElement>("[data-row-id]");
  if (!tableRow) return 0;
  const rowId = tableRow.dataset.rowId;
  if (!rowId) return 0;
  const markerSelector =
    variant === "row-dom"
      ? `.inline-row-marker[data-timeline-row-id="${CSS.escape(rowId)}"]`
      : `.timeline-row-rule[data-timeline-row-id="${CSS.escape(rowId)}"]`;
  const timelineRow = document.querySelector<SVGGraphicsElement | HTMLElement>(markerSelector);
  if (!timelineRow) return 0;
  if (variant === "row-dom") {
    return Math.abs(tableRow.getBoundingClientRect().top - timelineRow.getBoundingClientRect().top);
  }
  return Math.abs(tableRow.getBoundingClientRect().bottom - timelineRow.getBoundingClientRect().top);
}

function percentile(values: readonly number[], fraction: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))]!;
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

async function twoFrames(): Promise<void> {
  await nextFrame();
  await nextFrame();
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}
