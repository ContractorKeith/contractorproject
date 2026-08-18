import { describe, expect, it } from "vitest";

import {
  createGanttVerificationReadModel,
  GANTT_VERIFICATION_FIXTURE_ID,
} from "./verificationFixture";

describe("packaged Gantt verification fixture", () => {
  it("contains the stable 1,000-row accessibility and rendering contract", () => {
    const model = createGanttVerificationReadModel();

    expect(model.jobId).toBe(GANTT_VERIFICATION_FIXTURE_ID);
    expect(model.rowCount).toBe(1_000);
    expect(model.rows).toHaveLength(1_000);
    expect(model.rows.filter((row) => row.summary)).toHaveLength(10);
    expect(model.rows.filter((row) => row.milestone)).toHaveLength(10);
    expect(model.rows.filter((row) => row.baseline)).toHaveLength(990);
    // Non-milestone leaves carry a positive baseline duration variance fact.
    expect(
      model.rows.some(
        (row) => row.baseline != null && row.baseline.durationVarianceMinutes > 0,
      ),
    ).toBe(true);
    expect(model.rows.some((row) => row.predecessorIds.length > 1)).toBe(true);
    expect(model.rows.some((row) => row.totalFloatMinutes > 0)).toBe(true);
    expect(model.rows[1]).toMatchObject({
      startNoEarlierThan: "2026-08-18",
      finishNoLaterThan: "2026-08-17",
      constraintViolated: true,
    });
    expect(model.rows[0]?.constraintViolated).toBe(true);
    expect(model.criticalPath).not.toHaveLength(0);
  });

  it("carries a data date and statused phase-1 rows across every progress state", () => {
    const model = createGanttVerificationReadModel();

    expect(model.contractVersion).toBe(4);
    expect(model.dataDate).not.toBeNull();
    expect(model.rows.some((row) => row.progressStatus === "completed")).toBe(true);
    expect(model.rows.some((row) => row.progressStatus === "inProgress")).toBe(true);
    expect(model.rows.some((row) => row.progressStatus === "notStarted")).toBe(true);

    // Completed rows carry both actuals; the in-progress row omits its finish.
    const complete = model.rows.find((row) => row.taskId === "phase-1-task-1")!;
    expect(complete.percentComplete).toBe(100);
    expect(complete.progressStatus).toBe("completed");
    expect(complete.actualStart).not.toBeNull();
    expect(complete.actualFinish).not.toBeNull();

    const running = model.rows.find((row) => row.taskId === "phase-1-task-4")!;
    expect(running.percentComplete).toBe(50);
    expect(running.progressStatus).toBe("inProgress");
    expect(running.actualStart).not.toBeNull();
    expect(running.actualFinish).toBeNull();

    // Summaries expose only a derived percent, never fabricated actuals.
    const summary = model.rows.find((row) => row.taskId === "phase-1")!;
    expect(summary.actualStart).toBeNull();
    expect(summary.actualFinish).toBeNull();
  });
});
