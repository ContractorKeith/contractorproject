import type { GanttReadModel, GanttRow } from "../types/gantt";

/** Projects collapsed state without changing the Rust-owned logical row metadata. */
export function selectVisibleGanttRows(
  readModel: GanttReadModel,
  collapsedTaskIds: ReadonlySet<string>,
): GanttRow[] {
  const visibleRows: GanttRow[] = [];
  let hiddenBelowDepth: number | null = null;

  for (const row of readModel.rows) {
    if (hiddenBelowDepth !== null) {
      if (row.depth > hiddenBelowDepth) continue;
      hiddenBelowDepth = null;
    }
    visibleRows.push(row);
    if (row.hasChildren && collapsedTaskIds.has(row.taskId)) {
      hiddenBelowDepth = row.depth;
    }
  }

  return visibleRows;
}
