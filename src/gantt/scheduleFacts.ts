import type { GanttReadModel, GanttRow } from "../types/gantt";

export interface ScheduleSummaryFacts {
  leafCount: number;
  completedLeafCount: number;
  criticalUnfinishedCount: number;
  attentionCount: number;
  nextUnfinished: GanttRow | null;
}

/** An unfinished leaf needing attention according to Rust-provided facts. */
export function isAttentionTask(row: GanttRow): boolean {
  return !row.summary && row.progressStatus !== "completed" && (row.constraintViolated || row.totalFloatMinutes < 0);
}

/** Derives display facts only; scheduling dates and status remain Rust-owned. */
export function scheduleSummaryFacts(readModel: GanttReadModel): ScheduleSummaryFacts {
  const leaves = readModel.rows.filter((row) => !row.summary);
  const unfinished = leaves.filter((row) => row.progressStatus !== "completed");
  return {
    leafCount: leaves.length,
    completedLeafCount: leaves.length - unfinished.length,
    criticalUnfinishedCount: unfinished.filter((row) => row.critical).length,
    attentionCount: unfinished.filter(isAttentionTask).length,
    nextUnfinished:
      unfinished.reduce<GanttRow | null>((next, row) => {
        if (!next || row.start < next.start || (row.start === next.start && row.logicalIndex < next.logicalIndex)) {
          return row;
        }
        return next;
      }, null),
  };
}
