import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("keeps new-job schedule guidance and setup controls readable in both themes", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 1000 });
  await page.goto("/tests/browser/workspace.html?empty");
  await page.getByRole("button", { name: "Open settings" }).click();
  await page.getByRole("textbox", { name: "Job name", exact: true }).fill("Kitchen renovation");
  await page.getByRole("button", { name: "Create job" }).click();

  const guidance = page.getByRole("status").filter({ hasText: "Start your schedule" });
  const startDate = page.getByLabel("Schedule start for Kitchen renovation");
  await expect(guidance).toContainText("Choose the first working day below");
  await expect(startDate).toBeVisible();
  await expect(page.getByRole("group", { name: "Working weekdays" })).toBeVisible();
  await page.evaluate(() => {
    const notice = document.createElement("div");
    notice.className = "gantt-state gantt-state--error";
    notice.setAttribute("role", "alert");
    notice.textContent = "Synthetic schedule error notice.";
    document.querySelector(".workspace")!.append(notice);
  });

  for (const theme of ["light", "dark"] as const) {
    await page.evaluate((resolvedTheme) => {
      document.documentElement.dataset.theme = resolvedTheme;
    }, theme);

    const ratios = await page.evaluate(() => {
      function rgb(value: string): [number, number, number] {
        const channels = value.match(/[\d.]+/g)?.map(Number);
        if (!channels || channels.length < 3) throw new Error(`Unexpected color: ${value}`);
        return channels.slice(0, 3) as [number, number, number];
      }
      function luminance(color: [number, number, number]) {
        const linear = color.map((channel) => {
          const value = channel / 255;
          return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
        });
        return 0.2126 * linear[0]! + 0.7152 * linear[1]! + 0.0722 * linear[2]!;
      }
      function contrast(foreground: string, background: string) {
        const levels = [luminance(rgb(foreground)), luminance(rgb(background))].sort((a, b) => b - a);
        return (levels[0]! + 0.05) / (levels[1]! + 0.05);
      }
      function paintedBackground(element: Element) {
        for (let current: Element | null = element; current; current = current.parentElement) {
          const background = getComputedStyle(current).backgroundColor;
          if (background !== "rgba(0, 0, 0, 0)" && background !== "transparent") return background;
        }
        return getComputedStyle(document.documentElement).backgroundColor;
      }
      const guidance = document.querySelector<HTMLElement>(".schedule-guidance")!;
      const notice = document.querySelector<HTMLElement>(".gantt-state--error")!;
      const setup = document.querySelector<HTMLElement>(".schedule-setup")!;
      const dateLabel = document.querySelector<HTMLElement>('label:has(> input[aria-label="Schedule start for Kitchen renovation"])')!;
      const start = document.querySelector<HTMLInputElement>('input[aria-label="Schedule start for Kitchen renovation"]')!;
      const legend = document.querySelector<HTMLElement>(".schedule-settings fieldset legend")!;
      const weekday = document.querySelector<HTMLElement>(".schedule-settings fieldset label")!;
      const colors = (element: Element) => {
        const style = getComputedStyle(element);
        return contrast(style.color, paintedBackground(element));
      };
      return {
        guidance: colors(guidance),
        guidanceCopy: colors(guidance.querySelector("span")!),
        errorNotice: colors(notice),
        setupLegend: colors(setup.querySelector("summary")!),
        startLabel: colors(dateLabel),
        dateValue: contrast(getComputedStyle(start).color, getComputedStyle(start).backgroundColor),
        weekdaysLegend: colors(legend),
        weekdayLabel: colors(weekday),
      };
    });

    for (const [name, ratio] of Object.entries(ratios)) {
      expect(ratio, `${theme} ${name} contrast`).toBeGreaterThanOrEqual(4.5);
    }
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations, `${theme} axe violations`).toEqual([]);
  }

  await startDate.fill("2026-10-05");
  await page.getByRole("button", { name: "Save schedule settings", exact: true }).click();
  await expect(guidance).toHaveCount(0);
  await page.getByText("Schedule setup", { exact: true }).click();
  await expect(page.getByText("Schedule settings saved.")).toBeVisible();
  await expect(startDate).toHaveValue("2026-10-05");
});
