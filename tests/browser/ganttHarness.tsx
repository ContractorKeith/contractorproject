import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { GanttTreegrid } from "../../src/gantt/GanttTreegrid";
import type { GanttReadModel, GanttRow } from "../../src/types/gantt";
import "../../src/styles.css";

const rows: GanttRow[] = [];
const fixtureStart = Date.UTC(2026, 7, 17, 8, 0);

function localTimestamp(dayOffset: number, hour = 8): string {
  return new Date(fixtureStart + dayOffset * 86_400_000 + (hour - 8) * 3_600_000).toISOString().slice(0, 19);
}

for (let phase = 0; phase < 10; phase += 1) {
  const summaryIndex = rows.length;
  const summaryId = `phase-${phase + 1}`;
  const phaseStartDay = phase * 9;
  rows.push({
    taskId: summaryId,
    parentTaskId: null,
    logicalIndex: summaryIndex,
    depth: 0,
    positionInSet: phase + 1,
    setSize: 10,
    sortKey: phase,
    wbs: `${phase + 1}`,
    name: `Phase ${phase + 1}`,
    kind: "summary",
    hasChildren: true,
    durationMinutes: 47_520,
    start: localTimestamp(phaseStartDay),
    finish: localTimestamp(phaseStartDay + 98, 17),
    totalFloatMinutes: phase === 0 ? 0 : 480,
    critical: phase === 0,
    milestone: false,
    summary: true,
    predecessorIds: [],
    baseline: null,
  });

  for (let task = 0; task < 99; task += 1) {
    const logicalIndex = rows.length;
    const taskId = `${summaryId}-task-${task + 1}`;
    const taskDay = phaseStartDay + task;
    const predecessorIds =
      task === 0
        ? []
        : task === 10
          ? [`${summaryId}-task-${task}`, `${summaryId}-task-${task - 1}`]
          : task === 50
            ? [`${summaryId}-task-${task}`, `${summaryId}-task-1`]
            : [`${summaryId}-task-${task}`];
    rows.push({
      taskId,
      parentTaskId: summaryId,
      logicalIndex,
      depth: 1,
      positionInSet: task + 1,
      setSize: 99,
      sortKey: task,
      wbs: `${phase + 1}.${task + 1}`,
      name: `Activity ${task + 1}`,
      kind: task === 98 ? "milestone" : "task",
      hasChildren: false,
      durationMinutes: task === 98 ? 0 : 480,
      start: localTimestamp(taskDay),
      finish: task === 98 ? localTimestamp(taskDay) : localTimestamp(taskDay, 17),
      totalFloatMinutes: phase === 0 ? 0 : 480,
      critical: phase === 0,
      milestone: task === 98,
      summary: false,
      predecessorIds,
      baseline: {
        start: localTimestamp(taskDay - 3),
        finish: task === 98 ? localTimestamp(taskDay - 3) : localTimestamp(taskDay - 3, 17),
        durationMinutes: task === 98 ? 0 : 480,
        startVarianceMinutes: 4_320,
        finishVarianceMinutes: 4_320,
      },
    });
  }
}

function readModel(projectedRows: GanttRow[]): GanttReadModel {
  return {
    contractVersion: 1,
    jobId: "browser-contract",
    jobVersion: 1,
    scheduleStart: "2026-08-17T08:00:00",
    scheduleFinish: localTimestamp(179, 17),
    baselineId: "browser-baseline",
    rowCount: projectedRows.length,
    criticalTaskIds: projectedRows.filter((row) => row.critical).map((row) => row.taskId),
    criticalPath: projectedRows.filter((row) => row.critical && !row.summary).map((row) => row.taskId),
    rows: projectedRows,
  };
}

function reorderFirstActivity(projectedRows: GanttRow[]): GanttRow[] {
  const reordered = [...projectedRows];
  const [activity] = reordered.splice(1, 1);
  if (!activity) return projectedRows;
  reordered.splice(99, 0, activity);

  let phaseOnePosition = 0;
  return reordered.map((row, logicalIndex) => {
    if (row.parentTaskId !== "phase-1") return { ...row, logicalIndex };
    phaseOnePosition += 1;
    return {
      ...row,
      logicalIndex,
      positionInSet: phaseOnePosition,
      sortKey: phaseOnePosition - 1,
      wbs: `1.${phaseOnePosition}`,
    };
  });
}

function BrowserContract() {
  const [projectedRows, setProjectedRows] = useState(rows);

  useEffect(() => {
    const reorder = () => setProjectedRows((current) => reorderFirstActivity(current));
    window.addEventListener("gantt-test-reorder", reorder);
    return () => window.removeEventListener("gantt-test-reorder", reorder);
  }, []);

  return <GanttTreegrid readModel={readModel(projectedRows)} viewportHeight={480} />;
}

const root = document.getElementById("root");
if (!root) throw new Error("Browser contract root is missing");

createRoot(root).render(<BrowserContract />);
