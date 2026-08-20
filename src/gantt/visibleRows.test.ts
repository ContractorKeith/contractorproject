import { describe, expect, it } from "vitest";

import { GANTT_READ_MODEL_VERSION, type GanttReadModel, type GanttRow } from "../types/gantt";
import { selectVisibleGanttRows } from "./visibleRows";

function row(
  taskId: string,
  parentTaskId: string | null,
  logicalIndex: number,
  depth: number,
  hasChildren: boolean,
): GanttRow {
  return {
    taskId,
    parentTaskId,
    logicalIndex,
    depth,
    positionInSet: 1,
    setSize: 1,
    sortKey: 0,
    wbs: String(logicalIndex + 1),
    name: taskId,
    kind: hasChildren ? "summary" : "task",
    hasChildren,
    durationMinutes: 480,
    start: "2026-01-05T08:00:00",
    finish: "2026-01-05T16:00:00",
    totalFloatMinutes: 0,
    startNoEarlierThan: null,
    finishNoLaterThan: null,
    constraintViolated: false,
    critical: true,
    milestone: false,
    summary: hasChildren,
    percentComplete: 0,
    actualStart: null,
    actualFinish: null,
    progressStatus: "notStarted",
    predecessors: [],
    baseline: null,
  };
}

function nestedReadModel(): GanttReadModel {
  const rows = [
    row("phase-1", null, 0, 0, true),
    row("package-1", "phase-1", 1, 1, true),
    row("task-1", "package-1", 2, 2, false),
    row("phase-2", null, 3, 0, true),
    row("task-2", "phase-2", 4, 1, false),
  ];
  return {
    contractVersion: GANTT_READ_MODEL_VERSION,
    jobId: "job-1",
    jobVersion: 5,
    scheduleStart: "2026-01-05T08:00:00",
    scheduleFinish: "2026-01-05T16:00:00",
    dataDate: null,
    baselineId: null,
    rowCount: rows.length,
    criticalTaskIds: rows.map((item) => item.taskId),
    criticalPath: ["task-1"],
    calendar: {
      workingWeekdays: ["monday", "tuesday", "wednesday", "thursday", "friday"],
      exceptionDates: [],
    },
    rows,
  };
}

describe("Gantt visible-row projection", () => {
  it("keeps Rust logical metadata stable while hiding every collapsed descendant", () => {
    const visibleRows = selectVisibleGanttRows(
      nestedReadModel(),
      new Set(["phase-1"]),
    );

    expect(visibleRows.map((item) => item.taskId)).toEqual([
      "phase-1",
      "phase-2",
      "task-2",
    ]);
    expect(visibleRows.map((item) => item.logicalIndex)).toEqual([0, 3, 4]);
  });

  it("returns the authoritative rows unchanged when nothing is collapsed", () => {
    const readModel = nestedReadModel();

    expect(selectVisibleGanttRows(readModel, new Set())).toEqual(readModel.rows);
  });
});
