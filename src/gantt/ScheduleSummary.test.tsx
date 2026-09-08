import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { GanttReadModel, GanttRow } from "../types/gantt";
import { ScheduleSummary } from "./ScheduleSummary";
import { scheduleSummaryFacts } from "./scheduleFacts";

function row(overrides: Partial<GanttRow> & Pick<GanttRow, "taskId" | "logicalIndex" | "name">): GanttRow {
  const { taskId, logicalIndex, name, ...rest } = overrides;
  return {
    taskId,
    logicalIndex,
    name,
    parentTaskId: null,
    depth: 0,
    positionInSet: 1,
    setSize: 1,
    sortKey: logicalIndex,
    wbs: String(logicalIndex + 1),
    kind: "task",
    hasChildren: false,
    durationMinutes: 480,
    start: "2026-08-17T08:00:00",
    finish: "2026-08-17T17:00:00",
    totalFloatMinutes: 0,
    startNoEarlierThan: null,
    finishNoLaterThan: null,
    constraintViolated: false,
    critical: false,
    milestone: false,
    summary: false,
    percentComplete: 0,
    actualStart: null,
    actualFinish: null,
    progressStatus: "notStarted",
    predecessors: [],
    baseline: null,
    explanation: {
      kind: "scheduled",
      taskId,
      primaryDriver: { kind: "scheduleStart" },
      otherBindingDrivers: [],
      startedActualStart: null,
      calendarGap: null,
      totalFloatMinutes: 0,
      critical: false,
      lateFinishLimit: { kind: "projectFinish" },
    },
    ...rest,
  };
}

function model(rows: GanttRow[], dataDate: string | null = null): GanttReadModel {
  return {
    contractVersion: 7,
    jobId: "job-1",
    jobVersion: 1,
    scheduleStart: "2026-08-17T08:00:00",
    scheduleFinish: "2026-08-21T17:00:00",
    dataDate,
    baselineId: null,
    rowCount: rows.length,
    criticalTaskIds: [],
    criticalPath: [],
    calendar: { workingWeekdays: ["monday"], exceptionDates: [] },
    rows,
  };
}

describe("ScheduleSummary", () => {
  it("counts leaves only and keeps attention distinct from critical work", () => {
    const readModel = model([
      row({ taskId: "summary", logicalIndex: 0, name: "Site work", kind: "summary", summary: true, hasChildren: true, critical: true }),
      row({ taskId: "done", logicalIndex: 1, name: "Completed layout", parentTaskId: "summary", progressStatus: "completed", critical: true }),
      row({ taskId: "next", logicalIndex: 2, name: "Set posts", parentTaskId: "summary", start: "2026-08-18T08:00:00", critical: true }),
      row({ taskId: "risk", logicalIndex: 3, name: "Inspection", parentTaskId: "summary", start: "2026-08-19T08:00:00", totalFloatMinutes: -60 }),
    ]);

    expect(scheduleSummaryFacts(readModel)).toMatchObject({
      leafCount: 3,
      completedLeafCount: 1,
      criticalUnfinishedCount: 1,
      attentionCount: 1,
      nextUnfinished: { taskId: "next" },
    });

    render(<ScheduleSummary readModel={readModel} />);
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("1 of 3 complete");
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("Set posts2026-08-18 08:00");
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("No progress recorded yet");
  });

  it("states honest empty and all-complete conditions", () => {
    const { rerender } = render(<ScheduleSummary readModel={model([])} />);
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("No scheduled work");
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("Add scheduled work");

    rerender(<ScheduleSummary readModel={model([row({ taskId: "done", logicalIndex: 0, name: "Closeout", progressStatus: "completed" })], "2026-08-21")} />);
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("All work complete");
    expect(screen.getByRole("region", { name: "Schedule summary" })).toHaveTextContent("Progress recorded through 2026-08-21");
  });
});
