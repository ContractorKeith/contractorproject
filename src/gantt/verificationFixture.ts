import type {
  GanttPredecessorLink,
  GanttProgressStatus,
  GanttReadModel,
  GanttRow,
} from "../types/gantt";

export const GANTT_VERIFICATION_FIXTURE_ID = "contractorproject-gantt-1000-v1";

const fixtureStart = Date.UTC(2026, 7, 17, 8, 0);

function localTimestamp(dayOffset: number, hour = 8): string {
  return new Date(
    fixtureStart + dayOffset * 86_400_000 + (hour - 8) * 3_600_000,
  )
    .toISOString()
    .slice(0, 19);
}

export function createGanttVerificationReadModel(): GanttReadModel {
  const rows: GanttRow[] = [];

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
      startNoEarlierThan: null,
      finishNoLaterThan: null,
      constraintViolated: phase === 0,
      critical: phase === 0,
      milestone: false,
      summary: true,
      // Phase 1 is statused; summaries expose only a derived percent, never actuals.
      percentComplete: phase === 0 ? 42 : 0,
      actualStart: null,
      actualFinish: null,
      progressStatus: phase === 0 ? "inProgress" : "notStarted",
      predecessors: [],
      baseline: null,
    });

    for (let task = 0; task < 99; task += 1) {
      const logicalIndex = rows.length;
      const taskId = `${summaryId}-task-${task + 1}`;
      const taskDay = phaseStartDay + task;
      // Statused phase-1 activities: first three complete, the fourth in progress.
      let percentComplete = 0;
      let actualStart: string | null = null;
      let actualFinish: string | null = null;
      let progressStatus: GanttProgressStatus = "notStarted";
      if (phase === 0 && task <= 2) {
        percentComplete = 100;
        actualStart = localTimestamp(taskDay);
        actualFinish = localTimestamp(taskDay, 17);
        progressStatus = "completed";
      } else if (phase === 0 && task === 3) {
        percentComplete = 50;
        actualStart = localTimestamp(taskDay);
        progressStatus = "inProgress";
      }
      // The immediately-preceding activity is the default FS+0 driver. A few
      // low-index phase-1 activities carry SS, FF, SF, and a negative lag so the
      // browser suite exercises real type-aware dependency-line geometry.
      const priorId = `${summaryId}-task-${task}`;
      const typedLink = (): GanttPredecessorLink => {
        if (task === 4)
          return { taskId: priorId, dependencyType: "SS", lagMinutes: 120 };
        if (task === 5)
          return { taskId: priorId, dependencyType: "FF", lagMinutes: -60 };
        if (task === 6)
          return { taskId: priorId, dependencyType: "SF", lagMinutes: 240 };
        // A deliberately leftward link: the predecessor is a later-day activity,
        // so an FF anchor puts the successor finish LEFT of the predecessor finish
        // and the elbow path routes backward. Exercises finite reversed geometry.
        if (task === 8)
          return {
            taskId: `${summaryId}-task-12`,
            dependencyType: "FF",
            lagMinutes: -240,
          };
        return { taskId: priorId, dependencyType: "FS", lagMinutes: 0 };
      };
      const predecessors: GanttPredecessorLink[] =
        task === 0
          ? []
          : task === 10
            ? [
                { taskId: priorId, dependencyType: "FS", lagMinutes: 0 },
                {
                  taskId: `${summaryId}-task-${task - 1}`,
                  dependencyType: "FS",
                  lagMinutes: 0,
                },
              ]
            : task === 50
              ? [
                  { taskId: priorId, dependencyType: "FS", lagMinutes: 0 },
                  {
                    taskId: `${summaryId}-task-1`,
                    dependencyType: "FS",
                    lagMinutes: 0,
                  },
                ]
              : [typedLink()];
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
        finish:
          task === 98 ? localTimestamp(taskDay) : localTimestamp(taskDay, 17),
        totalFloatMinutes: phase === 0 ? 0 : 480,
        startNoEarlierThan: phase === 0 && task === 0 ? "2026-08-18" : null,
        finishNoLaterThan: phase === 0 && task === 0 ? "2026-08-17" : null,
        constraintViolated: phase === 0 && task === 0,
        critical: phase === 0,
        milestone: task === 98,
        summary: false,
        percentComplete,
        actualStart,
        actualFinish,
        progressStatus,
        predecessors,
        baseline: {
          start: localTimestamp(taskDay - 3),
          finish:
            task === 98
              ? localTimestamp(taskDay - 3)
              : localTimestamp(taskDay - 3, 17),
          durationMinutes: task === 98 ? 0 : 360,
          startVarianceMinutes: 4_320,
          finishVarianceMinutes: 4_320,
          // Current leaves run 480 min against a 360-min baseline (milestones 0).
          durationVarianceMinutes: task === 98 ? 0 : 120,
        },
      });
    }
  }

  return readModel(rows);
}

export function reorderFirstVerificationActivity(
  readModel: GanttReadModel,
): GanttReadModel {
  const reordered = [...readModel.rows];
  const [activity] = reordered.splice(1, 1);
  if (!activity) return readModel;
  reordered.splice(99, 0, activity);

  let phaseOnePosition = 0;
  const rows = reordered.map((row, logicalIndex) => {
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
  return readModelFrom(readModel, rows);
}

function readModel(rows: GanttRow[]): GanttReadModel {
  return {
    contractVersion: 5,
    jobId: GANTT_VERIFICATION_FIXTURE_ID,
    jobVersion: 1,
    scheduleStart: "2026-08-17T08:00:00",
    scheduleFinish: localTimestamp(179, 17),
    // Statused fixture: the data date sits within phase 1's activity window.
    dataDate: localTimestamp(4),
    baselineId: "verification-baseline",
    rowCount: rows.length,
    criticalTaskIds: rows
      .filter((row) => row.critical)
      .map((row) => row.taskId),
    criticalPath: rows
      .filter((row) => row.critical && !row.summary)
      .map((row) => row.taskId),
    rows,
  };
}

function readModelFrom(
  readModel: GanttReadModel,
  rows: GanttRow[],
): GanttReadModel {
  return {
    ...readModel,
    rowCount: rows.length,
    criticalTaskIds: rows
      .filter((row) => row.critical)
      .map((row) => row.taskId),
    criticalPath: rows
      .filter((row) => row.critical && !row.summary)
      .map((row) => row.taskId),
    rows,
  };
}
