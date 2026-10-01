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

import type {
  GanttDependencyType,
  GanttPredecessorLink,
  GanttReadModel,
  GanttRow,
} from "../types/gantt";
import { formatDayQuantity, formatDays, formatInstant, formatSignedCalendarDays, formatSignedDays } from "./format";
import { GANTT_ZOOMS, GanttTimeline, type GanttZoom } from "./GanttTimeline";
import {
  selectPresentedGanttRows,
  type GanttTaskFilter,
} from "./visibleRows";
import "./ganttTreegrid.css";

const ROW_HEIGHT_FALLBACK = 28;
const HEADER_HEIGHT = 34;
const DEFAULT_VIEWPORT_HEIGHT = 520;
const OVERSCAN = 8;

const columns = ["WBS", "Name", "Duration", "% Done", "Start", "Finish", "Predecessors", "Float"] as const;
const TREE_COLUMN_INDEX = 1;

interface ActiveCell {
  taskId: string;
  columnIndex: number;
}

export interface GanttTreegridProps {
  readModel: GanttReadModel;
  /** Job calendar workday length in minutes. Defaults for older consumers. */
  workdayDurationMinutes?: number;
  ariaLabel?: string;
  viewportHeight?: number;
  todayDate?: string | undefined;
  /** The application starts compact; verification consumers can retain all facts. */
  initialDetailsVisible?: boolean;
  /** Task already selected by a sibling editor when this projection first mounts. */
  initialActiveTaskId?: string | null;
  /** Notified with the focused task id (null when none) so a sibling surface can
   * follow the roving cell — e.g. the schedule-explanation panel. */
  onActiveTaskChange?: (taskId: string | null) => void;
}

/** Authoritative, keyboard-operable work-breakdown projection for a Gantt schedule. */
export function GanttTreegrid({
  readModel,
  workdayDurationMinutes = 480,
  ariaLabel = "Work breakdown schedule",
  viewportHeight = DEFAULT_VIEWPORT_HEIGHT,
  todayDate,
  initialDetailsVisible = true,
  initialActiveTaskId = null,
  onActiveTaskChange,
}: GanttTreegridProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowHeightProbeRef = useRef<HTMLDivElement>(null);
  const cellRefs = useRef(new Map<string, HTMLTableCellElement>());
  const shouldRestoreFocusRef = useRef(false);
  const initialScrollRowHeightRef = useRef<number | null>(null);
  const [rowHeight, setRowHeight] = useState(ROW_HEIGHT_FALLBACK);
  const [zoom, setZoom] = useState<GanttZoom>("week");
  const [zoomAnchor, setZoomAnchor] = useState<{
    minute: number;
    screenX: number;
  } | null>(null);
  const [hoveredTaskId, setHoveredTaskId] = useState<string | null>(null);
  const [collapsedTaskIds, setCollapsedTaskIds] = useState<ReadonlySet<string>>(() => new Set<string>());
  const [filter, setFilter] = useState<GanttTaskFilter>("all");
  const [search, setSearch] = useState("");
  const [detailsVisible, setDetailsVisible] = useState(initialDetailsVisible);
  const columnCount = detailsVisible ? columns.length : 6;
  const presentation = useMemo(
    () => selectPresentedGanttRows(readModel, collapsedTaskIds, filter, search),
    [collapsedTaskIds, filter, readModel, search],
  );
  const visibleRows = presentation.rows;
  const initialActiveRowIndex = initialActiveTaskId === null
    ? -1
    : visibleRows.findIndex((row) => row.taskId === initialActiveTaskId);
  const [activeCell, setActiveCell] = useState<ActiveCell | null>(() =>
    initialActiveRowIndex >= 0
      ? { taskId: visibleRows[initialActiveRowIndex]!.taskId, columnIndex: 0 }
      : readModel.rows[0] ? { taskId: readModel.rows[0].taskId, columnIndex: 0 } : null,
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
    initialOffset: Math.max(0, initialActiveRowIndex) * ROW_HEIGHT_FALLBACK,
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

  useLayoutEffect(() => {
    if (initialActiveRowIndex < 0 || initialScrollRowHeightRef.current === rowHeight) return;
    // The initial offset uses the 28px fallback before CSS is measured. Re-run
    // positioning after measurement so an offscreen selected task receives the
    // mounted roving tab stop without taking document focus.
    rowVirtualizer.scrollToIndex(initialActiveRowIndex, { align: "auto" });
    initialScrollRowHeightRef.current = rowHeight;
  }, [initialActiveRowIndex, rowHeight, rowVirtualizer]);
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

  // Follow the focused task id (including collapse-driven focus recovery, which
  // updates activeCell to the nearest visible ancestor). The grid markup is
  // untouched; this only notifies a sibling surface such as the explanation panel.
  // The callback is held in a ref so an inline-lambda consumer cannot re-fire the
  // effect or loop; the notify depends only on the focused task id.
  const onActiveTaskChangeRef = useRef(onActiveTaskChange);
  onActiveTaskChangeRef.current = onActiveTaskChange;
  useEffect(() => {
    onActiveTaskChangeRef.current?.(activeCell?.taskId ?? null);
  }, [activeCell?.taskId]);

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
    const boundedColumnIndex = Math.max(0, Math.min(columnCount - 1, columnIndex));
    const row = visibleRows[boundedRowIndex];
    if (!row) return;
    focusCell({ taskId: row.taskId, columnIndex: boundedColumnIndex }, boundedRowIndex);
  }

  function onCellKeyDown(
    event: KeyboardEvent<HTMLTableCellElement>,
    row: GanttRow,
    rowIndex: number,
    columnIndex: number,
    hierarchyReadOnly = false,
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
        nextColumnIndex = columnCount - 1;
        break;
      case "ArrowRight":
        if (!hierarchyReadOnly && columnIndex === TREE_COLUMN_INDEX && row.hasChildren && collapsedTaskIds.has(row.taskId)) {
          toggleCollapsed(row);
          event.preventDefault();
          return;
        }
        nextColumnIndex += 1;
        break;
      case "ArrowLeft":
        if (!hierarchyReadOnly && columnIndex === TREE_COLUMN_INDEX && row.hasChildren && !collapsedTaskIds.has(row.taskId)) {
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
        if (!hierarchyReadOnly && columnIndex === TREE_COLUMN_INDEX && row.hasChildren) {
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

  function resetPresentation() {
    setFilter("all");
    setSearch("");
  }

  function toggleDetails() {
    const next = !detailsVisible;
    setDetailsVisible(next);
    // Keep the roving tab stop on the same task when its diagnostic cell hides.
    if (!next) setActiveCell((current) => current ? { ...current, columnIndex: Math.min(current.columnIndex, 5) } : null);
  }

  return (
    <section className={`gantt-schedule${detailsVisible ? "" : " gantt-schedule--compact"}`}>
      <div className="gantt-schedule__toolbar">
        <div className="gantt-schedule__filters" role="group" aria-label="Schedule filters">
          <label className="gantt-schedule__search">
            <span className="visually-hidden">Search tasks or work breakdown</span>
            <input
              type="search"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search tasks or WBS"
              aria-label="Search tasks or WBS"
            />
          </label>
          {(
            [
              ["all", "All work"],
              ["attention", "Needs attention"],
              ["inProgress", "In progress"],
              ["completed", "Completed"],
            ] as const
          ).map(([value, label]) => (
            <button
              type="button"
              key={value}
              aria-pressed={filter === value}
              onClick={() => setFilter(value)}
            >
              {label}
            </button>
          ))}
          {presentation.filtering ? (
            <button type="button" className="gantt-schedule__reset" onClick={resetPresentation}>
              Reset
            </button>
          ) : null}
          <span className="gantt-schedule__result-count" aria-live="polite">
            {presentation.matchCount === 1 ? "1 result" : `${presentation.matchCount} results`}
          </span>
        </div>
        <div className="gantt-schedule__zoom" role="group" aria-label="Timeline zoom">
          <button type="button" aria-pressed={detailsVisible} onClick={toggleDetails}>
            {detailsVisible ? "Hide schedule details" : "Show schedule details"}
          </button>
          {GANTT_ZOOMS.map((option) => (
            <button type="button" key={option} aria-pressed={zoom === option} onClick={() => changeZoom(option)}>
              {option[0]!.toUpperCase() + option.slice(1)}
            </button>
          ))}
        </div>
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
              aria-colcount={columnCount}
            >
              {visibleRows.length === 0 ? (
                <caption className="gantt-treegrid__empty">
                  {presentation.filtering ? "No matching tasks" : "No scheduled tasks"}
                </caption>
              ) : null}
              <colgroup>
                <col className="gantt-treegrid__wbs-column" />
                <col className="gantt-treegrid__name-column" />
                <col className="gantt-treegrid__duration-column" />
                <col className="gantt-treegrid__progress-column" />
                <col className="gantt-treegrid__date-column" />
                <col className="gantt-treegrid__date-column" />
                {detailsVisible ? <><col className="gantt-treegrid__predecessor-column" /><col className="gantt-treegrid__float-column" /></> : null}
              </colgroup>
              <thead>
                <tr aria-rowindex={1}>
                  {columns.slice(0, columnCount).map((column) => (
                    <th scope="col" key={column}>
                      {column}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {topSpacer > 0 ? <SpacerRow height={topSpacer} columnCount={columnCount} /> : null}
                {virtualRows.map((virtualRow) => {
                  const ganttRow = visibleRows[virtualRow.index];
                  if (!ganttRow) return null;
                  return (
                    <TaskRow
                      key={ganttRow.taskId}
                      row={ganttRow}
                      workdayDurationMinutes={workdayDurationMinutes}
                      visibleIndex={virtualRow.index}
                      collapsed={presentation.filtering ? false : collapsedTaskIds.has(ganttRow.taskId)}
                      hierarchyReadOnly={presentation.filtering}
                      detailsVisible={detailsVisible}
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
                {bottomSpacer > 0 ? <SpacerRow height={bottomSpacer} columnCount={columnCount} /> : null}
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
            todayDate={todayDate}
          />
        </div>
      </div>
    </section>
  );
}

interface TaskRowProps {
  row: GanttRow;
  workdayDurationMinutes: number;
  visibleIndex: number;
  collapsed: boolean;
  hierarchyReadOnly: boolean;
  detailsVisible: boolean;
  tabStopCell: ActiveCell | null;
  registerCell: (key: string, element: HTMLTableCellElement | null) => void;
  onActivate: (event: MouseEvent<HTMLTableCellElement>, row: GanttRow, columnIndex: number) => void;
  onFocusCell: (row: GanttRow, columnIndex: number) => void;
  onKeyDown: (
    event: KeyboardEvent<HTMLTableCellElement>,
    row: GanttRow,
    rowIndex: number,
    columnIndex: number,
    hierarchyReadOnly: boolean,
  ) => void;
  onToggle: (row: GanttRow) => void;
  onHover: (taskId: string | null) => void;
}

function TaskRow({
  row,
  workdayDurationMinutes,
  visibleIndex,
  collapsed,
  hierarchyReadOnly,
  detailsVisible,
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
      {row.hasChildren && !hierarchyReadOnly ? (
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
        <span title={row.name}>{row.name}</span>
        <span className="gantt-treegrid__states">
          {row.summary ? <span>Summary</span> : null}
          {row.milestone ? <span>Milestone</span> : null}
          {row.critical ? <span className="gantt-treegrid__critical">Critical</span> : null}
        </span>
      </span>
    </span>,
    detailsVisible ? <DurationValue key="duration" row={row} workdayDurationMinutes={workdayDurationMinutes} /> : <span key="duration">{row.milestone ? "Milestone" : formatDayQuantity(row.durationMinutes, workdayDurationMinutes)}</span>,
    <ProgressValue key="progress" row={row} />,
    detailsVisible ? <ScheduleDate
      key="start"
      current={currentStart}
      baseline={row.baseline ? formatLocalDate(row.baseline.start) : null}
      variance={row.baseline?.startVarianceMinutes ?? null}
      constraint={row.startNoEarlierThan ? `≥ ${row.startNoEarlierThan}` : null}
    /> : <span key="start" title={currentStart}>{formatLocalDate(row.start)}</span>,
    detailsVisible ? <ScheduleDate
      key="finish"
      current={currentFinish}
      baseline={row.baseline ? formatLocalDate(row.baseline.finish) : null}
      variance={row.baseline?.finishVarianceMinutes ?? null}
      constraint={row.finishNoLaterThan ? `≤ ${row.finishNoLaterThan}` : null}
    /> : <span key="finish" title={currentFinish}>{formatLocalDate(row.finish)}</span>,
    <PredecessorValue key="predecessors" row={row} workdayDurationMinutes={workdayDurationMinutes} />,
    <FloatValue key="float" row={row} workdayDurationMinutes={workdayDurationMinutes} />,
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
      {cells.slice(0, detailsVisible ? columns.length : 6).map((content, columnIndex) => {
        const active = tabStopCell?.taskId === row.taskId && tabStopCell.columnIndex === columnIndex;
        const Cell = columnIndex === TREE_COLUMN_INDEX ? "th" : "td";
        return (
          <Cell
            key={columns[columnIndex]}
            role={columnIndex === TREE_COLUMN_INDEX ? "rowheader" : "gridcell"}
            scope={columnIndex === TREE_COLUMN_INDEX ? "row" : undefined}
            aria-label={cellLabel(row, columnIndex, workdayDurationMinutes)}
            tabIndex={active ? 0 : -1}
            ref={(element) => registerCell(cellKey({ taskId: row.taskId, columnIndex }), element)}
            onClick={(event) => onActivate(event, row, columnIndex)}
            onFocus={() => onFocusCell(row, columnIndex)}
            onKeyDown={(event) => onKeyDown(event, row, visibleIndex, columnIndex, hierarchyReadOnly)}
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
        {baseline ? `Baseline ${baseline} ${compactCalendarVariance(variance ?? 0)}` : "No baseline"}
      </span>
      {constraint ? <span className="gantt-treegrid__constraint-date" data-testid="schedule-constraint">{constraint}</span> : null}
    </span>
  );
}

function DurationValue({ row, workdayDurationMinutes }: { row: GanttRow; workdayDurationMinutes: number }) {
  // Visible duration plus the Rust-derived baseline duration variance fact, so
  // schedule slippage in scope is text, not color-only. Summaries carry no
  // baseline and simply show their rolled-up duration. Milestones (zero-length)
  // suppress the duration fact — their slippage already shows on Start/Finish.
  const baseline = row.baseline;
  const showBaseline = baseline != null && !row.milestone;
  return (
    <span className="gantt-treegrid__duration gantt-treegrid__date--detail">
      <span data-testid={`duration-current-${row.taskId}`}>
        {row.milestone ? "Milestone" : formatDayQuantity(row.durationMinutes, workdayDurationMinutes)}
      </span>
      {showBaseline ? (
        <span className="gantt-treegrid__secondary" data-testid={`duration-baseline-${row.taskId}`}>
          {baselineDurationFact(baseline, workdayDurationMinutes)}
        </span>
      ) : null}
    </span>
  );
}

function ProgressValue({ row }: { row: GanttRow }) {
  // Visible percent text plus a compact status word so state is never color-only.
  return (
    <span
      className={`gantt-treegrid__progress gantt-treegrid__progress--${row.progressStatus}`}
      data-testid={`progress-${row.taskId}`}
    >
      <span className="gantt-treegrid__progress-numeral">{`${row.percentComplete}%`}</span>
      <span className="gantt-treegrid__progress-status">{progressStatusText(row.progressStatus)}</span>
    </span>
  );
}

function FloatValue({ row, workdayDurationMinutes }: { row: GanttRow; workdayDurationMinutes: number }) {
  if (row.constraintViolated) {
    return (
      <span className="gantt-treegrid__constraint-float" data-testid={`constraint-float-${row.taskId}`}>
        <span>{formatSignedDays(row.totalFloatMinutes, workdayDurationMinutes)}</span>
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
        ? `Critical · ${formatSignedDays(row.totalFloatMinutes, workdayDurationMinutes)}`
        : formatSignedDays(row.totalFloatMinutes, workdayDurationMinutes)}
    </span>
  );
}

// The Predecessors cell always renders explicit per-link annotations so a bare
// id can never be mistaken for FS+0. Each link reads "T2 SS +0.25 days" (the lag
// text is dropped at zero: "T2 FS"). Links flow comma-separated and wrap in
// reading order; the inline flow keeps multi-link cells compact enough to hold
// spare vertical track lines at the compact wide-screen row height.
function PredecessorValue({ row, workdayDurationMinutes }: { row: GanttRow; workdayDurationMinutes: number }) {
  if (row.predecessors.length === 0) {
    return <span data-testid={`predecessors-${row.taskId}`}>None</span>;
  }
  return (
    <span className="gantt-treegrid__predecessors" data-testid={`predecessors-${row.taskId}`}>
      {row.predecessors.map((link) => predecessorAnnotation(link, workdayDurationMinutes)).join(", ")}
    </span>
  );
}

// Visible annotation for one predecessor link, e.g. "T2 SS +0.25 days" or "T2 FS".
function predecessorAnnotation(link: GanttPredecessorLink, workdayDurationMinutes: number): string {
  if (link.lagMinutes === 0) return `${link.taskId} ${link.dependencyType}`;
  return `${link.taskId} ${link.dependencyType} ${formatSignedDays(link.lagMinutes, workdayDurationMinutes)}`;
}

// Full spoken relationship name for a dependency type code.
function dependencyTypeName(type: GanttDependencyType): string {
  switch (type) {
    case "FS":
      return "finish-to-start";
    case "SS":
      return "start-to-start";
    case "FF":
      return "finish-to-finish";
    default:
      return "start-to-finish";
  }
}

// Accessible fragment for one predecessor link, joined into the row label,
// e.g. "predecessor T2, start-to-start, lag +0.25 days".
function predecessorLinkLabel(link: GanttPredecessorLink, workdayDurationMinutes: number): string {
  const lag =
    link.lagMinutes === 0
      ? "no lag"
      : `lag ${formatSignedDays(link.lagMinutes, workdayDurationMinutes)}`;
  return `predecessor ${link.taskId}, ${dependencyTypeName(link.dependencyType)}, ${lag}`;
}

function SpacerRow({ height, columnCount }: { height: number; columnCount: number }) {
  return (
    <tr className="gantt-treegrid__spacer" aria-hidden="true">
      <td colSpan={columnCount} style={{ height }} />
    </tr>
  );
}

function cellKey(cell: ActiveCell): string {
  return `${cell.taskId}:${cell.columnIndex}`;
}

// Shared with the explanation panel; the lockstep instant format is a contract.
const formatLocalDateTime = formatInstant;

// Civil date only for all schedule surfaces.
function formatLocalDate(value: string): string {
  return value.slice(0, 10);
}

// Visible baseline duration fact, with duration and variance in working days.
function baselineDurationFact(baseline: NonNullable<GanttRow["baseline"]>, workdayDurationMinutes: number): string {
  return `Baseline ${formatDays(baseline.durationMinutes, workdayDurationMinutes)} ${formatSignedDays(baseline.durationVarianceMinutes, workdayDurationMinutes)}`;
}

function compactCalendarVariance(minutes: number): string {
  return formatSignedCalendarDays(minutes).replace("calendar day", "cal. day");
}

function cellLabel(row: GanttRow, columnIndex: number, workdayDurationMinutes: number): string {
  const prefix = `${row.wbs} ${row.name}`;
  switch (columnIndex) {
    case 0:
      return `${prefix}, WBS`;
    case 1:
      return `${prefix}, task, ${row.summary ? "summary" : row.milestone ? "milestone" : "activity"}, ${row.critical ? "critical" : "not critical"}, ${constraintLabel(row)}`;
    case 2:
      return `${prefix}, duration, ${row.milestone ? "milestone" : formatDayQuantity(row.durationMinutes, workdayDurationMinutes)}, ${baselineDurationLabel(row, workdayDurationMinutes)}`;
    case 3:
      return `${prefix}, percent complete, ${progressLabel(row)}`;
    case 4:
      return `${prefix}, start, ${formatLocalDateTime(row.start)}, ${baselineLabel(row, "start")}`;
    case 5:
      return `${prefix}, finish, ${formatLocalDateTime(row.finish)}, ${baselineLabel(row, "finish")}`;
    case 6:
      return `${prefix}, predecessors, ${row.predecessors.length > 0 ? row.predecessors.map((link) => predecessorLinkLabel(link, workdayDurationMinutes)).join(", ") : "none"}`;
    default:
      return `${prefix}, total float, ${formatSignedDays(row.totalFloatMinutes, workdayDurationMinutes)}, ${row.critical ? "critical" : "not critical"}, ${row.constraintViolated ? "constraint violated" : "constraint satisfied"}`;
  }
}

function progressStatusText(status: GanttRow["progressStatus"]): string {
  if (status === "completed") return "Complete";
  if (status === "inProgress") return "In progress";
  return "Not started";
}

function progressLabel(row: GanttRow): string {
  const facts = [`${row.percentComplete} percent complete`];
  if (row.progressStatus === "completed") facts.push("complete");
  else if (row.progressStatus === "inProgress") facts.push("in progress");
  else facts.push("not started");
  if (row.actualStart) facts.push(`actual start ${formatLocalDateTime(row.actualStart)}`);
  if (row.actualFinish) facts.push(`actual finish ${formatLocalDateTime(row.actualFinish)}`);
  return facts.join(", ");
}

function constraintLabel(row: GanttRow): string {
  const constraints = [
    row.startNoEarlierThan ? `start no earlier than ${row.startNoEarlierThan}` : null,
    row.finishNoLaterThan ? `finish no later than ${row.finishNoLaterThan}` : null,
    row.constraintViolated ? "constraint violated" : null,
  ].filter((value): value is string => value !== null);
  return constraints.length > 0 ? constraints.join(", ") : "no task constraints";
}

function baselineDurationLabel(row: GanttRow, workdayDurationMinutes: number): string {
  // Milestones suppress the visible duration fact, so the label matches.
  if (!row.baseline || row.milestone) return "no baseline";
  return `baseline duration ${formatDayQuantity(row.baseline.durationMinutes, workdayDurationMinutes)}, variance ${formatSignedDays(row.baseline.durationVarianceMinutes, workdayDurationMinutes)}`;
}

function baselineLabel(row: GanttRow, field: "start" | "finish"): string {
  if (!row.baseline) return "no baseline";
  const value = field === "start" ? row.baseline.start : row.baseline.finish;
  const variance = field === "start" ? row.baseline.startVarianceMinutes : row.baseline.finishVarianceMinutes;
  // Start/finish variance is elapsed calendar time, expressed in calendar days.
  return `baseline ${field} ${formatLocalDate(value)}, variance ${formatSignedCalendarDays(variance)}`;
}
