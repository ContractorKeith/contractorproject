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
    const download = vi.spyOn(exports, "downloadSchedule").mockImplementation(() => undefined);
    const user = userEvent.setup();
    const { rerender } = render(<ScheduleExports readModel={readModel} jobName="Oak job" disabled />);
    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    await user.click(screen.getByRole("button", { name: "Download printable schedule" }));
    expect(download).not.toHaveBeenCalled();
    rerender(<ScheduleExports readModel={readModel} jobName="Oak job" />);
    await user.click(screen.getByRole("button", { name: "Download printable schedule" }));
    expect(download).toHaveBeenCalledWith(expect.any(String), "Oak-job-schedule.html", "text/html");
  });

  it("offers direct exports, reports a failure, and clears it on retry", async () => {
    const download = vi.spyOn(exports, "downloadSchedule")
      .mockImplementationOnce(() => { throw new Error("blocked"); })
      .mockImplementation(() => undefined);
    const user = userEvent.setup();
    render(<ScheduleExports readModel={readModel} jobName="Oak job" />);

    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    expect(exports.downloadSchedule).toHaveBeenCalledWith(expect.any(String), "Oak-job-schedule.csv", "text/csv");
    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't download the schedule CSV. Try again.");
    expect(screen.getByRole("button", { name: "Download printable schedule" })).toBeVisible();
    expect(screen.getByText("Exports include all tasks. Open the HTML file to print.")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Export CSV" }));
    expect(download).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
