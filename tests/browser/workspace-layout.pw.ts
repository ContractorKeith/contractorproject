import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test("keeps the jobs list close to the header across supported widths", async ({ page }) => {
  await page.goto("/tests/browser/workspace.html");
  await expect(page.getByRole("button", { name: "Open schedule for Riverside renovation" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1, name: "Local jobs" })).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

  for (const width of [1440, 760, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const layout = await page.evaluate(() => {
      const header = document.querySelector(".app-header")!.getBoundingClientRect();
      const jobs = document.querySelector(".job-section")!.getBoundingClientRect();
      return {
        headerBottom: header.bottom,
        jobsTop: jobs.top,
        pageWidth: document.documentElement.scrollWidth,
      };
    });

    expect(layout.jobsTop - layout.headerBottom).toBeLessThanOrEqual(40);
    expect(layout.jobsTop).toBeLessThan(180);
    expect(layout.pageWidth).toBeLessThanOrEqual(width);
  }
});

test("starts the open schedule near the header on desktop and mobile", async ({ page }) => {
  await page.goto("/tests/browser/workspace.html");
  await page.getByRole("button", { name: "Open schedule for Riverside renovation" }).click();
  await expect(page.getByRole("treegrid")).toBeVisible();

  for (const width of [1440, 760]) {
    await page.setViewportSize({ width, height: 900 });
    const distance = await page.evaluate(() => {
      const header = document.querySelector(".app-header")!.getBoundingClientRect();
      const workspace = document.querySelector(".job-workspace__heading")!.getBoundingClientRect();
      return {
        gap: workspace.top - header.bottom,
        pageWidth: document.documentElement.scrollWidth,
      };
    });

    // Allow the back/archive toolbar and card padding, but no hero above it.
    expect(distance.gap).toBeLessThanOrEqual(120);
    expect(distance.pageWidth).toBeLessThanOrEqual(width);
  }
});
