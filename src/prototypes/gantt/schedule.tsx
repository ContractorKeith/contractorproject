import { useEffect, useState, type KeyboardEvent, type RefObject } from "react";
import type { VirtualItem } from "@tanstack/react-virtual";

import { dayLabel, finishDay, SPIKE_ROW_COUNT, type SpikeTaskRow } from "./fixture";

export const ROW_HEIGHT = 28;
export const WBS_WIDTH = 704;
export const VIEWPORT_HEIGHT = 532;
export const OVERSCAN = 8;

export const zoomWidths = {
  day: 28,
  week: 8,
  month: 2.4,
  quarter: 0.8,
} as const;

export type Zoom = keyof typeof zoomWidths;
export type VariantKey = "shared-svg" | "split-canvas" | "row-dom";

interface TreegridProps {
  rows: readonly SpikeTaskRow[];
  collapsedIds: ReadonlySet<string>;
  toggleCollapsed: (id: string) => void;
  virtualItems: readonly VirtualItem[];
  totalSize: number;
  scrollRef: RefObject<HTMLDivElement | null>;
  scrollToIndex: (index: number) => void;
}

const columns = ["Task", "Duration", "Start", "Finish", "Float", "Predecessors"] as const;

export function ScheduleColumnHeader({ label }: { label: string }) {
  return (
    <div className="wbs-column-header" aria-hidden="true" title={label}>
      {columns.map((column) => (
        <span key={column}>{column}</span>
      ))}
    </div>
  );
}

export function TimelineScale({ zoom, offset = 0 }: { zoom: Zoom; offset?: number }) {
  const dayWidth = zoomWidths[zoom];
  const step = zoom === "day" ? 7 : zoom === "week" ? 28 : zoom === "month" ? 56 : 91;
  const timelineWidth = Math.ceil(230 * dayWidth);
  return (
    <div className="timeline-ruler" style={{ width: timelineWidth, transform: `translateX(${-offset}px)` }}>
      {Array.from({ length: Math.ceil(230 / step) }, (_, index) => (
        <span key={index} style={{ left: index * step * dayWidth }}>
          Day {index * step + 1}
        </span>
      ))}
    </div>
  );
}

export function VirtualTreegrid({
  rows,
  collapsedIds,
  toggleCollapsed,
  virtualItems,
  totalSize,
  scrollRef,
  scrollToIndex,
}: TreegridProps) {
  const { activeCell, onCellKeyDown, activate } = useGridKeyboard(
    rows,
    collapsedIds,
    toggleCollapsed,
    scrollRef,
    scrollToIndex,
  );
  const topSpacer = virtualItems[0]?.start ?? 0;
  const bottomSpacer = Math.max(
    0,
    totalSize - (virtualItems[virtualItems.length - 1]?.end ?? 0),
  );

  return (
    <table
      className="spike-treegrid spike-treegrid--virtual"
      role="treegrid"
      aria-label="Work breakdown schedule"
      aria-rowcount={SPIKE_ROW_COUNT + 1}
      aria-colcount={columns.length}
    >
      <colgroup>
        <col className="task-column" />
        <col className="duration-column" />
        <col className="date-column" />
        <col className="date-column" />
        <col className="float-column" />
        <col className="predecessor-column" />
      </colgroup>
      <thead>
        <tr aria-rowindex={1}>
          {columns.map((column) => (
            <th scope="col" key={column}>
              {column}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {topSpacer > 0 ? <SpacerRow height={topSpacer} /> : null}
        {virtualItems.map((virtualRow) => {
          const row = rows[virtualRow.index]!;
          return (
            <TaskRow
              key={row.id}
              row={row}
              rowIndex={virtualRow.index}
              collapsed={collapsedIds.has(row.id)}
              activeCell={activeCell}
              activate={activate}
              onCellKeyDown={onCellKeyDown}
              toggleCollapsed={toggleCollapsed}
            />
          );
        })}
        {bottomSpacer > 0 ? <SpacerRow height={bottomSpacer} /> : null}
      </tbody>
    </table>
  );
}

export function FullTreegrid({
  rows,
  collapsedIds,
  toggleCollapsed,
  scrollRef,
  timelineWidth,
  renderTimelineCell,
}: Omit<TreegridProps, "virtualItems" | "totalSize" | "scrollToIndex"> & {
  timelineWidth: number;
  renderTimelineCell: (row: SpikeTaskRow, index: number) => React.ReactNode;
}) {
  const { activeCell, onCellKeyDown, activate } = useGridKeyboard(
    rows,
    collapsedIds,
    toggleCollapsed,
    scrollRef,
    (index) => {
      const target = scrollRef.current?.querySelector<HTMLElement>(`[data-row-index="${index}"]`);
      target?.scrollIntoView({ block: "nearest" });
    },
  );

  return (
    <table
      className="spike-treegrid spike-treegrid--full"
      role="treegrid"
      aria-label="Work breakdown schedule"
      aria-rowcount={SPIKE_ROW_COUNT + 1}
      aria-colcount={columns.length}
    >
      <colgroup>
        <col className="task-column" />
        <col className="duration-column" />
        <col className="date-column" />
        <col className="date-column" />
        <col className="float-column" />
        <col className="predecessor-column" />
        <col className="inline-timeline-column" style={{ width: timelineWidth }} />
      </colgroup>
      <thead>
        <tr aria-rowindex={1}>
          {columns.map((column) => (
            <th scope="col" key={column}>
              {column}
            </th>
          ))}
          <th scope="col" aria-hidden="true">Timeline</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row, index) => (
          <TaskRow
            key={row.id}
            row={row}
            rowIndex={index}
            collapsed={collapsedIds.has(row.id)}
            activeCell={activeCell}
            activate={activate}
            onCellKeyDown={onCellKeyDown}
            toggleCollapsed={toggleCollapsed}
          >
            {renderTimelineCell(row, index)}
          </TaskRow>
        ))}
      </tbody>
    </table>
  );
}

interface TaskRowProps {
  row: SpikeTaskRow;
  rowIndex: number;
  collapsed: boolean;
  activeCell: { rowIndex: number; columnIndex: number };
  activate: (rowIndex: number, columnIndex: number) => void;
  onCellKeyDown: (
    event: KeyboardEvent<HTMLTableCellElement>,
    rowIndex: number,
    columnIndex: number,
  ) => void;
  toggleCollapsed: (id: string) => void;
  children?: React.ReactNode;
}

function TaskRow({
  row,
  rowIndex,
  collapsed,
  activeCell,
  activate,
  onCellKeyDown,
  toggleCollapsed,
  children,
}: TaskRowProps) {
  const values = [
    <span className="task-name" style={{ paddingInlineStart: row.depth * 16 }} key="task">
      {row.kind === "summary" ? (
        <button
          type="button"
          className="disclosure"
          tabIndex={-1}
          aria-label={`${collapsed ? "Expand" : "Collapse"} ${row.name}`}
          onClick={() => toggleCollapsed(row.id)}
        >
          <span aria-hidden="true">{collapsed ? "›" : "⌄"}</span>
        </button>
      ) : (
        <span className="disclosure-placeholder" aria-hidden="true" />
      )}
      <span>
        <span className="wbs-id">{row.wbs}</span> {row.name}
        {row.critical ? <span className="critical-tag">critical</span> : null}
      </span>
    </span>,
    row.kind === "milestone" ? "Milestone" : `${row.durationDays}d`,
    dayLabel(row.startDay),
    dayLabel(finishDay(row)),
    row.totalFloatDays === 0 ? "Critical" : `${row.totalFloatDays > 0 ? "+" : ""}${row.totalFloatDays}d`,
    row.predecessorIds.length > 0 ? row.predecessorIds.map(shortId).join(", ") : "—",
  ];
  const rowAttributes = {
    "aria-level": row.depth + 1,
    "aria-expanded": row.kind === "summary" ? !collapsed : undefined,
    "aria-rowindex": row.logicalIndex + 2,
    "aria-posinset": row.positionInSet,
    "aria-setsize": row.setSize,
    "data-row-id": row.id,
    "data-row-index": rowIndex,
  };

  return (
    <tr {...rowAttributes} className={row.kind === "summary" ? "summary-row" : undefined}>
      {values.map((value, columnIndex) => {
        const active =
          activeCell.rowIndex === rowIndex && activeCell.columnIndex === columnIndex;
        return (
          <td
            key={columns[columnIndex]}
            role="gridcell"
            tabIndex={active ? 0 : -1}
            data-cell={`${row.id}-${columnIndex}`}
            onFocus={() => activate(rowIndex, columnIndex)}
            onKeyDown={(event) => onCellKeyDown(event, rowIndex, columnIndex)}
            aria-label={cellLabel(row, columnIndex)}
          >
            {value}
          </td>
        );
      })}
      {children}
    </tr>
  );
}

function SpacerRow({ height }: { height: number }) {
  return (
    <tr aria-hidden="true" className="virtual-spacer">
      <td colSpan={columns.length} style={{ height }} />
    </tr>
  );
}

function useGridKeyboard(
  rows: readonly SpikeTaskRow[],
  collapsedIds: ReadonlySet<string>,
  toggleCollapsed: (id: string) => void,
  scrollRef: RefObject<HTMLDivElement | null>,
  scrollToIndex: (index: number) => void,
) {
  const [activeCell, setActiveCell] = useState({ rowIndex: 0, columnIndex: 0 });

  useEffect(() => {
    if (activeCell.rowIndex >= rows.length) {
      setActiveCell({ rowIndex: Math.max(rows.length - 1, 0), columnIndex: activeCell.columnIndex });
    }
  }, [activeCell, rows.length]);

  function activate(rowIndex: number, columnIndex: number) {
    setActiveCell({ rowIndex, columnIndex });
  }

  function move(rowIndex: number, columnIndex: number) {
    const nextRow = Math.max(0, Math.min(rows.length - 1, rowIndex));
    const nextColumn = Math.max(0, Math.min(columns.length - 1, columnIndex));
    scrollToIndex(nextRow);
    setActiveCell({ rowIndex: nextRow, columnIndex: nextColumn });
    requestAnimationFrame(() => {
      const target = scrollRef.current?.querySelector<HTMLElement>(
        `[data-cell="${rows[nextRow]?.id}-${nextColumn}"]`,
      );
      target?.focus({ preventScroll: true });
    });
  }

  function onCellKeyDown(
    event: KeyboardEvent<HTMLTableCellElement>,
    rowIndex: number,
    columnIndex: number,
  ) {
    const row = rows[rowIndex]!;
    let targetRow = rowIndex;
    let targetColumn = columnIndex;

    switch (event.key) {
      case "ArrowDown":
        targetRow += 1;
        break;
      case "ArrowUp":
        targetRow -= 1;
        break;
      case "PageDown":
        targetRow += Math.floor(VIEWPORT_HEIGHT / ROW_HEIGHT);
        break;
      case "PageUp":
        targetRow -= Math.floor(VIEWPORT_HEIGHT / ROW_HEIGHT);
        break;
      case "Home":
        if (event.ctrlKey || event.metaKey) targetRow = 0;
        targetColumn = 0;
        break;
      case "End":
        if (event.ctrlKey || event.metaKey) targetRow = rows.length - 1;
        targetColumn = columns.length - 1;
        break;
      case "ArrowRight":
        if (columnIndex === 0 && row.kind === "summary" && collapsedIds.has(row.id)) {
          toggleCollapsed(row.id);
          event.preventDefault();
          return;
        }
        targetColumn += 1;
        break;
      case "ArrowLeft":
        if (columnIndex === 0 && row.kind === "summary" && !collapsedIds.has(row.id)) {
          toggleCollapsed(row.id);
          event.preventDefault();
          return;
        }
        targetColumn -= 1;
        break;
      case "Enter":
      case " ":
        if (columnIndex === 0 && row.kind === "summary") {
          toggleCollapsed(row.id);
          event.preventDefault();
        }
        return;
      default:
        return;
    }

    event.preventDefault();
    move(targetRow, targetColumn);
  }

  return { activeCell, activate, onCellKeyDown };
}

function cellLabel(row: SpikeTaskRow, columnIndex: number): string {
  const prefix = `${row.wbs} ${row.name}`;
  switch (columnIndex) {
    case 0:
      return `${prefix}, ${row.kind}, ${row.critical ? "critical" : "not critical"}, current ${dayLabel(
        row.startDay,
      )} to ${dayLabel(finishDay(row))}, baseline ${dayLabel(row.baselineStartDay)} to ${dayLabel(
        row.baselineStartDay + Math.max(row.baselineDurationDays - 1, 0),
      )}, start variance ${signedDays(row.startDay - row.baselineStartDay)}, predecessors ${predecessorNames(row)}`;
    case 1:
      return `${prefix}, duration ${row.durationDays} days`;
    case 2:
      return `${prefix}, starts ${dayLabel(row.startDay)}`;
    case 3:
      return `${prefix}, finishes ${dayLabel(finishDay(row))}`;
    case 4:
      return `${prefix}, total float ${row.totalFloatDays} days`;
    default:
      return `${prefix}, predecessors ${predecessorNames(row)}`;
  }
}

function shortId(value: string): string {
  return value.replace("phase-", "P").replace("-package-", ".").replace("-task-", ".");
}

function predecessorNames(row: SpikeTaskRow): string {
  return row.predecessorIds.length > 0 ? row.predecessorIds.map(shortId).join(", ") : "none";
}

function signedDays(value: number): string {
  if (value === 0) return "0 days";
  return `${value > 0 ? "+" : ""}${value} days`;
}
