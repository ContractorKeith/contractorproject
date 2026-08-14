import { useEffect, useMemo, useState } from "react";

import { GanttTreegrid } from "./GanttTreegrid";
import {
  createGanttVerificationReadModel,
  GANTT_VERIFICATION_FIXTURE_ID,
} from "./verificationFixture";

interface PackagedBenchmarkResult {
  initialPaintMs: number;
  scrollP95Ms: number;
  scrollP99Ms: number;
  framesAbove25Percent: number;
  maximumVerticalDriftPx: number;
  taskRelatedNodeCount: number;
  medianLongTasksAbove50Ms: number;
}

type BenchmarkState =
  | { status: "idle" }
  | { status: "running" }
  | { status: "complete"; result: PackagedBenchmarkResult }
  | { status: "error"; message: string };

export function GanttVerificationApp() {
  const readModel = useMemo(createGanttVerificationReadModel, []);
  const [initialPaintMs, setInitialPaintMs] = useState<number | null>(null);
  const [benchmark, setBenchmark] = useState<BenchmarkState>({
    status: "idle",
  });
  const commit = import.meta.env.VITE_APP_COMMIT || "unrecorded";

  useEffect(() => {
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => setInitialPaintMs(performance.now()));
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, []);

  async function handleBenchmark() {
    setBenchmark({ status: "running" });
    try {
      setBenchmark({
        status: "complete",
        result: await runPackagedBenchmark(initialPaintMs ?? performance.now()),
      });
    } catch (reason: unknown) {
      setBenchmark({
        status: "error",
        message:
          reason instanceof Error
            ? reason.message
            : "Packaged benchmark failed",
      });
    }
  }

  return (
    <main className="gantt-verification">
      <header className="gantt-verification__header">
        <div>
          <p className="eyebrow">Packaged platform verification</p>
          <h1>1,000-task production Gantt</h1>
          <p>
            This deterministic fixture exercises the production treegrid and
            supplemental SVG adapter. It contains no customer data and is
            excluded from normal application builds.
          </p>
        </div>
        <dl
          className="gantt-verification__metadata"
          aria-label="Verification artifact metadata"
        >
          <div>
            <dt>Fixture</dt>
            <dd>{GANTT_VERIFICATION_FIXTURE_ID}</dd>
          </div>
          <div>
            <dt>Commit</dt>
            <dd>{commit}</dd>
          </div>
          <div>
            <dt>Contract</dt>
            <dd>Gantt read model v{readModel.contractVersion}</dd>
          </div>
          <div>
            <dt>Rows</dt>
            <dd>{readModel.rowCount}</dd>
          </div>
          <div className="gantt-verification__user-agent">
            <dt>Webview user agent</dt>
            <dd>{navigator.userAgent}</dd>
          </div>
        </dl>
      </header>

      <section aria-labelledby="verification-schedule-heading">
        <div className="gantt-verification__section-heading">
          <div>
            <p className="eyebrow">Assistive-technology surface</p>
            <h2 id="verification-schedule-heading">
              Schedule verification fixture
            </h2>
          </div>
          <button
            type="button"
            onClick={() => void handleBenchmark()}
            disabled={benchmark.status === "running" || initialPaintMs === null}
          >
            {benchmark.status === "running"
              ? "Running benchmark…"
              : "Run packaged benchmark"}
          </button>
        </div>
        <GanttTreegrid
          readModel={readModel}
          ariaLabel="Packaged Gantt verification schedule"
          viewportHeight={520}
        />
      </section>

      <section
        className="gantt-verification__result"
        aria-labelledby="benchmark-heading"
      >
        <h2 id="benchmark-heading">Packaged benchmark</h2>
        {benchmark.status === "idle" ? (
          <p>
            Run after the packaged app is stable and no screen reader is
            speaking.
          </p>
        ) : null}
        {benchmark.status === "running" ? (
          <p role="status">Measuring one warm-up and five recorded passes…</p>
        ) : null}
        {benchmark.status === "error" ? (
          <p role="alert">{benchmark.message}</p>
        ) : null}
        {benchmark.status === "complete" ? (
          <output aria-live="polite">
            <BenchmarkResult result={benchmark.result} />
          </output>
        ) : null}
      </section>
    </main>
  );
}

function BenchmarkResult({ result }: { result: PackagedBenchmarkResult }) {
  return (
    <dl className="gantt-verification__metrics">
      {Object.entries(result).map(([name, value]) => (
        <div key={name}>
          <dt>{name}</dt>
          <dd>{formatMetric(value)}</dd>
        </div>
      ))}
    </dl>
  );
}

function formatMetric(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2);
}

async function runPackagedBenchmark(
  initialPaintMs: number,
): Promise<PackagedBenchmarkResult> {
  const scrollport = document.querySelector<HTMLElement>(
    ".gantt-treegrid-scrollport",
  );
  if (!scrollport) throw new Error("Gantt scrollport is missing");
  const passes: Array<{
    p95: number;
    p99: number;
    framesAbove25Percent: number;
    maximumDrift: number;
    nodeCount: number;
    longTasks: number;
  }> = [];

  for (let pass = 0; pass < 6; pass += 1) {
    const samples: number[] = [];
    const longTasks: PerformanceEntry[] = [];
    const observer =
      typeof PerformanceObserver === "undefined"
        ? null
        : new PerformanceObserver((list) =>
            longTasks.push(...list.getEntries()),
          );
    observer?.observe({ type: "longtask" });
    let maximumDrift = 0;
    let previous = performance.now();
    const maximumScroll = scrollport.scrollHeight - scrollport.clientHeight;

    for (let frame = 0; frame < 120; frame += 1) {
      const progress = frame < 60 ? frame / 59 : (119 - frame) / 59;
      scrollport.scrollTop = maximumScroll * progress;
      await animationFrame();
      const now = performance.now();
      samples.push(now - previous);
      previous = now;
      maximumDrift = Math.max(maximumDrift, measureMaximumDrift(scrollport));
    }
    observer?.disconnect();
    scrollport.scrollTop = 0;

    if (pass > 0) {
      samples.sort((left, right) => left - right);
      passes.push({
        p95: percentile(samples, 0.95),
        p99: percentile(samples, 0.99),
        framesAbove25Percent:
          (samples.filter((sample) => sample > 25).length / samples.length) *
          100,
        maximumDrift,
        nodeCount: scrollport.querySelectorAll(
          "tr[data-task-id], [data-timeline-row-id], [data-timeline-task-id], [data-baseline-task-id], [data-dependency]",
        ).length,
        longTasks: longTasks.length,
      });
    }
  }

  return {
    initialPaintMs,
    scrollP95Ms: median(passes.map((pass) => pass.p95)),
    scrollP99Ms: median(passes.map((pass) => pass.p99)),
    framesAbove25Percent: median(
      passes.map((pass) => pass.framesAbove25Percent),
    ),
    maximumVerticalDriftPx: Math.max(
      ...passes.map((pass) => pass.maximumDrift),
    ),
    taskRelatedNodeCount: Math.max(...passes.map((pass) => pass.nodeCount)),
    medianLongTasksAbove50Ms: median(passes.map((pass) => pass.longTasks)),
  };
}

function measureMaximumDrift(scrollport: HTMLElement): number {
  return Math.max(
    0,
    ...Array.from(
      scrollport.querySelectorAll<HTMLElement>("tr[data-task-id]"),
      (row) => {
        const id = row.dataset.taskId;
        const rule = scrollport.querySelector<SVGLineElement>(
          `.gantt-timeline__row-rule[data-timeline-row-id="${id}"]`,
        );
        return rule
          ? Math.abs(
              row.getBoundingClientRect().bottom -
                rule.getBoundingClientRect().top,
            )
          : 0;
      },
    ),
  );
}

function animationFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

function median(values: number[]): number {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.floor(ordered.length / 2)] ?? 0;
}

function percentile(values: number[], value: number): number {
  return values[Math.max(0, Math.ceil(values.length * value) - 1)] ?? 0;
}
