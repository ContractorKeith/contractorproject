import { expect, test, type Locator } from "@playwright/test";

interface PassResult {
  p95: number;
  p99: number;
  framesAbove25Percent: number;
  maximumDrift: number;
  taskNodeCount: number;
  longTasks: number;
}

test("keeps the 1,000-row shared timeline within the ADR regression floor", async ({ page }) => {
  const navigationStart = performance.now();
  await page.goto("/tests/browser/gantt.html");
  await expect(page.locator('g[data-timeline-row-id="phase-1"]')).toBeVisible();
  const initialPaint = performance.now() - navigationStart;

  const passes: PassResult[] = [];
  for (let pass = 0; pass < 6; pass += 1) {
    const result = await page.evaluate(async () => {
      const scrollport = document.querySelector<HTMLElement>(".gantt-treegrid-scrollport");
      if (!scrollport) throw new Error("Gantt scrollport is missing");
      const samples: number[] = [];
      const longTasks: PerformanceEntry[] = [];
      const observer =
        typeof PerformanceObserver === "undefined"
          ? null
          : new PerformanceObserver((list) => longTasks.push(...list.getEntries()));
      observer?.observe({ type: "longtask" });
      let maximumDrift = 0;
      let previous = performance.now();
      const maximumScroll = scrollport.scrollHeight - scrollport.clientHeight;

      for (let frame = 0; frame < 120; frame += 1) {
        const progress = frame < 60 ? frame / 59 : (119 - frame) / 59;
        scrollport.scrollTop = maximumScroll * progress;
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        const now = performance.now();
        samples.push(now - previous);
        previous = now;

        for (const row of Array.from(scrollport.querySelectorAll<HTMLElement>("tr[data-task-id]"))) {
          const id = row.dataset.taskId;
          const rule = scrollport.querySelector<SVGLineElement>(
            `.gantt-timeline__row-rule[data-timeline-row-id="${id}"]`,
          );
          if (rule) {
            maximumDrift = Math.max(
              maximumDrift,
              Math.abs(row.getBoundingClientRect().bottom - rule.getBoundingClientRect().top),
            );
          }
        }
      }
      observer?.disconnect();
      samples.sort((left, right) => left - right);
      const percentile = (value: number) => samples[Math.max(0, Math.ceil(samples.length * value) - 1)] ?? 0;
      return {
        p95: percentile(0.95),
        p99: percentile(0.99),
        framesAbove25Percent: (samples.filter((sample) => sample > 25).length / samples.length) * 100,
        maximumDrift,
        // Node budget scope: this counts mounted ROWS and per-row TIMELINE marks
        // (row groups, task/baseline/progress bars, dependency paths) plus the
        // non-working shading rects (one per merged run over the domain) — not the
        // per-cell text spans inside the treegrid. Stacked baseline facts add
        // text spans, not counted nodes, so the budget stays honest about DOM
        // rows/marks that drive scroll/paint cost.
        taskNodeCount: scrollport.querySelectorAll(
          "tr[data-task-id], [data-timeline-row-id], [data-timeline-task-id], [data-baseline-task-id], [data-progress-task-id], [data-dependency], [data-nonworking]",
        ).length,
        longTasks: longTasks.length,
      };
    });
    if (pass > 0) passes.push(result);
  }

  const collapseSamples: number[] = [];
  for (let index = 0; index < 10; index += 1) {
    collapseSamples.push(await measureClick(page.locator('[aria-label$="Phase 1"]')));
  }
  const zoomSamples: number[] = [];
  for (let index = 0; index < 10; index += 1) {
    const label = index % 2 === 0 ? "Day" : "Week";
    const button = page.getByRole("button", { name: label, exact: true });
    zoomSamples.push(await measureClick(button));
    await expect(button).toHaveAttribute("aria-pressed", "true");
  }

  const evidence = {
    initialPaint,
    scrollP95: median(passes.map((pass) => pass.p95)),
    scrollP99: median(passes.map((pass) => pass.p99)),
    framesAbove25Percent: median(passes.map((pass) => pass.framesAbove25Percent)),
    maximumDrift: Math.max(...passes.map((pass) => pass.maximumDrift)),
    taskNodeCount: Math.max(...passes.map((pass) => pass.taskNodeCount)),
    longTasks: median(passes.map((pass) => pass.longTasks)),
    collapseP95: percentile(collapseSamples, 0.95),
    zoomP95: percentile(zoomSamples, 0.95),
    passes,
  };
  console.log(`GANTT_BENCHMARK ${JSON.stringify(evidence)}`);

  expect(evidence.initialPaint).toBeLessThanOrEqual(750);
  expect(evidence.scrollP95).toBeLessThanOrEqual(25);
  expect(evidence.scrollP99).toBeLessThanOrEqual(100);
  expect(evidence.framesAbove25Percent).toBeLessThan(5);
  expect(evidence.maximumDrift).toBeLessThanOrEqual(1);
  expect(evidence.taskNodeCount).toBeLessThan(500);
  expect(evidence.longTasks).toBe(0);
  expect(evidence.collapseP95).toBeLessThanOrEqual(100);
  expect(evidence.zoomP95).toBeLessThanOrEqual(100);
});

function median(values: number[]): number {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.floor(ordered.length / 2)] ?? 0;
}

function percentile(values: number[], value: number): number {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.max(0, Math.ceil(ordered.length * value) - 1)] ?? 0;
}

async function measureClick(locator: Locator): Promise<number> {
  return locator.evaluate(async (element) => {
    const target = element as HTMLElement;
    const start = performance.now();
    target.click();
    await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    return performance.now() - start;
  });
}
