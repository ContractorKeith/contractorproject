import { describe, expect, it } from "vitest";

import { createGanttFixture, SPIKE_ROW_COUNT } from "./fixture";
import { flattenVisibleRows } from "./projection";

describe("Gantt spike fixture", () => {
  it("creates the same 1,000-row hierarchy and dependency graph every time", () => {
    const first = createGanttFixture();
    const second = createGanttFixture();

    expect(first).toEqual(second);
    expect(first).toHaveLength(SPIKE_ROW_COUNT);
    expect(first.filter((row) => row.depth === 0)).toHaveLength(10);
    expect(first.filter((row) => row.depth === 1)).toHaveLength(90);
    expect(first.filter((row) => row.depth === 2)).toHaveLength(900);
    expect(first.reduce((total, row) => total + row.predecessorIds.length, 0)).toBeGreaterThan(1_400);
  });

  it("removes every descendant of a collapsed summary in one projection", () => {
    const rows = createGanttFixture();

    expect(flattenVisibleRows(rows, new Set(["phase-1"]))).toHaveLength(901);
    expect(flattenVisibleRows(rows, new Set(["phase-1-package-1"]))).toHaveLength(990);
  });
});
