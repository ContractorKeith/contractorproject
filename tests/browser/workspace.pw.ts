import { readFile } from "node:fs/promises";
import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/browser/workspace.html");
  await page.getByRole("button", { name: "Open schedule for Riverside renovation" }).click();
  await expect(page.getByRole("treegrid")).toBeVisible();
});

test("puts answers and readable names first with one editor at desktop and narrow widths", async ({ page }) => {
  await expect(page.getByRole("region", { name: "Schedule summary" })).toContainText("3 of 25 complete");
  await expect(page.getByRole("region", { name: "Schedule summary" })).toContainText("Rough plumbing");
  await expect(page.getByRole("treegrid")).toHaveAttribute("aria-colcount", "6");
  await expect(page.locator('.task-editor input[aria-label^="Task name for"]')).toHaveCount(1);
  await expect(page.locator(".advanced-schedule")).not.toHaveAttribute("open");
  await expect(page.locator(".task-editor__advanced")).not.toHaveAttribute("open");

  for (const width of [1440, 760]) {
    await page.setViewportSize({ width, height: 1000 });
    const grid = await page.getByRole("treegrid").boundingBox();
    expect(grid!.y).toBeLessThan(800);
    const name = page.locator('tr[data-task-id="phase-1-task-4"] .gantt-treegrid__task-copy > span').first();
    const fit = await name.evaluate((element) => ({ available: element.clientWidth, used: element.scrollWidth }));
    expect(fit.available).toBeGreaterThanOrEqual(fit.used);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  }
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);
});

test("edits a selected task in place, keeps its draft, and records progress without advanced settings", async ({ page }) => {
  await page.locator('tr[data-task-id="phase-1-task-4"]').getByRole("rowheader").click();
  const duration = page.getByRole("spinbutton", { name: "Duration for Rough plumbing", exact: true });
  await duration.fill("2");
  await page.getByRole("button", { name: "Save duration", exact: true }).click();
  await expect(duration).toHaveValue("2");
  await expect(page.getByRole("heading", { name: "Edit Rough plumbing" })).toBeVisible();

  await page.getByRole("spinbutton", { name: "Percent complete for Rough plumbing" }).fill("100");
  await page.getByLabel("Actual start for Rough plumbing", { exact: true }).fill("2026-08-20");
  await page.getByLabel("Actual finish for Rough plumbing", { exact: true }).fill("2026-08-21");
  await page.getByRole("button", { name: "Save progress", exact: true }).click();
  await expect(page.getByRole("region", { name: "Schedule summary" })).toContainText("4 of 25 complete");
  await expect(page.locator(".task-editor__advanced")).not.toHaveAttribute("open");

  const taskName = page.getByRole("textbox", { name: "Task name for Rough plumbing" });
  await taskName.fill("Keep my draft");
  page.once("dialog", (dialog) => dialog.dismiss());
  await page.locator('tr[data-task-id="phase-1-task-5"]').getByRole("rowheader").click();
  await expect(taskName).toHaveValue("Keep my draft");
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "Edit selected task" }).click();
  await expect(page.getByRole("textbox", { name: "Task name for Activity 5" })).toBeVisible();
});

test("downloads the full job from a filtered schedule and renders a self-contained printable report", async ({ page, context }) => {
  await page.getByRole("searchbox", { name: "Search tasks or WBS" }).fill("Rough plumbing");
  await expect(page.getByText("1 result", { exact: true })).toBeVisible();
  const csvEvent = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export CSV", exact: true }).click();
  const csv = await csvEvent;
  expect(csv.suggestedFilename()).toBe("Riverside-renovation-schedule.csv");
  const csvText = await readFile((await csv.path())!, "utf8");
  expect(csvText).toContain("Rough plumbing");
  expect(csvText).toContain("Activity 25");
  expect(csvText.trim().split("\r\n")).toHaveLength(27);

  const htmlEvent = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download printable schedule" }).click();
  const html = await htmlEvent;
  const htmlText = await readFile((await html.path())!, "utf8");
  expect(html.suggestedFilename()).toBe("Riverside-renovation-schedule.html");
  expect(htmlText).toContain("Activity 25");
  const report = await context.newPage();
  const requests: string[] = [];
  report.on("request", (request) => requests.push(request.url()));
  await report.setContent(htmlText);
  await report.emulateMedia({ media: "print" });
  await expect(report.getByRole("heading", { name: "Riverside renovation" })).toBeVisible();
  await expect(report.locator("tbody tr")).toHaveCount(26);
  expect(requests).toEqual([]);
  expect(await report.locator("script").count()).toBe(0);
  await report.close();
});

test("creates a job directly into an actionable task workspace", async ({ page }) => {
  await page.goto("/tests/browser/workspace.html?empty");
  await page.getByRole("textbox", { name: "Job name", exact: true }).fill("Kitchen renovation");
  await page.getByRole("button", { name: "Create job" }).click();
  await expect(page.getByText("Choose a schedule start to calculate task dates.")).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add task", exact: true })).toBeVisible();
  await expect(page.getByLabel("Schedule start for Kitchen renovation")).toBeVisible();
  await page.getByLabel("Schedule start for Kitchen renovation").fill("2026-09-08");
  await page.getByRole("button", { name: "Save schedule settings", exact: true }).click();
  await page.getByRole("textbox", { name: "New root task" }).fill("Prepare site");
  await page.getByRole("button", { name: "Add task", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Edit Prepare site" })).toBeVisible();
  await expect(page.getByText("Add a duration to every leaf task to calculate the schedule.")).toBeVisible();
  await page.getByRole("spinbutton", { name: "Duration for Prepare site", exact: true }).fill("1");
  await page.getByRole("button", { name: "Save duration", exact: true }).click();
  await expect(page.getByRole("treegrid", { name: "Schedule for Kitchen renovation" })).toBeVisible();
});
