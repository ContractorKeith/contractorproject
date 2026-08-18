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
});
