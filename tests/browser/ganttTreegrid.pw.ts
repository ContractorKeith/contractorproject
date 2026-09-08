import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/browser/gantt.html");
});

test("virtualizes 1,000 logical rows without accessibility violations", async ({ page }) => {
  const treegrid = page.getByRole("treegrid", {
    name: "Work breakdown schedule",
  });
  await expect(treegrid).toHaveAttribute("aria-rowcount", "1001");
  await expect(treegrid.locator('tr[data-task-id="phase-1"]')).toBeVisible();

  const mountedTaskRows = await treegrid.locator("tbody tr[data-task-id]").count();
  expect(mountedTaskRows).toBeLessThan(50);

  const rowGeometry = await treegrid.evaluate((grid) => ({
    configured: Number.parseFloat(getComputedStyle(grid).getPropertyValue("--row-h")),
    actual: Array.from(
      grid.querySelectorAll<HTMLElement>("tr[data-task-id]"),
      (row) => row.getBoundingClientRect().height,
    ).slice(0, 5),
  }));
  expect(rowGeometry.actual).not.toHaveLength(0);
  for (const actualHeight of rowGeometry.actual) {
    expect(actualHeight).toBeCloseTo(rowGeometry.configured, 1);
  }

  const mountedTimelineRows = page.locator('[data-testid="gantt-timeline"] [data-timeline-row-id]').filter({
    has: page.locator(".gantt-timeline__row-rule"),
  });
  await expect(mountedTimelineRows).toHaveCount(mountedTaskRows);
  const mountedIds = await page.getByTestId("gantt-scrollport").evaluate((scrollport) => ({
    table: Array.from(scrollport.querySelectorAll<HTMLElement>("tr[data-task-id]"), (row) => row.dataset.taskId),
    timeline: Array.from(
      scrollport.querySelectorAll<SVGGElement>('g[data-timeline-row-id]'),
      (row) => row.dataset.timelineRowId,
    ),
  }));
  expect(mountedIds.timeline).toEqual(mountedIds.table);
  expect(await page.locator('[data-testid="gantt-timeline"] *').count()).toBeLessThan(500);

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("compact columns keep names readable and preserve the active task when details hide", async ({ page }) => {
  const task = page.locator('tr[data-task-id="phase-1-task-1"]');
  await task.getByRole("gridcell").last().click();
  await page.getByRole("button", { name: "Hide schedule details" }).click();
  await expect(page.getByRole("treegrid")).toHaveAttribute("aria-colcount", "6");
  await expect(task.locator('[tabindex="0"]')).toHaveCount(1);

  for (const width of [1440, 760]) {
    await page.setViewportSize({ width, height: 900 });
    const name = task.locator(".gantt-treegrid__task-copy > span").first();
    await expect(name).toHaveText("Activity 1");
    const fit = await name.evaluate((element) => ({ width: element.clientWidth, textWidth: element.scrollWidth }));
    expect(fit.width).toBeGreaterThanOrEqual(90);
    expect(fit.textWidth).toBeLessThanOrEqual(fit.width);
    const heights = await task.evaluate((row) => ({
      actual: row.getBoundingClientRect().height,
      configured: parseFloat(getComputedStyle(row).getPropertyValue("--row-h")),
    }));
    expect(heights.actual).toBeCloseTo(heights.configured, 1);
  }
  await task.locator('[tabindex="0"]').focus();
  await page.keyboard.press("End");
  await expect(task.getByRole("gridcell").last()).toBeFocused();
  await page.getByRole("button", { name: "Show schedule details" }).click();
  await expect(page.getByRole("treegrid")).toHaveAttribute("aria-colcount", "8");
  await expect(task.locator('[tabindex="0"]')).toHaveCount(1);
});

test("announces visible constraint values and direct violations without color-only state", async ({ page }) => {
  await page.locator(".gantt-schedule").evaluate((schedule) => {
    (schedule as HTMLElement).style.setProperty("--font-heading", "system-ui, sans-serif");
  });
  for (const width of [1100, 760]) {
    await page.setViewportSize({ width, height: 700 });
    if (width === 760) {
      await page.getByTestId("gantt-scrollport").evaluate((scrollport) => {
        const schedule = scrollport.closest<HTMLElement>(".gantt-schedule");
        if (!schedule) throw new Error("Schedule surface is missing");
        schedule.style.width = "724px";
      });
    }
    const constrainedName = page.getByRole("rowheader", {
      name: /1\.1 Activity 1, task, activity, critical, start no earlier than 2026-08-18, finish no later than 2026-08-17, constraint violated/,
    });
    await expect(constrainedName).toBeVisible();
    const constrainedRow = page.locator('tr[data-task-id="phase-1-task-1"]');
    const visibleFacts = constrainedRow.locator("[data-testid='schedule-current'], [data-testid='schedule-baseline'], [data-testid='schedule-constraint'], .gantt-treegrid__constraint-float");
    await expect(visibleFacts.nth(0)).toHaveText("2026-08-17 08:00");
    await expect(visibleFacts.nth(1)).toHaveText("Baseline 2026-08-14 +4,320 min");
    await expect(visibleFacts.nth(2)).toHaveText("≥ 2026-08-18");
    await expect(constrainedRow.getByTestId("constraint-float-phase-1-task-1")).toHaveText(/Constraint\s*violated/);
    // The Duration cell surfaces the Rust-derived baseline duration variance fact
    // (unit dropped from the first number to shorten the narrow-cell fragment).
    const durationBaseline = constrainedRow.getByTestId("duration-baseline-phase-1-task-1");
    await expect(durationBaseline).toHaveText("Baseline 360 +120 min");

    // Milestone rows suppress the duration fact (they carry start/finish baselines).
    const milestoneDuration = page.locator('tr[data-task-id="phase-1-task-99"] [data-testid="duration-baseline-phase-1-task-99"]');
    await expect(milestoneDuration).toHaveCount(0);

    // measure() reports line-box geometry under forced system-ui. Wrapping text
    // fills the width (scrollWidth == clientWidth is meaningless), so the widest
    // rendered LINE is measured via range rects; the fact stretches to its grid
    // track so spare vertical lines are real and a wrap regression overflows.
    const measure = async (locator: import("@playwright/test").Locator) =>
      locator.evaluate((element) => {
        const line = parseFloat(getComputedStyle(element).lineHeight) || element.clientHeight;
        const range = document.createRange();
        range.selectNodeContents(element);
        const rects = Array.from(range.getClientRects());
        const widestLine = rects.length ? Math.max(...rects.map((rect) => rect.width)) : element.scrollWidth;
        // Count actual rendered lines by unique top (a stretched span makes
        // scrollHeight == clientHeight, so it cannot report the line count).
        const tops = new Set(rects.map((rect) => Math.round(rect.top)));
        const lineCount = Math.max(1, tops.size);
        const trackLines = Math.max(1, Math.round(element.clientHeight / line));
        return {
          clientWidth: element.clientWidth,
          widestLine: Math.round(widestLine * 10) / 10,
          widthHeadroom: Math.round(((element.clientWidth - widestLine) / element.clientWidth) * 1000) / 1000,
          lineCount,
          trackLines,
          spareLines: trackLines - lineCount,
        };
      });

    // Wrapping baseline facts must never overflow horizontally (widest rendered
    // line stays within the cell) AND keep >=1 spare vertical line, at BOTH the
    // default (720px pane) and the narrow (460px capped pane) layouts. Horizontal
    // "headroom" is not asserted on wrapping facts: greedy wrapping fills the
    // width by design, so the meaningful clip guard is vertical.
    const wrapping = {
      "schedule-baseline": visibleFacts.nth(1),
      "duration-baseline": durationBaseline,
    } as const;
    for (const [name, locator] of Object.entries(wrapping)) {
      const m = await measure(locator);
      console.log(`FACT_FIT ${width} ${name} ${JSON.stringify(m)}`);
      expect(m.widestLine, `${name} horizontal fit @${width}`).toBeLessThanOrEqual(m.clientWidth);
      expect(m.spareLines, `${name} spare lines @${width}`).toBeGreaterThanOrEqual(1);
      // Lock the stretched-track metric: the fact must fill a >=3-line track. If a
      // CSS change un-stretches the spans or drops the spare line, clientHeight
      // collapses to the content and this fails loudly instead of going tautological.
      expect(m.trackLines, `${name} stretched track >=3 lines @${width}`).toBeGreaterThanOrEqual(3);
    }
    const currentFit = await measure(visibleFacts.nth(0));
    console.log(`FACT_FIT ${width} schedule-current ${JSON.stringify(currentFit)}`);
    expect(currentFit.widthHeadroom, `schedule-current width headroom @${width}`).toBeGreaterThanOrEqual(0.05);
    expect(currentFit.lineCount, `schedule-current vertical fit @${width}`).toBeLessThanOrEqual(currentFit.trackLines);

    // Summary rows render the heaviest Start/Finish weight, so measure the
    // summary Start fact too: hierarchy emphasis now lives on the Name cell only,
    // so the summary date fact keeps the same >=5% single-line width headroom.
    const summaryCurrent = await measure(
      page.locator('tr[data-task-id="phase-1"] [data-testid="schedule-current"]').nth(0),
    );
    console.log(`FACT_FIT ${width} summary-current ${JSON.stringify(summaryCurrent)}`);
    expect(summaryCurrent.widthHeadroom, `summary schedule-current width headroom @${width}`).toBeGreaterThanOrEqual(0.05);
    expect(summaryCurrent.lineCount, `summary schedule-current vertical fit @${width}`).toBeLessThanOrEqual(summaryCurrent.trackLines);

    // Constraint facts (single line each) must remain visible and unclipped.
    const factDimensions = await visibleFacts.evaluateAll((elements) => elements.map((element) => ({
      fact: element.getAttribute("data-testid") ?? element.className,
      clientWidth: element.clientWidth,
      scrollWidth: element.scrollWidth,
      clientHeight: element.clientHeight,
      scrollHeight: element.scrollHeight,
      visible: element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0,
    })));
    for (const dimensions of factDimensions) {
      expect(dimensions.visible).toBe(true);
      expect(dimensions.scrollWidth, `${dimensions.fact} horizontal fit`).toBeLessThanOrEqual(dimensions.clientWidth);
      expect(dimensions.scrollHeight, `${dimensions.fact} vertical fit`).toBeLessThanOrEqual(dimensions.clientHeight);
    }
  }

  const float = page.getByRole("gridcell", {
    name: /1\.1 Activity 1, total float, 0 min, critical, constraint violated/,
  });
  await expect(float).toHaveText(/Constraint\s*violated/);
});

test("announces progress facts, draws the data-date marker, and stays accessible", async ({ page }) => {
  // The % Done column exposes visible percent text and an accessible progress fact.
  const completeName = page.getByRole("gridcell", {
    name: /1\.1 Activity 1, percent complete, 100 percent complete, complete/,
  });
  await expect(completeName).toBeVisible();
  await expect(page.getByTestId("progress-phase-1-task-1")).toHaveText(/100%/);

  const runningName = page.getByRole("gridcell", {
    name: /1\.4 Activity 4, percent complete, 50 percent complete, in progress/,
  });
  await expect(runningName).toBeVisible();

  // The in-progress bar carries a proportional fill overlay.
  const bar = page.locator('[data-timeline-task-id="phase-1-task-4"]');
  const fill = page.locator('[data-progress-task-id="phase-1-task-4"]');
  await expect(fill).toHaveCount(1);
  const proportion = await page.evaluate(() => {
    const barRect = document
      .querySelector<SVGRectElement>('[data-timeline-task-id="phase-1-task-4"]')!
      .getBBox();
    const fillRect = document
      .querySelector<SVGRectElement>('[data-progress-task-id="phase-1-task-4"]')!
      .getBBox();
    return fillRect.width / barRect.width;
  });
  expect(proportion).toBeGreaterThan(0.45);
  expect(proportion).toBeLessThan(0.55);
  await expect(bar).toHaveCount(1);

  // The vertical data-date marker has a visible label and an accessible name.
  const marker = page.getByTestId("gantt-data-date-marker");
  await expect(marker).toBeVisible();
  await expect(marker).toHaveText(/Data date \d{4}-\d{2}-\d{2}/);
  await expect(marker).toHaveAccessibleName(/Data date \d{4}-\d{2}-\d{2}/);

  // Today is injected by the harness, including when it is a non-working Sunday.
  const todayMarker = page.getByTestId("gantt-today-marker");
  await expect(todayMarker).toBeVisible();
  await expect(todayMarker).toHaveText("Today 2026-08-23");
  await expect(todayMarker).toHaveAccessibleName("Today 2026-08-23");
  const markerStyles = await page.evaluate(() => {
    const dataDate = document.querySelector<HTMLElement>('[data-testid="gantt-data-date-marker"]')!;
    const today = document.querySelector<HTMLElement>('[data-testid="gantt-today-marker"]')!;
    return {
      dataDateLine: getComputedStyle(dataDate).borderLeftStyle,
      todayLine: getComputedStyle(today).borderLeftStyle,
    };
  });
  expect(markerStyles.dataDateLine).toBe("solid");
  expect(markerStyles.todayLine).toBe("dashed");
  const labelsOverlap = await page.evaluate(() => {
    const dataDate = document.querySelector<HTMLElement>(".gantt-timeline__data-date-label")!.getBoundingClientRect();
    const today = document.querySelector<HTMLElement>(".gantt-timeline__today-label")!.getBoundingClientRect();
    return !(
      dataDate.right <= today.left ||
      today.right <= dataDate.left ||
      dataDate.bottom <= today.top ||
      today.bottom <= dataDate.top
    );
  });
  expect(labelsOverlap).toBe(false);

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("keeps the complete percent numeral visible at supported narrow layouts", async ({ page }) => {
  await page.locator(".gantt-schedule").evaluate((schedule) => {
    (schedule as HTMLElement).style.setProperty("--font-heading", "system-ui, sans-serif");
  });
  const progress = page.getByTestId("progress-phase-1-task-1");
  const numeral = progress.locator(".gantt-treegrid__progress-numeral");

  for (const width of [1100, 760]) {
    await page.setViewportSize({ width, height: 700 });
    if (width === 760) {
      await page.getByTestId("gantt-scrollport").evaluate((scrollport) => {
        const schedule = scrollport.closest<HTMLElement>(".gantt-schedule");
        if (!schedule) throw new Error("Schedule surface is missing");
        schedule.style.width = "724px";
      });
    }
    await expect(numeral).toHaveText("100%");
    const fit = await numeral.evaluate((element) => {
      const cell = element.closest<HTMLElement>("td");
      if (!cell) throw new Error("Progress cell is missing");
      const range = document.createRange();
      range.selectNodeContents(element);
      const rect = range.getBoundingClientRect();
      const cellRect = cell.getBoundingClientRect();
      return {
        textWidth: rect.width,
        progressWidth: element.getBoundingClientRect().width,
        cellLeft: cellRect.left,
        cellRight: cellRect.right,
        textLeft: rect.left,
        textRight: rect.right,
      };
    });
    expect(fit.progressWidth, `numeral width @${width}`).toBeGreaterThanOrEqual(fit.textWidth);
    expect(fit.textLeft, `numeral left fit @${width}`).toBeGreaterThanOrEqual(fit.cellLeft);
    expect(fit.textRight, `numeral right fit @${width}`).toBeLessThanOrEqual(fit.cellRight);
  }
});

test("filters with ancestor context, preserves saved collapse, and keeps keyboard focus valid", async ({ page }) => {
  const phase = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
  await phase.focus();
  await page.keyboard.press("Space");
  await expect(page.locator('tr[data-task-id="phase-1-task-50"]')).toHaveCount(0);

  const search = page.getByRole("searchbox", { name: "Search tasks or WBS" });
  await search.fill("1.50");
  await expect(page.locator('tr[data-task-id="phase-1"]')).toBeVisible();
  await expect(page.locator('tr[data-task-id="phase-1"]')).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator('tr[data-task-id="phase-1-task-50"]')).toBeVisible();
  await expect(page.getByText("1 result", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /Collapse Phase 1|Expand Phase 1/ })).toHaveCount(0);

  await phase.focus();
  await page.keyboard.press("Space");
  await expect(page.locator('tr[data-task-id="phase-1-task-50"]')).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("rowheader", { name: /1\.50 Activity 50, task/ })).toBeFocused();

  await page.getByRole("button", { name: "Reset" }).click();
  await expect(page.locator('tr[data-task-id="phase-1"]')).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator('tr[data-task-id="phase-1-task-50"]')).toHaveCount(0);

  await search.fill("does not exist");
  await expect(page.getByText("No matching tasks", { exact: true })).toBeVisible();
  await expect(page.getByText("0 results", { exact: true })).toBeVisible();
});

test("recovers virtualization and focus when filtering an offscreen task then no results", async ({ page }) => {
  const scrollport = page.getByTestId("gantt-scrollport");
  await scrollport.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.locator('tr[data-task-id="phase-10-task-99"]')).toBeVisible();

  const search = page.getByRole("searchbox", { name: "Search tasks or WBS" });
  await search.fill("10.99");
  await expect(page.getByText("1 result", { exact: true })).toBeVisible();
  const target = page.getByRole("rowheader", { name: /10\.99 Activity 99, task/ });
  await expect(target).toBeVisible();
  await target.focus();
  await expect(target).toBeFocused();
  expect(await page.locator("tbody tr[data-task-id]").count()).toBeLessThan(10);

  await search.fill("not-a-task");
  await expect(page.getByText("No matching tasks", { exact: true })).toBeVisible();
  await expect(page.locator('.gantt-treegrid [role="rowheader"][tabindex="0"], .gantt-treegrid [role="gridcell"][tabindex="0"]')).toHaveCount(0);

  await page.getByRole("button", { name: "Reset" }).click();
  await expect(page.locator('.gantt-treegrid [role="rowheader"][tabindex="0"], .gantt-treegrid [role="gridcell"][tabindex="0"]')).toHaveCount(1);
});

test("shades non-working time and distinguishes calendar exceptions", async ({ page }) => {
  // Weekly non-working days (weekends) and dated exceptions are both shaded, and
  // are distinguishable by their data attribute and accessible labeling.
  const weekly = page.locator('[data-nonworking="weekly"]');
  const exception = page.locator('[data-nonworking="exception"]');
  await expect(weekly.first()).toBeVisible();
  await expect(exception.first()).toBeVisible();

  // The fixture's Thu-Fri-Mon closure bridges the first weekend into one merged
  // exception rect carrying an accessible name listing the closure dates.
  const bridge = page.getByRole("img", { name: /Calendar exceptions? 2026-08-20/ });
  await expect(bridge).toHaveCount(1);
  await expect(bridge).toHaveAccessibleName(/2026-08-20/);
  await expect(bridge).toHaveAccessibleName(/2026-08-24/);

  // Weekly non-working rects are decorative (no accessible role/name).
  await expect(weekly.first()).toHaveAttribute("aria-hidden", "true");

  // A merged exception rect is wider than a lone weekend, proving adjacency merge.
  const geometry = await page.evaluate(() => {
    const exceptionRect = document
      .querySelector<HTMLElement>('[data-nonworking="exception"]')!
      .getBoundingClientRect();
    const weeklyRect = document
      .querySelector<HTMLElement>('[data-nonworking="weekly"]')!
      .getBoundingClientRect();
    return { exceptionWidth: exceptionRect.width, weeklyWidth: weeklyRect.width };
  });
  expect(geometry.exceptionWidth).toBeGreaterThan(geometry.weeklyWidth);

  // The fill actually paints: a neutral background at 50% opacity in light mode.
  const lightPaint = await weekly.first().evaluate((node) => {
    const style = getComputedStyle(node);
    return { background: style.backgroundColor, opacity: style.opacity };
  });
  expect(lightPaint.opacity).toBe("0.5");
  expect(lightPaint.background).not.toBe("rgba(0, 0, 0, 0)");
  expect(lightPaint.background).not.toBe("transparent");

  // The same fill paints in dark mode, using the dark theme's neutral token.
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  const darkPaint = await weekly.first().evaluate((node) => {
    const style = getComputedStyle(node);
    return { background: style.backgroundColor, opacity: style.opacity };
  });
  expect(darkPaint.opacity).toBe("0.5");
  expect(darkPaint.background).not.toBe("rgba(0, 0, 0, 0)");
  expect(darkPaint.background).not.toBe("transparent");
  expect(darkPaint.background).not.toBe(lightPaint.background);
  await page.evaluate(() => document.documentElement.removeAttribute("data-theme"));

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("renders no non-working shading at the widest zoom", async ({ page }) => {
  // Quarter zoom is too coarse to read a shaded day, so no rects are rendered.
  await page.getByRole("button", { name: "Quarter" }).click();
  await expect(page.locator("[data-nonworking]")).toHaveCount(0);
  // Returning to a finer zoom restores the shading.
  await page.getByRole("button", { name: "Day" }).click();
  await expect(page.locator('[data-nonworking="exception"]').first()).toBeVisible();
});

test("keeps non-working shading distinguishable under forced colors", async ({ page }) => {
  await page.emulateMedia({ forcedColors: "active" });
  await page.reload();
  const exception = page.locator('[data-nonworking="exception"]').first();
  await expect(exception).toBeVisible();
  // Forced colors strips the gray fill, so exception rects fall back to a dashed
  // system-colored border to stay perceptible and distinct from weekly runs.
  const borderStyle = await exception.evaluate(
    (node) => getComputedStyle(node).borderInlineStyle,
  );
  expect(borderStyle).toBe("dashed");
  const todayTreatment = await page.getByTestId("gantt-today-marker").evaluate((node) => {
    const style = getComputedStyle(node);
    return { lineStyle: style.borderLeftStyle, lineColor: style.borderLeftColor };
  });
  expect(todayTreatment.lineStyle).toBe("dashed");
  expect(todayTreatment.lineColor).not.toBe("transparent");
  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("keeps compact density synchronized with the virtual scroll model", async ({ page }) => {
  await page.evaluate(() => {
    document.querySelector<HTMLElement>(".gantt-schedule")?.style.setProperty("--row-h", "var(--row-h-compact)");
  });

  await expect
    .poll(async () =>
      page.locator(".gantt-treegrid").evaluate((grid) => ({
        configured: Number.parseFloat(getComputedStyle(grid).getPropertyValue("--row-h")),
        actual: grid.querySelector<HTMLElement>("tr[data-task-id]")?.getBoundingClientRect().height,
      })),
    )
    .toEqual({ configured: 50, actual: 50 });

  const geometry = await page.locator(".gantt-treegrid-scrollport").evaluate((scrollport) => ({
    scrollHeight: scrollport.scrollHeight,
    headerHeight: scrollport.querySelector<HTMLElement>("thead")?.getBoundingClientRect().height ?? 0,
  }));
  expect(geometry.scrollHeight - geometry.headerHeight).toBeCloseTo(50_000, 0);
});

test("moves cell focus and recovers it after an offscreen jump", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  await firstTaskCell.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("gridcell", { name: /1 Phase 1, duration/ })).toBeFocused();

  await page.keyboard.press("Control+End");
  const lastCell = page.getByRole("gridcell", {
    name: /10\.99 Activity 99, total float/,
  });
  await expect(lastCell).toBeFocused();
  await expect(lastCell.locator("xpath=ancestor::tr")).toHaveAttribute("aria-rowindex", "1001");

  await page.keyboard.press("Control+Home");
  await expect(page.getByRole("gridcell", { name: /1 Phase 1, WBS/ })).toBeFocused();
});

test("keeps horizontal cell focus in view", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 700 });
  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  await firstTaskCell.focus();
  await page.keyboard.press("End");

  const floatCell = page.getByRole("gridcell", {
    name: /1 Phase 1, total float/,
  });
  await expect(floatCell).toBeFocused();
  const geometry = await page.locator(".gantt-treegrid-scrollport").evaluate((scrollport) => {
    const cell = scrollport.querySelector<HTMLElement>('[role="gridcell"]:focus');
    if (!cell) throw new Error("Focused cell is missing");
    return {
      scrollLeft: scrollport.scrollLeft,
      cell: cell.getBoundingClientRect().toJSON(),
      scrollport: scrollport.getBoundingClientRect().toJSON(),
    };
  });
  expect(geometry.cell.left).toBeGreaterThanOrEqual(geometry.scrollport.left);
  expect(geometry.cell.right).toBeLessThanOrEqual(geometry.scrollport.right);
});

test("reserves a usable timeline pane at the supported minimum window width", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 700 });
  await page.getByTestId("gantt-scrollport").evaluate((scrollport) => {
    const schedule = scrollport.closest<HTMLElement>(".gantt-schedule");
    if (!schedule) throw new Error("Schedule surface is missing");
    schedule.style.width = "724px";
  });
  const geometry = await page.getByTestId("gantt-scrollport").evaluate((scrollport) => {
    const table = scrollport.querySelector<HTMLElement>(".gantt-treegrid-viewport");
    const timeline = scrollport.querySelector<HTMLElement>(".gantt-timeline-pane");
    if (!table || !timeline) throw new Error("Schedule panes are missing");
    const tableRect = table.getBoundingClientRect();
    return {
      tableWidth: tableRect.width,
      timelineWidth: scrollport.clientWidth - tableRect.width,
    };
  });
  expect(geometry.tableWidth).toBeLessThanOrEqual(460.5);
  expect(geometry.timelineWidth).toBeGreaterThanOrEqual(240);
});

test("moves a viewport page down and back while keeping focus visible", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  await firstTaskCell.focus();
  const pageStep = await page.getByTestId("gantt-scrollport").evaluate((scrollport) => {
    const styles = getComputedStyle(scrollport);
    const rowHeight = Number.parseFloat(styles.getPropertyValue("--row-h"));
    const viewportHeight = Number.parseFloat(styles.getPropertyValue("--gantt-viewport-height"));
    return Math.max(1, Math.floor(viewportHeight / rowHeight) - 1);
  });
  await page.keyboard.press("PageDown");

  const pagedCell = page.locator(`tr[aria-rowindex="${pageStep + 2}"] [role="rowheader"]`);
  await expect(pagedCell).toBeFocused();
  await expect(pagedCell).toBeInViewport();
  await expect(pagedCell).toHaveAccessibleName(new RegExp(`1\\.${pageStep} Activity ${pageStep}, task`));

  await page.keyboard.press("PageUp");
  await expect(firstTaskCell).toBeFocused();
  await expect(firstTaskCell).toBeInViewport();
});

test("collapses descendants without renumbering rows or losing focus", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  const phaseBar = page.locator('[data-timeline-task-id="phase-1"] rect');
  const phaseBarX = await phaseBar.getAttribute("x");
  await firstTaskCell.focus();
  await page.keyboard.press("Space");

  await expect(firstTaskCell).toBeFocused();
  await expect(firstTaskCell.locator("xpath=ancestor::tr")).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator('tr[data-task-id="phase-1-task-1"]')).toHaveCount(0);
  await expect(page.locator('[data-timeline-row-id="phase-1-task-1"]')).toHaveCount(0);
  await expect(page.locator('tr[data-task-id="phase-2"]')).toHaveAttribute("aria-rowindex", "102");
  await expect(phaseBar).toHaveAttribute("x", phaseBarX ?? "");
});

test("keeps every mounted timeline row aligned through scroll and compact density", async ({ page }) => {
  const maximumDrift = async () =>
    page.locator(".gantt-treegrid-scrollport").evaluate((scrollport) => {
      const tableRows = Array.from(scrollport.querySelectorAll<HTMLElement>("tr[data-task-id]"));
      return Math.max(
        0,
        ...tableRows.map((row) => {
          const id = row.dataset.taskId;
          const rule = scrollport.querySelector<SVGLineElement>(
            `.gantt-timeline__row-rule[data-timeline-row-id="${id}"]`,
          );
          if (!rule) return Number.POSITIVE_INFINITY;
          return Math.abs(row.getBoundingClientRect().bottom - rule.getBoundingClientRect().top);
        }),
      );
    });

  expect(await maximumDrift()).toBeLessThanOrEqual(1);
  await page.locator(".gantt-treegrid-scrollport").evaluate((element) => {
    element.scrollTop = 12_000;
  });
  await page.waitForTimeout(50);
  expect(await maximumDrift()).toBeLessThanOrEqual(1);
  const stickyGeometry = await page.getByTestId("gantt-scrollport").evaluate((scrollport) => ({
    scrollportTop: scrollport.getBoundingClientRect().top,
    tableHeaderTop: scrollport.querySelector("thead")?.getBoundingClientRect().top,
    rulerTop: scrollport.querySelector(".gantt-timeline__ruler")?.getBoundingClientRect().top,
  }));
  expect(Math.abs((stickyGeometry.tableHeaderTop ?? 0) - stickyGeometry.scrollportTop)).toBeLessThanOrEqual(1.1);
  expect(Math.abs((stickyGeometry.rulerTop ?? 0) - stickyGeometry.scrollportTop)).toBeLessThanOrEqual(1.1);

  await page.evaluate(() =>
    document.querySelector<HTMLElement>(".gantt-schedule")?.style.setProperty("--row-h", "var(--row-h-compact)"),
  );
  await page.waitForTimeout(50);
  expect(await maximumDrift()).toBeLessThanOrEqual(1);

  await page.getByTestId("gantt-scrollport").evaluate((element) => {
    const rowHeight = Number.parseFloat(getComputedStyle(element).getPropertyValue("--row-h"));
    element.scrollTop = rowHeight * 99;
  });
  const milestoneStack = await page.locator('[data-timeline-task-id="phase-1-task-99"]').evaluate((milestone) => {
    const baseline = document.querySelector<SVGRectElement>('[data-baseline-task-id="phase-1-task-99"]');
    if (!baseline) throw new Error("Milestone baseline is missing");
    return {
      milestoneBottom: milestone.getBoundingClientRect().bottom,
      baselineTop: baseline.getBoundingClientRect().top,
    };
  });
  expect(milestoneStack.milestoneBottom).toBeLessThan(milestoneStack.baselineTop);
});

test("keeps ruler, grid, and viewport-center date synchronized while scrolling and zooming", async ({ page }) => {
  const timeline = page.getByTestId("gantt-timeline-viewport");
  const scrollport = page.getByTestId("gantt-scrollport");
  await scrollport.evaluate((element) => {
    element.scrollLeft = 600;
  });

  const aligned = await timeline.evaluate((viewport) => {
    const tick = viewport.querySelector<HTMLElement>("[data-ruler-minute]");
    if (!tick) throw new Error("Ruler tick is missing");
    const minute = tick.dataset.rulerMinute;
    const line = viewport.querySelector<SVGLineElement>(`[data-grid-minute="${minute}"]`);
    if (!line) throw new Error("Matching grid line is missing");
    return Math.abs(tick.getBoundingClientRect().left - line.getBoundingClientRect().left);
  });
  expect(aligned).toBeLessThanOrEqual(1);

  await page.getByRole("button", { name: "Day" }).click();
  await scrollport.evaluate((element) => {
    element.scrollLeft = 1_200;
  });
  const before = await timeline.evaluate(centerMinute);
  const verticalBefore = await page.getByTestId("gantt-scrollport").evaluate((element) => element.scrollTop);
  await page.getByRole("button", { name: "Week" }).click();
  await expect
    .poll(async () => {
      const after = await timeline.evaluate(centerMinute);
      const dayWidth = Number(await timeline.getAttribute("data-day-width"));
      return Math.abs(after - before) <= 1_440 / dayWidth;
    })
    .toBe(true);
  await expect(page.getByTestId("gantt-scrollport")).toHaveJSProperty("scrollTop", verticalBefore);
});

test("drags the shared scroll plane and preserves alignment and zoom anchoring", async ({ page }) => {
  const timeline = page.getByTestId("gantt-timeline-viewport");
  const scrollport = page.getByTestId("gantt-scrollport");
  const box = await timeline.boundingBox();
  if (!box) throw new Error("Timeline pane is missing");
  const visibleTimelineWidth = await scrollport.evaluate((element) => {
    const tableWidth = element.querySelector<HTMLElement>(".gantt-treegrid-viewport")?.offsetWidth ?? 0;
    return element.clientWidth - tableWidth;
  });
  const startX = box.x + Math.min(visibleTimelineWidth - 20, 220);
  const startY = box.y + 80;
  const beforePan = await scrollport.evaluate((element) => ({
    scrollLeft: element.scrollLeft,
    scrollTop: element.scrollTop,
  }));

  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX - 2, startY + 20);
  await expect(scrollport).toHaveJSProperty("scrollLeft", beforePan.scrollLeft);
  await page.mouse.move(startX - 3, startY + 20);
  await page.mouse.move(startX - 123, startY + 35, { steps: 8 });
  await expect(timeline).toHaveCSS("cursor", "grabbing");
  await page.mouse.up();
  await expect(timeline).toHaveCSS("cursor", "grab");

  const afterScrollLeft = await scrollport.evaluate((element) => element.scrollLeft);
  expect(Math.abs(afterScrollLeft - beforePan.scrollLeft - 120)).toBeLessThanOrEqual(1);
  await expect(scrollport).toHaveJSProperty("scrollTop", beforePan.scrollTop);
  expect(await rulerGridDrift(timeline)).toBeLessThanOrEqual(1);
  expect(await timelineBarXDrift(timeline)).toBeLessThanOrEqual(1);
  expect(await maximumMountedRowDrift(scrollport)).toBeLessThanOrEqual(1);

  const centerBeforeZoom = await timeline.evaluate(centerMinute);
  await page.getByRole("button", { name: "Day" }).click();
  await expect
    .poll(async () => {
      const centerAfterZoom = await timeline.evaluate(centerMinute);
      const dayWidth = Number(await timeline.getAttribute("data-day-width"));
      return Math.abs(centerAfterZoom - centerBeforeZoom) <= 1_440 / dayWidth;
    })
    .toBe(true);
});

async function rulerGridDrift(timeline: import("@playwright/test").Locator): Promise<number> {
  return timeline.evaluate((viewport) => {
    const tick = viewport.querySelector<HTMLElement>("[data-ruler-minute]");
    if (!tick) throw new Error("Ruler tick is missing");
    const line = viewport.querySelector<SVGLineElement>(`[data-grid-minute="${tick.dataset.rulerMinute}"]`);
    if (!line) throw new Error("Matching grid line is missing");
    return Math.abs(tick.getBoundingClientRect().left - line.getBoundingClientRect().left);
  });
}

async function maximumMountedRowDrift(scrollport: import("@playwright/test").Locator): Promise<number> {
  return scrollport.evaluate((element) => {
    const rows = Array.from(element.querySelectorAll<HTMLElement>("tr[data-task-id]"));
    return Math.max(
      0,
      ...rows.map((row) => {
        const rule = element.querySelector<SVGLineElement>(
          `.gantt-timeline__row-rule[data-timeline-row-id="${row.dataset.taskId}"]`,
        );
        if (!rule) return Number.POSITIVE_INFINITY;
        return Math.abs(row.getBoundingClientRect().bottom - rule.getBoundingClientRect().top);
      }),
    );
  });
}

async function timelineBarXDrift(timeline: import("@playwright/test").Locator): Promise<number> {
  return timeline.evaluate((viewport) => {
    const bar = viewport.querySelector<SVGGraphicsElement>('[data-timeline-task-id="phase-1-task-1"]');
    if (!bar) throw new Error("Mounted timeline bar is missing");
    const localX = Number(bar.getAttribute("x"));
    return Math.abs(bar.getBoundingClientRect().left - (viewport.getBoundingClientRect().left + localX));
  });
}

function centerMinute(element: HTMLElement): number {
  const scrollport = element.closest<HTMLElement>(".gantt-treegrid-scrollport");
  if (!scrollport) throw new Error("Shared scrollport is missing");
  const domainStart = Number(element.dataset.domainStartMinute);
  const dayWidth = Number(element.dataset.dayWidth);
  const origin = Number(element.dataset.originX);
  const tableWidth = scrollport.querySelector<HTMLElement>(".gantt-treegrid-viewport")?.offsetWidth ?? 0;
  const screenX = tableWidth + Math.max(0, scrollport.clientWidth - tableWidth) / 2;
  const centerContent = scrollport.scrollLeft + screenX;
  return domainStart + ((centerContent - element.offsetLeft - origin) / dayWidth) * 1_440;
}

test("clips a long-distance predecessor path into the mounted successor window", async ({ page }) => {
  const scrollport = page.getByTestId("gantt-scrollport");
  await scrollport.evaluate((element) => {
    const rowHeight = Number.parseFloat(getComputedStyle(element).getPropertyValue("--row-h"));
    element.scrollTop = rowHeight * 42;
  });
  await expect(page.locator('[data-dependency="phase-1-task-1->phase-1-task-51:FS"]')).toHaveCount(1);
  await expect(page.locator('tr[data-task-id="phase-1-task-1"]')).toHaveCount(0);
});

test("keeps the work breakdown pinned and emphasizes hovered task dependencies", async ({ page }) => {
  const scrollport = page.getByTestId("gantt-scrollport");
  await page.getByRole("region", { name: "Schedule timeline" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect.poll(() => scrollport.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
  await scrollport.evaluate((element) => {
    element.scrollLeft = 1_000;
  });
  const pinned = await scrollport.evaluate((element) => ({
    scrollportLeft: element.getBoundingClientRect().left,
    tableLeft: element.querySelector(".gantt-treegrid-viewport")?.getBoundingClientRect().left,
  }));
  expect(Math.abs((pinned.tableLeft ?? 0) - pinned.scrollportLeft)).toBeLessThanOrEqual(1.1);

  await page.locator('tr[data-task-id="phase-1-task-2"]').hover();
  await expect(page.locator('[data-dependency="phase-1-task-1->phase-1-task-2:FS"]')).toHaveClass(
    /gantt-timeline__dependency--active/,
  );
  await expect(page.locator('[data-dependency="phase-1-task-3->phase-1-task-4:FS"]')).toHaveClass(
    /gantt-timeline__dependency--muted/,
  );
});

test("restores focus by task identity after a projection reorders it offscreen", async ({ page }) => {
  const activity = page.getByRole("rowheader", {
    name: /1\.1 Activity 1, task/,
  });
  await activity.focus();
  await page.evaluate(() => window.dispatchEvent(new Event("gantt-test-reorder")));

  const movedActivity = page.getByRole("rowheader", {
    name: /1\.99 Activity 1, task/,
  });
  await expect(movedActivity).toBeFocused();
  await expect(movedActivity.locator("xpath=ancestor::tr")).toHaveAttribute("aria-rowindex", "101");
});

test("keeps one usable tab stop when manual scrolling recycles the focused row", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  await firstTaskCell.focus();
  await page.locator(".gantt-treegrid-scrollport").evaluate((element) => {
    element.scrollTop = 20_000;
    element.dispatchEvent(new Event("scroll"));
  });

  const tabStop = page.locator(
    '.gantt-treegrid [role="rowheader"][tabindex="0"], .gantt-treegrid [role="gridcell"][tabindex="0"]',
  );
  await expect(tabStop).toHaveCount(1);
  await expect(tabStop).toBeFocused();
  await expect(firstTaskCell).toHaveCount(0);
  const geometry = await page.locator(".gantt-treegrid-scrollport").evaluate((scrollport) => {
    const cell = scrollport.querySelector<HTMLElement>('[tabindex="0"]');
    if (!cell) throw new Error("Tab stop is missing");
    return {
      cell: cell.getBoundingClientRect().toJSON(),
      scrollport: scrollport.getBoundingClientRect().toJSON(),
    };
  });
  expect(geometry.cell.top).toBeGreaterThanOrEqual(geometry.scrollport.top + 34);
  expect(geometry.cell.bottom).toBeLessThanOrEqual(geometry.scrollport.bottom);
});

test("does not reclaim focus after a pointer moves it outside the treegrid", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  await firstTaskCell.focus();
  await page.getByRole("heading", { name: "Production treegrid browser contract" }).click();
  await expect(page.locator("body")).toBeFocused();

  await page.locator(".gantt-treegrid-scrollport").evaluate((element) => {
    element.scrollTop = 20_000;
    element.dispatchEvent(new Event("scroll"));
  });
  await expect(page.locator("body")).toBeFocused();
});

test("keeps state distinguishable with reduced motion and forced colors", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "active" });
  await page.reload();

  const firstTaskCell = page.getByRole("rowheader", {
    name: /1 Phase 1, task/,
  });
  await firstTaskCell.focus();
  await expect(firstTaskCell).toBeFocused();
  const disclosure = page.getByRole("button", { name: "Collapse Phase 1" });
  await expect(disclosure.locator("svg")).toHaveCSS("transition-duration", "0s");
  await expect(page.getByText("Critical", { exact: true }).first()).toBeVisible();

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("uses the documented schedule palette in dark mode", async ({ page }) => {
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  const tokens = await page.getByTestId("gantt-scrollport").evaluate((element) => {
    const style = getComputedStyle(element);
    const todayProbe = document.createElement("span");
    todayProbe.style.color = "var(--color-neutral-500)";
    element.append(todayProbe);
    const expectedToday = getComputedStyle(todayProbe).color;
    todayProbe.remove();
    return {
      critical: style.getPropertyValue("--sched-critical").trim(),
      expectedCritical: style.getPropertyValue("--color-accent-300").trim(),
      normal: style.getPropertyValue("--sched-normal").trim(),
      expectedNormal: style.getPropertyValue("--color-accent-600").trim(),
      baseline: style.getPropertyValue("--sched-baseline").trim(),
      expectedBaseline: style.getPropertyValue("--color-neutral-600").trim(),
      today: getComputedStyle(document.querySelector<HTMLElement>('[data-testid="gantt-today-marker"]')!)
        .borderLeftColor,
      expectedToday,
    };
  });
  expect(tokens.critical).toBe(tokens.expectedCritical);
  expect(tokens.normal).toBe(tokens.expectedNormal);
  expect(tokens.baseline).toBe(tokens.expectedBaseline);
  expect(tokens.today).toBe(tokens.expectedToday);
});

test("renders typed predecessor annotations with accessible names and stays accessible", async ({ page }) => {
  await page.locator(".gantt-schedule").evaluate((schedule) => {
    (schedule as HTMLElement).style.setProperty("--font-heading", "system-ui, sans-serif");
  });

  // Activity 5 carries an SS+120 link from Activity 4 (phase-1-task-4).
  const ssCell = page.getByRole("gridcell", {
    name: /1\.5 Activity 5, predecessors, predecessor phase-1-task-4, start-to-start, lag \+120 minutes/,
  });
  await expect(ssCell).toBeVisible();
  await expect(page.getByTestId("predecessors-phase-1-task-5")).toHaveText(/phase-1-task-4 SS \+120 min/);

  // Activity 6 carries an FF-60 link (negative lag).
  const ffCell = page.getByRole("gridcell", {
    name: /1\.6 Activity 6, predecessors, predecessor phase-1-task-5, finish-to-finish, lag -60 minutes/,
  });
  await expect(ffCell).toBeVisible();
  await expect(page.getByTestId("predecessors-phase-1-task-6")).toHaveText(/phase-1-task-5 FF -60 min/);

  // Activity 7 carries an SF+240 link.
  await expect(page.getByTestId("predecessors-phase-1-task-7")).toHaveText(/phase-1-task-6 SF \+240 min/);
  // A plain FS+0 link drops the lag text entirely.
  await expect(page.getByTestId("predecessors-phase-1-task-2")).toHaveText(/^phase-1-task-1 FS$/);

  // Fit discipline: measure the WIDEST predecessor cells — the SF +240 single
  // link (phase-1-task-7) and the two-link cell (phase-1-task-11) — at both
  // layouts. Each must not clip horizontally and must keep >=2 spare track lines.
  const measurePred = (testId: string) =>
    page.getByTestId(testId).evaluate((element) => {
      const line = parseFloat(getComputedStyle(element).lineHeight) || element.clientHeight;
      const range = document.createRange();
      range.selectNodeContents(element);
      const rects = Array.from(range.getClientRects());
      const widestLine = rects.length ? Math.max(...rects.map((rect) => rect.width)) : element.scrollWidth;
      const tops = new Set(rects.map((rect) => Math.round(rect.top)));
      const lineCount = Math.max(1, tops.size);
      const trackLines = Math.max(1, Math.round(element.clientHeight / line));
      return {
        clientWidth: element.clientWidth,
        scrollWidth: element.scrollWidth,
        widestLine: Math.round(widestLine * 10) / 10,
        margin: Math.round((element.clientWidth - widestLine) * 10) / 10,
        lineCount,
        trackLines,
        spareLines: trackLines - lineCount,
      };
    });
  for (const width of [1100, 760]) {
    await page.setViewportSize({ width, height: 700 });
    if (width === 760) {
      await page.getByTestId("gantt-scrollport").evaluate((scrollport) => {
        const schedule = scrollport.closest<HTMLElement>(".gantt-schedule");
        if (!schedule) throw new Error("Schedule surface is missing");
        schedule.style.width = "724px";
      });
    }
    for (const testId of ["predecessors-phase-1-task-7", "predecessors-phase-1-task-11"]) {
      const fit = await measurePred(testId);
      console.log(`PRED_FIT ${width} ${testId} ${JSON.stringify(fit)}`);
      // Greedy wrapping fills the line box, so the range-rect width can round a
      // sub-pixel over the floored clientWidth without a real overflow; the honest
      // clip guard is the integer scrollWidth (as the constraint facts use) plus,
      // for wrapping cells, vertical spare track lines.
      expect(fit.scrollWidth, `${testId} horizontal clip @${width}`).toBeLessThanOrEqual(fit.clientWidth);
      // >=2 spare track lines locally so a wider Linux glyph set (which inflated
      // the annotation by ~1 line in CI) still leaves at least one spare line.
      expect(fit.spareLines, `${testId} spare lines @${width}`).toBeGreaterThanOrEqual(2);
    }
  }
  await page.setViewportSize({ width: 1100, height: 700 });

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("draws type-aware dependency-line anchors for SS, FF, and SF links", async ({ page }) => {
  // First browser coverage of dependency-line geometry. Anchors come from the
  // row instants: predecessor start for SS/SF and finish for FS/FF; successor
  // start for FS/SS and finish for FF/SF.
  const geometry = await page.evaluate(() => {
    const bar = (taskId: string) => {
      const rect = document
        .querySelector<SVGRectElement>(`[data-timeline-task-id="${taskId}"]`)!
        .getBBox();
      return { left: rect.x, right: rect.x + rect.width };
    };
    const link = (dependency: string) => {
      const path = document.querySelector<SVGPathElement>(`[data-dependency="${dependency}"]`)!;
      const totalLength = path.getTotalLength();
      const start = path.getPointAtLength(0);
      const end = path.getPointAtLength(totalLength);
      return {
        predecessorAnchor: path.dataset.predecessorAnchor,
        successorAnchor: path.dataset.successorAnchor,
        type: path.dataset.dependencyType,
        startX: start.x,
        endX: end.x,
        totalLength,
        d: path.getAttribute("d") ?? "",
      };
    };
    return {
      ss: { link: link("phase-1-task-4->phase-1-task-5:SS"), pred: bar("phase-1-task-4"), succ: bar("phase-1-task-5") },
      ff: { link: link("phase-1-task-5->phase-1-task-6:FF"), pred: bar("phase-1-task-5"), succ: bar("phase-1-task-6") },
      sf: { link: link("phase-1-task-6->phase-1-task-7:SF"), pred: bar("phase-1-task-6"), succ: bar("phase-1-task-7") },
      // A leftward link (predecessor phase-1-task-12 is a later-day activity, so
      // the FF successor finish lands left of the predecessor finish). The elbow
      // routes backward and must still render finitely.
      leftward: {
        link: link("phase-1-task-12->phase-1-task-9:FF"),
        pred: bar("phase-1-task-12"),
        succ: bar("phase-1-task-9"),
      },
    };
  });

  // SS: predecessor start -> successor start.
  expect(geometry.ss.link.type).toBe("SS");
  expect(geometry.ss.link.predecessorAnchor).toBe("start");
  expect(geometry.ss.link.successorAnchor).toBe("start");
  expect(geometry.ss.link.startX).toBeCloseTo(geometry.ss.pred.left, 0);
  expect(geometry.ss.link.endX).toBeCloseTo(geometry.ss.succ.left, 0);

  // FF: predecessor finish -> successor finish.
  expect(geometry.ff.link.type).toBe("FF");
  expect(geometry.ff.link.predecessorAnchor).toBe("finish");
  expect(geometry.ff.link.successorAnchor).toBe("finish");
  expect(geometry.ff.link.startX).toBeCloseTo(geometry.ff.pred.right, 0);
  expect(geometry.ff.link.endX).toBeCloseTo(geometry.ff.succ.right, 0);

  // SF: predecessor start -> successor finish.
  expect(geometry.sf.link.type).toBe("SF");
  expect(geometry.sf.link.predecessorAnchor).toBe("start");
  expect(geometry.sf.link.successorAnchor).toBe("finish");
  expect(geometry.sf.link.startX).toBeCloseTo(geometry.sf.pred.left, 0);
  expect(geometry.sf.link.endX).toBeCloseTo(geometry.sf.succ.right, 0);

  // Leftward FF link: successor finish anchor sits left of the predecessor finish
  // anchor, so the elbow routes backward. The path must render with finite length.
  expect(geometry.leftward.link.type).toBe("FF");
  expect(geometry.leftward.link.endX).toBeLessThan(geometry.leftward.link.startX);
  expect(geometry.leftward.link.startX).toBeCloseTo(geometry.leftward.pred.right, 0);
  expect(geometry.leftward.link.endX).toBeCloseTo(geometry.leftward.succ.right, 0);
  expect(Number.isFinite(geometry.leftward.link.totalLength)).toBe(true);
  expect(geometry.leftward.link.totalLength).toBeGreaterThan(0);
  expect(geometry.leftward.link.d).not.toMatch(/NaN|Infinity/);
});
