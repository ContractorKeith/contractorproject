import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const variants = ["shared-svg", "split-canvas", "row-dom"] as const;

for (const variant of variants) {
  test(`${variant} keeps the table authoritative and passes automated accessibility checks`, async ({ page }) => {
    await page.goto(`/gantt-prototype.html?variant=${variant}`);
    const treegrid = page.getByRole("treegrid", { name: "Work breakdown schedule" });
    await expect(treegrid).toHaveAttribute("aria-rowcount", "1001");
    await expect(treegrid.getByRole("columnheader")).toHaveCount(6);
    await expect(page.getByText("1,000 visible rows", { exact: true })).toBeVisible();

    const firstCell = page.getByRole("gridcell", { name: /1 Site work, summary, critical/i });
    await expect(firstCell).toHaveAttribute("aria-label", /baseline Jan 6 to Feb 18/i);
    await expect(page.getByRole("gridcell", { name: /1\.1\.2 Task 2, task, critical/i })).toHaveAttribute(
      "aria-label",
      /predecessors P1\.1\.1/i,
    );
    await firstCell.focus();
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("gridcell", { name: /1\.1 Site work package 1, summary/i })).toBeFocused();

    const accessibility = await new AxeBuilder({ page })
      .exclude("canvas")
      .analyze();
    expect(accessibility.violations).toEqual([]);
  });
}

test("shared SVG preserves keyboard focus, logical indexes, scroll alignment, and zoom anchor", async ({ page }) => {
  await page.goto("/gantt-prototype.html?variant=shared-svg");
  const scrollport = page.getByTestId("schedule-scrollport");
  const firstCell = page.getByRole("gridcell", { name: /1 Site work, summary, critical/i });
  await firstCell.focus();

  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("gridcell", { name: /1 Site work, duration 46 days/i })).toBeFocused();
  await expect(page).toHaveURL(/variant=shared-svg/);
  await page.keyboard.press("ArrowLeft");
  await expect(firstCell).toBeFocused();

  await page.keyboard.press("PageDown");
  await expect(page.locator('[data-cell="phase-1-package-2-task-7-0"]')).toBeFocused();
  await page.keyboard.press("ControlOrMeta+Home");
  await expect(firstCell).toBeFocused();

  await page.keyboard.press("ControlOrMeta+End");
  await expect(page.locator('[data-cell="phase-10-package-9-task-10-5"]')).toBeFocused();
  await expect(page.locator('[data-row-id="phase-10-package-9-task-10"]')).toHaveAttribute(
    "aria-rowindex",
    "1001",
  );

  await page.keyboard.press("ControlOrMeta+Home");
  await expect(firstCell).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByText("901 visible rows", { exact: true })).toBeVisible();
  await expect(page.locator('[data-row-id="phase-1"]')).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator('[data-row-id="phase-1"]')).toHaveAttribute("aria-posinset", "1");
  await expect(page.locator('[data-row-id="phase-1"]')).toHaveAttribute("aria-setsize", "10");
  await expect(page.locator('[data-row-id="phase-2"]')).toHaveAttribute("aria-rowindex", "102");
  await expect(firstCell).toBeFocused();

  await scrollport.evaluate((element) => {
    element.scrollTop = 2_800;
    element.scrollLeft = 800;
    element.dispatchEvent(new Event("scroll"));
  });
  await page.waitForTimeout(100);
  await expect(page.locator(".timeline-ruler")).toHaveAttribute("style", /translateX\(-800px\)/);
  const drift = await page.evaluate(() => {
    const row = document.querySelector<HTMLElement>("[data-row-id]");
    const marker = document.querySelector<SVGGraphicsElement>(".timeline-row-rule[data-timeline-row-id]");
    if (!row || !marker) return 999;
    return Math.abs(row.getBoundingClientRect().bottom - marker.getBoundingClientRect().top);
  });
  expect(drift).toBeLessThanOrEqual(1);

  const beforeZoom = await scrollport.evaluate((element) => ({
    centerDay: (element.scrollLeft + (element.clientWidth - 704) / 2) / 8,
    scrollTop: element.scrollTop,
  }));
  await page.getByRole("button", { name: "month", exact: true }).click();
  await page.waitForTimeout(100);
  const afterZoom = await scrollport.evaluate((element) => ({
    centerDay: (element.scrollLeft + (element.clientWidth - 704) / 2) / 2.4,
    scrollTop: element.scrollTop,
  }));
  expect(Math.abs(afterZoom.centerDay - beforeZoom.centerDay)).toBeLessThanOrEqual(1);
  expect(afterZoom.scrollTop).toBe(beforeZoom.scrollTop);
});

test("split canvas keeps its independently scrolled panes vertically synchronized", async ({ page }) => {
  await page.goto("/gantt-prototype.html?variant=split-canvas");
  const scrollports = page.locator(".split-scrollport");
  await scrollports.nth(0).evaluate((element) => {
    element.scrollTop = 4_200;
    element.dispatchEvent(new Event("scroll"));
  });
  await page.waitForTimeout(100);
  const offsets = await scrollports.evaluateAll((elements) =>
    elements.map((element) => (element as HTMLElement).scrollTop),
  );
  expect(Math.abs(offsets[0]! - offsets[1]!)).toBeLessThanOrEqual(1);
});

test("reduced motion and forced colors preserve the interaction surface", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "active" });
  for (const variant of variants) {
    await page.goto(`/gantt-prototype.html?variant=${variant}`);
    const firstCell = page.getByRole("gridcell", { name: /1 Site work, summary, critical/i });
    await expect(firstCell).toHaveAttribute("aria-label", /baseline Jan 6 to Feb 18/i);
    await firstCell.focus();
    await expect(firstCell).toBeFocused();
    const transitionDuration = await page.evaluate(
      () => getComputedStyle(document.documentElement).transitionDuration,
    );
    expect(Number.parseFloat(transitionDuration)).toBeLessThanOrEqual(0.001);
  }
});
