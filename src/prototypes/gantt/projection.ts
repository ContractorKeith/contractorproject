import type { SpikeTaskRow } from "./fixture";

export function flattenVisibleRows(
  rows: readonly SpikeTaskRow[],
  collapsedIds: ReadonlySet<string>,
): SpikeTaskRow[] {
  const hiddenIds = new Set<string>();

  return rows.filter((row) => {
    if (row.parentId && hiddenIds.has(row.parentId)) hiddenIds.add(row.id);
    if (collapsedIds.has(row.id)) hiddenIds.add(row.id);
    return !row.parentId || !hiddenIds.has(row.parentId);
  });
}
