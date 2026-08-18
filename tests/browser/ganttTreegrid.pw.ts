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

test("announces visible constraint values and direct violations without color-only state", async ({ page }) => {
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
    await expect(visibleFacts.nth(1)).toHaveText("Baseline 2026-08-14 08:00 +4,320 min");
    await expect(visibleFacts.nth(2)).toHaveText("≥ 2026-08-18");
    await expect(constrainedRow.getByTestId("constraint-float-phase-1-task-1")).toHaveText(/Constraint\s*violated/);
    const factDimensions = await visibleFacts.evaluateAll((elements) => elements.map((element) => ({
      clientWidth: element.clientWidth,
      scrollWidth: element.scrollWidth,
      clientHeight: element.clientHeight,
      scrollHeight: element.scrollHeight,
      visible: element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0,
    })));
    for (const dimensions of factDimensions) {
      expect(dimensions.visible).toBe(true);
      expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth);
      expect(dimensions.scrollHeight).toBeLessThanOrEqual(dimensions.clientHeight);
    }
  }

  const float = page.getByRole("gridcell", {
    name: /1\.1 Activity 1, total float, 0 min, critical, constraint violated/,
  });
  await expect(float).toHaveText(/Constraint\s*violated/);
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
    .toEqual({ configured: 36, actual: 36 });

  const geometry = await page.locator(".gantt-treegrid-scrollport").evaluate((scrollport) => ({
    scrollHeight: scrollport.scrollHeight,
    headerHeight: scrollport.querySelector<HTMLElement>("thead")?.getBoundingClientRect().height ?? 0,
  }));
  expect(geometry.scrollHeight - geometry.headerHeight).toBeCloseTo(36_000, 0);
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
    element.scrollTop = 1_300;
  });
  await expect(page.locator('[data-dependency="phase-1-task-1->phase-1-task-51"]')).toHaveCount(1);
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
  await expect(page.locator('[data-dependency="phase-1-task-1->phase-1-task-2"]')).toHaveClass(
    /gantt-timeline__dependency--active/,
  );
  await expect(page.locator('[data-dependency="phase-1-task-3->phase-1-task-4"]')).toHaveClass(
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
    return {
      critical: style.getPropertyValue("--sched-critical").trim(),
      expectedCritical: style.getPropertyValue("--color-accent-300").trim(),
      normal: style.getPropertyValue("--sched-normal").trim(),
      expectedNormal: style.getPropertyValue("--color-accent-600").trim(),
      baseline: style.getPropertyValue("--sched-baseline").trim(),
      expectedBaseline: style.getPropertyValue("--color-neutral-600").trim(),
    };
  });
  expect(tokens.critical).toBe(tokens.expectedCritical);
  expect(tokens.normal).toBe(tokens.expectedNormal);
  expect(tokens.baseline).toBe(tokens.expectedBaseline);
});
