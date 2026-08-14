import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/browser/gantt.html");
});

test("virtualizes 1,000 logical rows without accessibility violations", async ({ page }) => {
  const treegrid = page.getByRole("treegrid", { name: "Work breakdown schedule" });
  await expect(treegrid).toHaveAttribute("aria-rowcount", "1001");
  await expect(treegrid.locator('tr[data-task-id="phase-1"]')).toBeVisible();

  const mountedTaskRows = await treegrid.locator("tbody tr[data-task-id]").count();
  expect(mountedTaskRows).toBeLessThan(50);

  const rowGeometry = await treegrid.evaluate((grid) => ({
    configured: Number.parseFloat(getComputedStyle(grid).getPropertyValue("--row-h")),
    actual: Array.from(grid.querySelectorAll<HTMLElement>("tr[data-task-id]"), (row) =>
      row.getBoundingClientRect().height,
    ).slice(0, 5),
  }));
  expect(rowGeometry.actual).not.toHaveLength(0);
  for (const actualHeight of rowGeometry.actual) {
    expect(actualHeight).toBeCloseTo(rowGeometry.configured, 1);
  }

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});

test("keeps compact density synchronized with the virtual scroll model", async ({ page }) => {
  await page.evaluate(() => {
    document.documentElement.style.setProperty("--row-h", "var(--row-h-compact)");
  });

  await expect
    .poll(async () =>
      page.locator(".gantt-treegrid").evaluate((grid) => ({
        configured: Number.parseFloat(getComputedStyle(grid).getPropertyValue("--row-h")),
        actual: grid.querySelector<HTMLElement>("tr[data-task-id]")?.getBoundingClientRect().height,
      })),
    )
    .toEqual({ configured: 24, actual: 24 });

  const geometry = await page.locator(".gantt-treegrid-scrollport").evaluate((scrollport) => ({
    scrollHeight: scrollport.scrollHeight,
    headerHeight:
      scrollport.querySelector<HTMLElement>("thead")?.getBoundingClientRect().height ?? 0,
  }));
  expect(geometry.scrollHeight - geometry.headerHeight).toBeCloseTo(24_000, 0);
});

test("moves cell focus and recovers it after an offscreen jump", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
  await firstTaskCell.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("gridcell", { name: /1 Phase 1, duration/ })).toBeFocused();

  await page.keyboard.press("Control+End");
  const lastCell = page.getByRole("gridcell", { name: /10\.99 Activity 99, total float/ });
  await expect(lastCell).toBeFocused();
  await expect(lastCell.locator("xpath=ancestor::tr")).toHaveAttribute("aria-rowindex", "1001");

  await page.keyboard.press("Control+Home");
  await expect(page.getByRole("gridcell", { name: /1 Phase 1, WBS/ })).toBeFocused();
});

test("scrolls horizontal cells into view as keyboard focus moves", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 700 });
  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
  await firstTaskCell.focus();
  await page.keyboard.press("End");

  const floatCell = page.getByRole("gridcell", { name: /1 Phase 1, total float/ });
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
  expect(geometry.scrollLeft).toBeGreaterThan(0);
  expect(geometry.cell.left).toBeGreaterThanOrEqual(geometry.scrollport.left);
  expect(geometry.cell.right).toBeLessThanOrEqual(geometry.scrollport.right);
});

test("moves a viewport page down and back while keeping focus visible", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
  await firstTaskCell.focus();
  await page.keyboard.press("PageDown");

  const pagedCell = page.getByRole("rowheader", { name: /1\.16 Activity 16, task/ });
  await expect(pagedCell).toBeFocused();
  await expect(pagedCell).toBeInViewport();
  await expect(pagedCell.locator("xpath=ancestor::tr")).toHaveAttribute("aria-rowindex", "18");

  await page.keyboard.press("PageUp");
  await expect(firstTaskCell).toBeFocused();
  await expect(firstTaskCell).toBeInViewport();
});

test("collapses descendants without renumbering rows or losing focus", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
  await firstTaskCell.focus();
  await page.keyboard.press("Space");

  await expect(firstTaskCell).toBeFocused();
  await expect(firstTaskCell.locator("xpath=ancestor::tr")).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator('tr[data-task-id="phase-1-task-1"]')).toHaveCount(0);
  await expect(page.locator('tr[data-task-id="phase-2"]')).toHaveAttribute("aria-rowindex", "102");
});

test("restores focus by task identity after a projection reorders it offscreen", async ({ page }) => {
  const activity = page.getByRole("rowheader", { name: /1\.1 Activity 1, task/ });
  await activity.focus();
  await page.evaluate(() => window.dispatchEvent(new Event("gantt-test-reorder")));

  const movedActivity = page.getByRole("rowheader", { name: /1\.99 Activity 1, task/ });
  await expect(movedActivity).toBeFocused();
  await expect(movedActivity.locator("xpath=ancestor::tr")).toHaveAttribute("aria-rowindex", "101");
});

test("keeps one usable tab stop when manual scrolling recycles the focused row", async ({ page }) => {
  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
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
  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
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

  const firstTaskCell = page.getByRole("rowheader", { name: /1 Phase 1, task/ });
  await firstTaskCell.focus();
  await expect(firstTaskCell).toBeFocused();
  const disclosure = page.getByRole("button", { name: "Collapse Phase 1" });
  await expect(disclosure.locator("svg")).toHaveCSS("transition-duration", "0s");
  await expect(page.getByText("Critical", { exact: true }).first()).toBeVisible();

  const results = await new AxeBuilder({ page }).include(".gantt-treegrid-scrollport").analyze();
  expect(results.violations).toEqual([]);
});
