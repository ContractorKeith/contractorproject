import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as exports from "../scheduleExport";
import type { GanttReadModel } from "../types/gantt";
import { ScheduleExports } from "./ScheduleExports";

const readModel: GanttReadModel = { contractVersion: 7, jobId: "job", jobVersion: 1, scheduleStart: "2026-08-17T08:00:00", scheduleFinish: "2026-08-17T16:00:00", dataDate: null, baselineId: null, rowCount: 0, criticalTaskIds: [], criticalPath: [], calendar: { workingWeekdays: [], exceptionDates: [] }, rows: [] };

afterEach(() => vi.restoreAllMocks());

describe("ScheduleExports", () => {
  it("blocks stale schedule downloads until the current projection arrives", async () => {
    const save = vi.spyOn(exports, "saveSchedule").mockResolvedValue(true);
    const user = userEvent.setup();
    const { rerender } = render(<ScheduleExports readModel={readModel} jobName="Oak job" disabled />);
    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    await user.click(screen.getByRole("button", { name: "Download printable schedule" }));
    expect(save).not.toHaveBeenCalled();
    rerender(<ScheduleExports readModel={readModel} jobName="Oak job" />);
    await user.click(screen.getByRole("button", { name: "Download printable schedule" }));
    expect(save).toHaveBeenCalledWith(expect.any(String), "Oak-job-schedule.html", "html");
  });

  it("offers direct exports, reports a failure, and clears it on retry", async () => {
    const save = vi.spyOn(exports, "saveSchedule")
      .mockRejectedValueOnce(new Error("blocked"))
      .mockResolvedValue(true);
    const user = userEvent.setup();
    render(<ScheduleExports readModel={readModel} jobName="Oak job" />);

    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    expect(save).toHaveBeenCalledWith(expect.any(String), "Oak-job-schedule.csv", "csv");
    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't save the schedule CSV: blocked");
    expect(screen.getByRole("button", { name: "Download printable schedule" })).toBeVisible();
    expect(screen.getByText("Exports include all tasks. Open the HTML file to print. Choose a new filename; existing files are kept.")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    expect(save).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
