import { expect, test } from "@playwright/test";

const allVariants = ["shared-svg", "split-canvas", "row-dom"] as const;
const requestedVariant = process.env.GANTT_BENCHMARK_VARIANT;
const variants = allVariants.filter((variant) => !requestedVariant || variant === requestedVariant);

interface BenchmarkResult {
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

test("@benchmark records repeatable optimized-build evidence", async ({ page }) => {
  test.setTimeout(10 * 60_000);
  const evidence: Record<string, BenchmarkResult[]> = {};

  for (const variant of variants) {
    evidence[variant] = [];

    for (let pass = 0; pass < 6; pass += 1) {
      await page.goto(`/gantt-prototype.html?variant=${variant}`);
      const runButton = page.getByRole("button", { name: "Run benchmark" });
      await runButton.click();
      await expect(page.getByRole("button", { name: "Running benchmark…" })).toBeDisabled();
      await expect(runButton).toBeEnabled({ timeout: 60_000 });
      const output = page.getByTestId("benchmark-json");
      await output.waitFor({ state: "attached", timeout: 60_000 });
      const raw = await output.textContent();
      if (!raw) throw new Error(`Missing ${variant} benchmark output`);
      if (pass > 0) {
        const result = JSON.parse(raw) as BenchmarkResult;
        evidence[variant].push(result);
      }
    }

    if (variant === "shared-svg") {
      const recorded = evidence[variant]!;
      expect(median(recorded, "initialPaintMs")).toBeLessThanOrEqual(750);
      expect(median(recorded, "taskNodes")).toBeLessThan(500);
      expect(median(recorded, "scrollP95Ms")).toBeLessThanOrEqual(25);
      expect(median(recorded, "scrollP99Ms")).toBeLessThanOrEqual(100);
      expect(median(recorded, "droppedFramePercent")).toBeLessThan(5);
      expect(median(recorded, "collapseP95Ms")).toBeLessThanOrEqual(100);
      expect(median(recorded, "zoomP95Ms")).toBeLessThanOrEqual(100);
      expect(Math.max(...recorded.map((result) => result.maximumVerticalDriftPx))).toBeLessThanOrEqual(1);
      expect(median(recorded, "longTasksOver50Ms")).toBe(0);
    }
  }

  console.log(`GANTT_BENCHMARK ${JSON.stringify(evidence)}`);
});

function median(results: readonly BenchmarkResult[], key: keyof BenchmarkResult) {
  const values = results.map((result) => result[key]).sort((left, right) => left - right);
  return values[Math.floor(values.length / 2)]!;
}
