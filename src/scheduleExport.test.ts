import { describe, expect, it, vi } from "vitest";

import {
  createScheduleCsv,
  createScheduleReport,
  downloadSchedule,
  safeScheduleFilename,
} from "./scheduleExport";
import type { GanttReadModel, GanttRow } from "./types/gantt";

function row(overrides: Partial<GanttRow>): GanttRow {
  return {
    taskId: "install",
    parentTaskId: null,
    logicalIndex: 0,
    depth: 0,
    positionInSet: 1,
    setSize: 1,
    sortKey: 0,
    wbs: "1",
    name: "Install",
    kind: "task",
    hasChildren: false,
    durationMinutes: 480,
    start: "2026-08-17T08:00:00",
    finish: "2026-08-17T16:00:00",
    totalFloatMinutes: -60,
    startNoEarlierThan: null,
    finishNoLaterThan: null,
    constraintViolated: false,
    critical: false,
    milestone: false,
    summary: false,
    percentComplete: 25,
    actualStart: null,
    actualFinish: null,
    progressStatus: "inProgress",
    predecessors: [],
    baseline: null,
    explanation: { kind: "scheduled", taskId: "install", primaryDriver: { kind: "scheduleStart" }, otherBindingDrivers: [], startedActualStart: null, calendarGap: null, totalFloatMinutes: -60, critical: false, lateFinishLimit: { kind: "projectFinish" } },
    ...overrides,
  };
}

function model(rows: GanttRow[]): GanttReadModel {
  return { contractVersion: 7, jobId: "job", jobVersion: 1, scheduleStart: "2026-08-17T08:00:00", scheduleFinish: "2026-08-20T16:00:00", dataDate: "2026-08-18T08:00:00", baselineId: null, rowCount: rows.length, criticalTaskIds: [], criticalPath: [], calendar: { workingWeekdays: ["monday"], exceptionDates: [] }, rows };
}

describe("schedule exports", () => {
  const malicious = row({
    taskId: "malicious",
    wbs: " =1+1",
    name: '=HYPERLINK("https://bad.example","click")\nsecond line',
    kind: "milestone",
    durationMinutes: 0,
    start: "2026-08-18T09:00:00",
    finish: "2026-08-18T09:00:00",
    milestone: true,
    baseline: { start: "2026-08-17T09:00:00", finish: "2026-08-17T09:00:00", durationMinutes: 0, startVarianceMinutes: 1440, finishVarianceMinutes: 1440, durationVarianceMinutes: -30 },
    predecessors: [{ taskId: "source", dependencyType: "FF", lagMinutes: -60 }],
  });
  const source = row({ taskId: "source", name: "Prédecessor <unsafe>", wbs: "0" });

  it("exports every read-model row with formula-safe text but signed numeric variance", () => {
    const csv = createScheduleCsv(model([source, malicious]), "@Acme, Inc.");

    expect(csv).toMatch(/^\uFEFFJob name,/);
    expect(csv).toContain("'@Acme, Inc.");
    expect(csv).toContain("' =1+1");
    expect(csv).toContain("'=HYPERLINK");
    expect(csv).toContain('"\'=HYPERLINK(""https://bad.example"",""click"")\nsecond line"');
    expect(csv).toContain("Prédecessor <unsafe> (FF, -60 min)");
    expect(csv).toContain(",-30\r\n");
    expect(csv.split("\r\n")).toHaveLength(4);
  });

  it("reports all rows as self-contained escaped printable HTML and states missing baselines", () => {
    const report = createScheduleReport(model([source, malicious]), "A < B & 'quoted'");

    expect(report).toContain("<meta charset=\"utf-8\">");
    expect(report).toContain("A &lt; B &amp; &#39;quoted&#39;");
    expect(report).toContain("Prédecessor &lt;unsafe&gt; (FF, -60 min)");
    expect(report).toContain("No comparison baseline selected");
    expect(report).toContain("No baseline");
    expect(report).toContain("0 of 2 activities completed");
    expect(report).toContain("Needs attention: negative float");
    expect(report).toContain("start 1440 min, finish 1440 min, duration -30 min variance");
    expect(report).toContain("@page{size:landscape");
    expect(report).toContain("overflow-wrap:anywhere");
    expect(report).not.toContain('href="https://bad.example"');
    expect(report).toContain("@media print");
  });

  it("honestly renders an empty schedule", () => {
    const report = createScheduleReport(model([]), "Empty job");
    expect(report).toContain("Forecast finish</dt><dd>No forecast yet</dd>");
    expect(report).toContain("Progress</dt><dd>0 of 0 activities completed</dd>");
    expect(report).toContain("Schedule rows</dt><dd>0</dd>");
    expect(report).toContain("<tbody></tbody>");
  });

  it("keeps completed and summary rows out of active attention while retaining historical deadline facts", () => {
    const completed = row({ taskId: "done", progressStatus: "completed", totalFloatMinutes: -20, critical: true, constraintViolated: true });
    const summary = row({ taskId: "summary", summary: true, kind: "summary", totalFloatMinutes: -20, constraintViolated: true });
    const report = createScheduleReport(model([completed, summary]), "Job");
    expect(report).toContain("1 of 1 activities completed");
    expect(report).not.toContain("Needs attention: negative float");
    expect(report).not.toContain("Needs attention: deadline constraint violated");
    expect(report.match(/Recorded deadline violation/g)).toHaveLength(2);
    expect(report).toContain("<td>Critical</td><td class=\"attention\">Recorded deadline violation</td>");
  });

  it("uses safe, stable local filenames", () => {
    expect(safeScheduleFilename("  Job / Phase 2 ", "csv")).toBe("Job-Phase-2-schedule.csv");
    expect(safeScheduleFilename("", "html")).toBe("schedule-schedule.html");
  });

  it("always releases its object URL when a browser download click fails", () => {
    const createObjectURL = vi.fn(() => "blob:export");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => { throw new Error("blocked"); });

    expect(() => downloadSchedule("file", "schedule.csv", "text/csv")).toThrow("blocked");
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:export");
    expect(document.querySelector("a[download]")).toBeNull();
    click.mockRestore();
    vi.unstubAllGlobals();
  });

  it("defers object URL cleanup for a successful browser download", () => {
    vi.useFakeTimers();
    const createObjectURL = vi.fn(() => "blob:export");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);

    downloadSchedule("file", "schedule.csv", "text/csv");
    expect(revokeObjectURL).not.toHaveBeenCalled();
    expect(document.querySelector("a[download]")).toBeNull();
    vi.advanceTimersByTime(999);
    expect(revokeObjectURL).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:export");
    click.mockRestore();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });
});
