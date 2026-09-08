import type { GanttReadModel, GanttRow } from "../types/gantt";
import { isAttentionTask } from "./scheduleFacts";

export type GanttTaskFilter = "all" | "attention" | "inProgress" | "completed";

export interface GanttRowPresentation {
  rows: GanttRow[];
  /** Number of direct query/filter matches before ancestors are retained. */
  matchCount: number;
  /** True while a query or a non-default status filter deliberately overrides
   * saved hierarchy collapse so matching descendants remain discoverable. */
  filtering: boolean;
}

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

/**
 * Produces a presentation-only slice of the Rust-owned row order. Matching
 * leaves retain each ancestor so a result never loses its work-breakdown
 * context. This is linear in the row count: the reverse pass bubbles each
 * included row to its parent once, rather than repeatedly walking ancestry.
 */
export function selectPresentedGanttRows(
  readModel: GanttReadModel,
  collapsedTaskIds: ReadonlySet<string>,
  filter: GanttTaskFilter,
  search: string,
): GanttRowPresentation {
  const query = search.trim().toLocaleLowerCase();
  const filtering = filter !== "all" || query.length > 0;
  if (!filtering) {
    return {
      rows: selectVisibleGanttRows(readModel, collapsedTaskIds),
      matchCount: readModel.rowCount,
      filtering: false,
    };
  }

  const included = new Set<string>();
  for (const row of readModel.rows) {
    if (matchesPresentation(row, filter, query)) included.add(row.taskId);
  }
  const matchCount = included.size;

  // Rows are pre-ordered, so a reverse pass carries every matching descendant
  // to its parent in O(n) without changing its logical index or tree metadata.
  for (let index = readModel.rows.length - 1; index >= 0; index -= 1) {
    const row = readModel.rows[index]!;
    if (!included.has(row.taskId) || !row.parentTaskId) continue;
    included.add(row.parentTaskId);
  }

  return {
    rows: readModel.rows.filter((row) => included.has(row.taskId)),
    matchCount,
    filtering: true,
  };
}

function matchesPresentation(row: GanttRow, filter: GanttTaskFilter, query: string): boolean {
  if (query && !`${row.name} ${row.wbs}`.toLocaleLowerCase().includes(query)) return false;
  // Status filters intentionally apply to leaves only. Summary status is a
  // derived rollup and including it would double-count or make a parent look
  // like independently observed work.
  if (row.summary) return filter === "all";
  if (filter === "attention") return isAttentionTask(row);
  if (filter === "inProgress") return row.progressStatus === "inProgress";
  if (filter === "completed") return row.progressStatus === "completed";
  return true;
}
