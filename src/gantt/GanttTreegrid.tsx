import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import type { GanttReadModel, GanttRow } from "../types/gantt";
import { GANTT_ZOOMS, GanttTimeline, type GanttZoom } from "./GanttTimeline";
import { selectVisibleGanttRows } from "./visibleRows";
import "./ganttTreegrid.css";

const ROW_HEIGHT_FALLBACK = 28;
const HEADER_HEIGHT = 34;
const DEFAULT_VIEWPORT_HEIGHT = 520;
const OVERSCAN = 8;

const columns = ["WBS", "Name", "Duration", "Start", "Finish", "Predecessors", "Float"] as const;
const TREE_COLUMN_INDEX = 1;

interface ActiveCell {
  taskId: string;
  columnIndex: number;
}

export interface GanttTreegridProps {
  readModel: GanttReadModel;
  ariaLabel?: string;
  viewportHeight?: number;
}

/** Authoritative, keyboard-operable work-breakdown projection for a Gantt schedule. */
export function GanttTreegrid({
  readModel,
  ariaLabel = "Work breakdown schedule",
  viewportHeight = DEFAULT_VIEWPORT_HEIGHT,
}: GanttTreegridProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowHeightProbeRef = useRef<HTMLDivElement>(null);
  const cellRefs = useRef(new Map<string, HTMLTableCellElement>());
  const shouldRestoreFocusRef = useRef(false);
  const [rowHeight, setRowHeight] = useState(ROW_HEIGHT_FALLBACK);
  const [zoom, setZoom] = useState<GanttZoom>("week");
  const [zoomAnchor, setZoomAnchor] = useState<{
    minute: number;
    screenX: number;
  } | null>(null);
  const [hoveredTaskId, setHoveredTaskId] = useState<string | null>(null);
  const [collapsedTaskIds, setCollapsedTaskIds] = useState<ReadonlySet<string>>(() => new Set<string>());
  const visibleRows = useMemo(() => selectVisibleGanttRows(readModel, collapsedTaskIds), [collapsedTaskIds, readModel]);
  const [activeCell, setActiveCell] = useState<ActiveCell | null>(() =>
    readModel.rows[0] ? { taskId: readModel.rows[0].taskId, columnIndex: 0 } : null,
  );

  useLayoutEffect(() => {
    const probe = rowHeightProbeRef.current;
    if (!probe) return;
    const updateHeight = () => {
      const configuredHeight = probe.getBoundingClientRect().height;
      if (Number.isFinite(configuredHeight) && configuredHeight > 0) {
        setRowHeight(configuredHeight);
      }
    };
    updateHeight();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(updateHeight);
    observer.observe(probe);
    return () => observer.disconnect();
  }, []);

  const rowVirtualizer = useVirtualizer({
    count: visibleRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    getItemKey: (index) => visibleRows[index]?.taskId ?? index,
    overscan: OVERSCAN,
    initialRect: { width: 1100, height: viewportHeight },
    observeElementRect: (_instance, callback) => {
      callback({
        width: scrollRef.current?.clientWidth || 1100,
        height: viewportHeight,
      });
      return () => undefined;
    },
  });
  const virtualRows = rowVirtualizer.getVirtualItems();
  const activeCellIsMounted = virtualRows.some(
    (virtualRow) => visibleRows[virtualRow.index]?.taskId === activeCell?.taskId,
  );
  const scrollOffset = rowVirtualizer.scrollOffset ?? 0;
  const firstFullyVisibleVirtualRow = virtualRows.find(
    (virtualRow) => virtualRow.start >= scrollOffset + HEADER_HEIGHT,
  );
  const fallbackVirtualRow = firstFullyVisibleVirtualRow ?? virtualRows[virtualRows.length - 1];
  const fallbackRow = visibleRows[fallbackVirtualRow?.index ?? -1];
  const tabStopCell = activeCellIsMounted
    ? activeCell
    : fallbackRow
      ? {
          taskId: fallbackRow.taskId,
          columnIndex: activeCell?.columnIndex ?? 0,
        }
      : null;
  const topSpacer = virtualRows[0]?.start ?? 0;
  const bottomSpacer = Math.max(0, rowVirtualizer.getTotalSize() - (virtualRows[virtualRows.length - 1]?.end ?? 0));

  const focusMountedCell = useCallback((target: HTMLTableCellElement) => {
    target.focus({ preventScroll: true });
    const scrollport = scrollRef.current;
    if (!scrollport) return;

    const targetRect = target.getBoundingClientRect();
    const scrollportRect = scrollport.getBoundingClientRect();
    const focusInset = 4;
    const visibleTop = scrollportRect.top + HEADER_HEIGHT + focusInset;
    const visibleBottom = scrollportRect.bottom - focusInset;
    if (targetRect.bottom > visibleBottom) {
      scrollport.scrollTop += targetRect.bottom - visibleBottom;
    } else if (targetRect.top < visibleTop) {
      scrollport.scrollTop += targetRect.top - visibleTop;
    }
    if (targetRect.right > scrollportRect.right - focusInset) {
      scrollport.scrollLeft += targetRect.right - scrollportRect.right + focusInset;
    } else if (targetRect.left < scrollportRect.left + focusInset) {
      scrollport.scrollLeft += targetRect.left - scrollportRect.left - focusInset;
    }
  }, []);

  const focusCell = useCallback(
    (next: ActiveCell, rowIndex: number) => {
      setActiveCell(next);
      rowVirtualizer.scrollToIndex(rowIndex, { align: "auto" });

      const mountedTarget = cellRefs.current.get(cellKey(next));
      if (mountedTarget) {
        focusMountedCell(mountedTarget);
        return;
      }

      let attempts = 0;
      const tryFocus = () => {
        const target = cellRefs.current.get(cellKey(next));
        if (target) {
          focusMountedCell(target);
          return;
        }
        attempts += 1;
        if (attempts < 4) requestAnimationFrame(tryFocus);
      };
      requestAnimationFrame(tryFocus);
    },
    [focusMountedCell, rowVirtualizer],
  );

  useEffect(() => {
    if (visibleRows.length === 0) {
      setActiveCell(null);
      return;
    }
    if (!activeCell) {
      setActiveCell({ taskId: visibleRows[0]!.taskId, columnIndex: 0 });
      return;
    }
    const activeRowIndex = visibleRows.findIndex((row) => row.taskId === activeCell.taskId);
    if (activeRowIndex >= 0) {
      if (shouldRestoreFocusRef.current && !cellRefs.current.has(cellKey(activeCell))) {
        focusCell(activeCell, activeRowIndex);
      }
      return;
    }

    const hiddenRow = readModel.rows.find((row) => row.taskId === activeCell.taskId);
    let parentId = hiddenRow?.parentTaskId ?? null;
    while (parentId) {
      const visibleParentIndex = visibleRows.findIndex((row) => row.taskId === parentId);
      if (visibleParentIndex >= 0) {
        const next = { taskId: parentId, columnIndex: activeCell.columnIndex };
        if (shouldRestoreFocusRef.current) focusCell(next, visibleParentIndex);
        else setActiveCell(next);
        return;
      }
      parentId = readModel.rows.find((row) => row.taskId === parentId)?.parentTaskId ?? null;
    }
    const next = {
      taskId: visibleRows[0]!.taskId,
      columnIndex: activeCell.columnIndex,
    };
    if (shouldRestoreFocusRef.current) focusCell(next, 0);
    else setActiveCell(next);
  }, [activeCell, focusCell, readModel.rows, visibleRows]);

  function toggleCollapsed(row: GanttRow) {
    if (!row.hasChildren) return;
    setCollapsedTaskIds((current) => {
      const next = new Set(current);
      if (next.has(row.taskId)) next.delete(row.taskId);
      else next.add(row.taskId);
      return next;
    });
  }

  function moveFocus(rowIndex: number, columnIndex: number) {
    const boundedRowIndex = Math.max(0, Math.min(visibleRows.length - 1, rowIndex));
    const boundedColumnIndex = Math.max(0, Math.min(columns.length - 1, columnIndex));
    const row = visibleRows[boundedRowIndex];
    if (!row) return;
    focusCell({ taskId: row.taskId, columnIndex: boundedColumnIndex }, boundedRowIndex);
  }

  function onCellKeyDown(
    event: KeyboardEvent<HTMLTableCellElement>,
    row: GanttRow,
    rowIndex: number,
    columnIndex: number,
  ) {
    let nextRowIndex = rowIndex;
    let nextColumnIndex = columnIndex;

    switch (event.key) {
      case "ArrowDown":
        nextRowIndex += 1;
        break;
      case "ArrowUp":
        nextRowIndex -= 1;
        break;
      case "PageDown":
        nextRowIndex += Math.max(1, Math.floor(viewportHeight / rowHeight) - 1);
        break;
      case "PageUp":
        nextRowIndex -= Math.max(1, Math.floor(viewportHeight / rowHeight) - 1);
        break;
      case "Home":
        if (event.ctrlKey || event.metaKey) nextRowIndex = 0;
        nextColumnIndex = 0;
        break;
      case "End":
        if (event.ctrlKey || event.metaKey) nextRowIndex = visibleRows.length - 1;
        nextColumnIndex = columns.length - 1;
        break;
      case "ArrowRight":
        if (columnIndex === TREE_COLUMN_INDEX && row.hasChildren && collapsedTaskIds.has(row.taskId)) {
          toggleCollapsed(row);
          event.preventDefault();
          return;
        }
        nextColumnIndex += 1;
        break;
      case "ArrowLeft":
        if (columnIndex === TREE_COLUMN_INDEX && row.hasChildren && !collapsedTaskIds.has(row.taskId)) {
          toggleCollapsed(row);
          event.preventDefault();
          return;
        }
        if (columnIndex === TREE_COLUMN_INDEX && row.parentTaskId) {
          const parentIndex = visibleRows.findIndex((item) => item.taskId === row.parentTaskId);
          if (parentIndex >= 0) nextRowIndex = parentIndex;
        } else {
          nextColumnIndex -= 1;
        }
        break;
      case "Enter":
      case " ":
        if (columnIndex === TREE_COLUMN_INDEX && row.hasChildren) {
          toggleCollapsed(row);
          event.preventDefault();
        }
        return;
      default:
        return;
    }

    event.preventDefault();
    moveFocus(nextRowIndex, nextColumnIndex);
  }

  function activateCell(event: MouseEvent<HTMLTableCellElement>, row: GanttRow, columnIndex: number) {
    const next = { taskId: row.taskId, columnIndex };
    setActiveCell(next);
    event.currentTarget.focus();
  }

  function changeZoom(nextZoom: GanttZoom) {
    const scrollport = scrollRef.current;
    const timeline = scrollport?.querySelector<HTMLElement>(".gantt-timeline-pane");
    if (scrollport && timeline) {
      const domainStart = Number(timeline.dataset.domainStartMinute);
      const dayWidth = Number(timeline.dataset.dayWidth);
      const origin = Number(timeline.dataset.originX);
      const tableWidth = scrollport.querySelector<HTMLElement>(".gantt-treegrid-viewport")?.offsetWidth ?? 0;
      const screenX = tableWidth + Math.max(0, scrollport.clientWidth - tableWidth) / 2;
      const centerContent = scrollport.scrollLeft + screenX;
      const centerTimelineX = centerContent - timeline.offsetLeft;
      setZoomAnchor({
        minute: domainStart + ((centerTimelineX - origin) / dayWidth) * 1_440,
        screenX,
      });
    }
    setZoom(nextZoom);
  }

  return (
    <section className="gantt-schedule">
      <div className="gantt-schedule__toolbar" role="group" aria-label="Timeline zoom">
        {GANTT_ZOOMS.map((option) => (
          <button type="button" key={option} aria-pressed={zoom === option} onClick={() => changeZoom(option)}>
            {option[0]!.toUpperCase() + option.slice(1)}
          </button>
        ))}
      </div>
      <div
        className="gantt-treegrid-scrollport"
        data-testid="gantt-scrollport"
        ref={scrollRef}
        style={{ "--gantt-viewport-height": `${viewportHeight}px` } as CSSProperties}
        onFocusCapture={() => {
          shouldRestoreFocusRef.current = true;
        }}
        onBlurCapture={(event) => {
          if (!event.relatedTarget || !event.currentTarget.contains(event.relatedTarget)) {
            shouldRestoreFocusRef.current = false;
          }
        }}
        onScroll={() => {
          requestAnimationFrame(() => {
            const scrollport = scrollRef.current;
            if (!scrollport || !shouldRestoreFocusRef.current || scrollport.contains(document.activeElement)) {
              return;
            }
            const target = scrollport.querySelector<HTMLTableCellElement>(
              '[role="rowheader"][tabindex="0"], [role="gridcell"][tabindex="0"]',
            );
            if (target) focusMountedCell(target);
          });
        }}
      >
        <div ref={rowHeightProbeRef} className="gantt-treegrid__row-height-probe" aria-hidden="true" />
        <div className="gantt-schedule__split">
          <div className="gantt-treegrid-viewport">
            <table
              className="gantt-treegrid"
              role="treegrid"
              aria-label={ariaLabel}
              aria-rowcount={readModel.rowCount + 1}
              aria-colcount={columns.length}
            >
              {visibleRows.length === 0 ? (
                <caption className="gantt-treegrid__empty">No scheduled tasks</caption>
              ) : null}
              <colgroup>
                <col className="gantt-treegrid__wbs-column" />
                <col className="gantt-treegrid__name-column" />
                <col className="gantt-treegrid__duration-column" />
                <col className="gantt-treegrid__date-column" />
                <col className="gantt-treegrid__date-column" />
                <col className="gantt-treegrid__predecessor-column" />
                <col className="gantt-treegrid__float-column" />
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
                {virtualRows.map((virtualRow) => {
                  const ganttRow = visibleRows[virtualRow.index];
                  if (!ganttRow) return null;
                  return (
                    <TaskRow
                      key={ganttRow.taskId}
                      row={ganttRow}
                      visibleIndex={virtualRow.index}
                      collapsed={collapsedTaskIds.has(ganttRow.taskId)}
                      tabStopCell={tabStopCell}
                      registerCell={(key, element) => {
                        if (element) cellRefs.current.set(key, element);
                        else cellRefs.current.delete(key);
                      }}
                      onActivate={activateCell}
                      onFocusCell={(row, columnIndex) => setActiveCell({ taskId: row.taskId, columnIndex })}
                      onKeyDown={onCellKeyDown}
                      onToggle={toggleCollapsed}
                      onHover={setHoveredTaskId}
                    />
                  );
                })}
                {bottomSpacer > 0 ? <SpacerRow height={bottomSpacer} /> : null}
              </tbody>
            </table>
          </div>
          <GanttTimeline
            readModel={readModel}
            rows={visibleRows}
            virtualRows={virtualRows}
            totalSize={rowVirtualizer.getTotalSize()}
            rowHeight={rowHeight}
            zoom={zoom}
            zoomAnchor={zoomAnchor}
            scrollRef={scrollRef}
            hoveredTaskId={hoveredTaskId}
          />
        </div>
      </div>
    </section>
  );
}

interface TaskRowProps {
  row: GanttRow;
  visibleIndex: number;
  collapsed: boolean;
  tabStopCell: ActiveCell | null;
  registerCell: (key: string, element: HTMLTableCellElement | null) => void;
  onActivate: (event: MouseEvent<HTMLTableCellElement>, row: GanttRow, columnIndex: number) => void;
  onFocusCell: (row: GanttRow, columnIndex: number) => void;
  onKeyDown: (event: KeyboardEvent<HTMLTableCellElement>, row: GanttRow, rowIndex: number, columnIndex: number) => void;
  onToggle: (row: GanttRow) => void;
  onHover: (taskId: string | null) => void;
}

function TaskRow({
  row,
  visibleIndex,
  collapsed,
  tabStopCell,
  registerCell,
  onActivate,
  onFocusCell,
  onKeyDown,
  onToggle,
  onHover,
}: TaskRowProps) {
  const currentStart = formatLocalDateTime(row.start);
  const currentFinish = formatLocalDateTime(row.finish);
  const cells = [
    row.wbs,
    <span className="gantt-treegrid__task" style={{ "--gantt-depth": row.depth } as CSSProperties} key="task">
      {row.hasChildren ? (
        <button
          type="button"
          className="gantt-treegrid__disclosure"
          tabIndex={-1}
          aria-label={`${collapsed ? "Expand" : "Collapse"} ${row.name}`}
          title={`${collapsed ? "Expand" : "Collapse"} ${row.name}`}
          onClick={() => onToggle(row)}
        >
          <svg
            className={collapsed ? undefined : "gantt-treegrid__chevron--expanded"}
            aria-hidden="true"
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="m9 18 6-6-6-6" />
          </svg>
        </button>
      ) : (
        <span className="gantt-treegrid__disclosure-spacer" aria-hidden="true" />
      )}
      <span className="gantt-treegrid__task-copy">
        <span>{row.name}</span>
        <span className="gantt-treegrid__states">
          {row.summary ? <span>Summary</span> : null}
          {row.milestone ? <span>Milestone</span> : null}
          {row.critical ? <span className="gantt-treegrid__critical">Critical</span> : null}
        </span>
      </span>
    </span>,
    row.milestone ? "Milestone" : formatMinutes(row.durationMinutes),
    <ScheduleDate
      key="start"
      current={currentStart}
      baseline={row.baseline ? formatLocalDateTime(row.baseline.start) : null}
      variance={row.baseline?.startVarianceMinutes ?? null}
      constraint={row.startNoEarlierThan ? `≥ ${row.startNoEarlierThan}` : null}
    />,
    <ScheduleDate
      key="finish"
      current={currentFinish}
      baseline={row.baseline ? formatLocalDateTime(row.baseline.finish) : null}
      variance={row.baseline?.finishVarianceMinutes ?? null}
      constraint={row.finishNoLaterThan ? `≤ ${row.finishNoLaterThan}` : null}
    />,
    row.predecessorIds.length > 0 ? row.predecessorIds.join(", ") : "None",
    <FloatValue key="float" row={row} />,
  ];

  return (
    <tr
      className={row.summary ? "gantt-treegrid__summary-row" : undefined}
      aria-rowindex={row.logicalIndex + 2}
      aria-level={row.depth + 1}
      aria-posinset={row.positionInSet}
      aria-setsize={row.setSize}
      aria-expanded={row.hasChildren ? !collapsed : undefined}
      data-task-id={row.taskId}
      onMouseEnter={() => onHover(row.taskId)}
      onMouseLeave={() => onHover(null)}
    >
      {cells.map((content, columnIndex) => {
        const active = tabStopCell?.taskId === row.taskId && tabStopCell.columnIndex === columnIndex;
        const Cell = columnIndex === TREE_COLUMN_INDEX ? "th" : "td";
        return (
          <Cell
            key={columns[columnIndex]}
            role={columnIndex === TREE_COLUMN_INDEX ? "rowheader" : "gridcell"}
            scope={columnIndex === TREE_COLUMN_INDEX ? "row" : undefined}
            aria-label={cellLabel(row, columnIndex)}
            tabIndex={active ? 0 : -1}
            ref={(element) => registerCell(cellKey({ taskId: row.taskId, columnIndex }), element)}
            onClick={(event) => onActivate(event, row, columnIndex)}
            onFocus={() => onFocusCell(row, columnIndex)}
            onKeyDown={(event) => onKeyDown(event, row, visibleIndex, columnIndex)}
          >
            {content}
          </Cell>
        );
      })}
    </tr>
  );
}

function ScheduleDate({
  current,
  baseline,
  variance,
  constraint,
}: {
  current: string;
  baseline: string | null;
  variance: number | null;
  constraint: string | null;
}) {
  return (
    <span className="gantt-treegrid__date gantt-treegrid__date--detail">
      <span data-testid="schedule-current">{current}</span>
      <span className="gantt-treegrid__secondary" data-testid="schedule-baseline">
        {baseline ? `Baseline ${baseline} ${formatSignedMinutes(variance ?? 0)}` : "No baseline"}
      </span>
      {constraint ? <span className="gantt-treegrid__constraint-date" data-testid="schedule-constraint">{constraint}</span> : null}
    </span>
  );
}

function FloatValue({ row }: { row: GanttRow }) {
  if (row.constraintViolated) {
    return (
      <span className="gantt-treegrid__constraint-float" data-testid={`constraint-float-${row.taskId}`}>
        <span>{formatCompactFloat(row.totalFloatMinutes)}</span>
        <span>Constraint</span>
        <span>violated</span>
      </span>
    );
  }
  return (
    <span className={row.totalFloatMinutes < 0 ? "gantt-treegrid__risk" : undefined}>
      {row.totalFloatMinutes < 0 ? (
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          width="12"
          height="12"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M21.73 18 13.73 4a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
          <path d="M12 9v4" />
          <path d="M12 17h.01" />
        </svg>
      ) : null}
      {row.critical
        ? `Critical · ${formatSignedMinutes(row.totalFloatMinutes)}`
        : formatSignedMinutes(row.totalFloatMinutes)}
    </span>
  );
}

function SpacerRow({ height }: { height: number }) {
  return (
    <tr className="gantt-treegrid__spacer" aria-hidden="true">
      <td colSpan={columns.length} style={{ height }} />
    </tr>
  );
}

function cellKey(cell: ActiveCell): string {
  return `${cell.taskId}:${cell.columnIndex}`;
}

function formatLocalDateTime(value: string): string {
  return value.slice(0, 16).replace("T", " ");
}

function formatMinutes(value: number): string {
  return `${value.toLocaleString("en-US")} min`;
}

function formatSignedMinutes(value: number): string {
  if (value === 0) return "0 min";
  return `${value > 0 ? "+" : ""}${value.toLocaleString("en-US")} min`;
}

function formatCompactFloat(value: number): string {
  if (value === 0) return "0";
  return `${value > 0 ? "+" : ""}${value.toLocaleString("en-US")}`;
}

function cellLabel(row: GanttRow, columnIndex: number): string {
  const prefix = `${row.wbs} ${row.name}`;
  switch (columnIndex) {
    case 0:
      return `${prefix}, WBS`;
    case 1:
      return `${prefix}, task, ${row.summary ? "summary" : row.milestone ? "milestone" : "activity"}, ${row.critical ? "critical" : "not critical"}, ${constraintLabel(row)}`;
    case 2:
      return `${prefix}, duration, ${row.milestone ? "milestone" : formatMinutes(row.durationMinutes)}`;
    case 3:
      return `${prefix}, start, ${formatLocalDateTime(row.start)}, ${baselineLabel(row, "start")}`;
    case 4:
      return `${prefix}, finish, ${formatLocalDateTime(row.finish)}, ${baselineLabel(row, "finish")}`;
    case 5:
      return `${prefix}, predecessors, ${row.predecessorIds.length > 0 ? row.predecessorIds.join(", ") : "none"}`;
    default:
      return `${prefix}, total float, ${formatSignedMinutes(row.totalFloatMinutes)}, ${row.critical ? "critical" : "not critical"}, ${row.constraintViolated ? "constraint violated" : "constraint satisfied"}`;
  }
}

function constraintLabel(row: GanttRow): string {
  const constraints = [
    row.startNoEarlierThan ? `start no earlier than ${row.startNoEarlierThan}` : null,
    row.finishNoLaterThan ? `finish no later than ${row.finishNoLaterThan}` : null,
    row.constraintViolated ? "constraint violated" : null,
  ].filter((value): value is string => value !== null);
  return constraints.length > 0 ? constraints.join(", ") : "no task constraints";
}

function baselineLabel(row: GanttRow, field: "start" | "finish"): string {
  if (!row.baseline) return "no baseline";
  const value = field === "start" ? row.baseline.start : row.baseline.finish;
  const variance = field === "start" ? row.baseline.startVarianceMinutes : row.baseline.finishVarianceMinutes;
  return `baseline ${formatLocalDateTime(value)}, variance ${formatSignedMinutes(variance)}`;
}
