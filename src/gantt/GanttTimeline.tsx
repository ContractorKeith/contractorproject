import { useId, useLayoutEffect, useMemo, useRef, type CSSProperties, type RefObject } from "react";
import type { VirtualItem } from "@tanstack/react-virtual";

import type { GanttReadModel, GanttRow } from "../types/gantt";

export const GANTT_ZOOMS = ["day", "week", "month", "quarter"] as const;
export type GanttZoom = (typeof GANTT_ZOOMS)[number];

const DAY_MINUTES = 1_440;
const HEADER_HEIGHT = 34;
const LEFT_PADDING = 32;
const RIGHT_PADDING = 160;

const dayWidths: Record<GanttZoom, number> = {
  day: 28,
  week: 8,
  month: 2.4,
  quarter: 0.8,
};

interface GanttTimelineProps {
  readModel: GanttReadModel;
  rows: readonly GanttRow[];
  virtualRows: readonly VirtualItem[];
  totalSize: number;
  rowHeight: number;
  zoom: GanttZoom;
  zoomAnchor: { minute: number; screenX: number } | null;
  scrollRef: RefObject<HTMLDivElement | null>;
  hoveredTaskId: string | null;
}

/** Supplemental rendering adapter. Every fact drawn here is also present in the treegrid. */
export function GanttTimeline({
  readModel,
  rows,
  virtualRows,
  totalSize,
  rowHeight,
  zoom,
  zoomAnchor,
  scrollRef,
  hoveredTaskId,
}: GanttTimelineProps) {
  const paneRef = useRef<HTMLDivElement>(null);
  const previousZoomRef = useRef(zoom);
  const markerPrefix = useId().replaceAll(":", "");
  const domain = useMemo(() => timelineDomain(readModel), [readModel]);
  const dayWidth = dayWidths[zoom];
  const timelineWidth = Math.max(
    480,
    Math.ceil(LEFT_PADDING + ((domain.finish - domain.start) / DAY_MINUTES) * dayWidth + RIGHT_PADDING),
  );

  useLayoutEffect(() => {
    const viewport = scrollRef.current;
    const pane = paneRef.current;
    const previousZoom = previousZoomRef.current;
    if (!viewport || !pane) return;
    if (previousZoom === zoom || zoomAnchor === null) return;
    const frame = requestAnimationFrame(() => {
      const currentViewport = scrollRef.current;
      const currentPane = paneRef.current;
      if (!currentViewport || !currentPane) return;
      const relativeMinute = Math.max(0, zoomAnchor.minute - domain.start);
      const nextLeft =
        currentPane.offsetLeft + LEFT_PADDING + (relativeMinute / DAY_MINUTES) * dayWidth - zoomAnchor.screenX;
      currentViewport.scrollLeft = Math.max(0, nextLeft);
      previousZoomRef.current = zoom;
    });
    return () => cancelAnimationFrame(frame);
  }, [dayWidth, domain.start, scrollRef, zoom, zoomAnchor]);

  const firstVirtual = virtualRows[0];
  const lastVirtual = virtualRows[virtualRows.length - 1];
  const cropTop = HEADER_HEIGHT + (firstVirtual?.start ?? 0);
  const cropBottom = HEADER_HEIGHT + (lastVirtual?.end ?? 0);
  const cropHeight = Math.max(0, cropBottom - cropTop);
  const mountedRows = virtualRows.flatMap((virtualRow) => {
    const row = rows[virtualRow.index];
    return row ? [{ row, virtualRow }] : [];
  });
  const rowById = new Map(rows.map((row) => [row.taskId, row]));
  const visibleIndexById = new Map(rows.map((row, index) => [row.taskId, index]));
  const criticalEdges = new Set(
    readModel.criticalPath.slice(1).map((taskId, index) => `${readModel.criticalPath[index]}->${taskId}`),
  );
  const normalMarkerId = `${markerPrefix}-gantt-arrow`;
  const criticalMarkerId = `${markerPrefix}-gantt-arrow-critical`;
  const activeMarkerId = `${markerPrefix}-gantt-arrow-active`;

  return (
    <div
      className="gantt-timeline-pane"
      data-testid="gantt-timeline-viewport"
      role="region"
      aria-label="Schedule timeline"
      tabIndex={0}
      ref={paneRef}
      data-domain-start-minute={domain.start}
      data-day-width={dayWidth}
      data-origin-x={LEFT_PADDING}
    >
      <div
        className="gantt-timeline"
        style={
          {
            width: timelineWidth,
            height: HEADER_HEIGHT + totalSize,
          } as CSSProperties
        }
      >
        <TimelineRuler domainStart={domain.start} width={timelineWidth} dayWidth={dayWidth} zoom={zoom} />
        {cropHeight > 0 ? (
          <svg
            className="gantt-timeline__svg"
            data-testid="gantt-timeline"
            aria-hidden="true"
            width={timelineWidth}
            height={cropHeight}
            viewBox={`0 ${cropTop} ${timelineWidth} ${cropHeight}`}
            style={{ top: cropTop }}
          >
            <defs>
              <marker
                id={normalMarkerId}
                className="gantt-timeline__arrow"
                viewBox="0 0 4 4"
                refX="4"
                refY="2"
                markerWidth="4"
                markerHeight="4"
                orient="auto"
              >
                <path d="M 0 0 L 4 2 L 0 4 Z" />
              </marker>
              <marker
                id={criticalMarkerId}
                className="gantt-timeline__arrow--critical"
                viewBox="0 0 4 4"
                refX="4"
                refY="2"
                markerWidth="4"
                markerHeight="4"
                orient="auto"
              >
                <path d="M 0 0 L 4 2 L 0 4 Z" />
              </marker>
              <marker
                id={activeMarkerId}
                className="gantt-timeline__arrow--active"
                viewBox="0 0 4 4"
                refX="4"
                refY="2"
                markerWidth="4"
                markerHeight="4"
                orient="auto"
              >
                <path d="M 0 0 L 4 2 L 0 4 Z" />
              </marker>
            </defs>
            <TimelineGrid
              domainStart={domain.start}
              width={timelineWidth}
              cropTop={cropTop}
              cropBottom={cropBottom}
              dayWidth={dayWidth}
              zoom={zoom}
            />
            {mountedRows.map(({ row, virtualRow }) => (
              <TimelineRow
                key={row.taskId}
                row={row}
                y={HEADER_HEIGHT + virtualRow.start}
                rowHeight={rowHeight}
                domainStart={domain.start}
                dayWidth={dayWidth}
                rowById={rowById}
                visibleIndexById={visibleIndexById}
                criticalEdges={criticalEdges}
                hoveredTaskId={hoveredTaskId}
                normalMarkerId={normalMarkerId}
                criticalMarkerId={criticalMarkerId}
                activeMarkerId={activeMarkerId}
              />
            ))}
          </svg>
        ) : null}
        {readModel.dataDate ? (
          <div
            className="gantt-timeline__data-date"
            data-testid="gantt-data-date-marker"
            role="img"
            aria-label={`Data date ${formatMarkerDate(readModel.dataDate)}`}
            style={{
              left: minuteToX(parseLocalMinute(readModel.dataDate), domain.start, dayWidth),
              top: HEADER_HEIGHT,
              height: totalSize,
            }}
          >
            <span className="gantt-timeline__data-date-label" aria-hidden="true">
              Data date {formatMarkerDate(readModel.dataDate)}
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function TimelineRuler({
  domainStart,
  width,
  dayWidth,
  zoom,
}: {
  domainStart: number;
  width: number;
  dayWidth: number;
  zoom: GanttZoom;
}) {
  const fineDays = zoom === "day" ? 1 : zoom === "week" ? 7 : zoom === "month" ? 28 : 91;
  const coarseDays = zoom === "day" ? 7 : zoom === "week" ? 28 : zoom === "month" ? 91 : 182;
  return (
    <div className="gantt-timeline__ruler" aria-hidden="true" style={{ width }}>
      <RulerTier
        className="gantt-timeline__ruler-coarse"
        domainStart={domainStart}
        width={width}
        dayWidth={dayWidth}
        stepDays={coarseDays}
      />
      <RulerTier
        className="gantt-timeline__ruler-fine"
        domainStart={domainStart}
        width={width}
        dayWidth={dayWidth}
        stepDays={fineDays}
      />
    </div>
  );
}

function RulerTier({
  className,
  domainStart,
  width,
  dayWidth,
  stepDays,
}: {
  className: string;
  domainStart: number;
  width: number;
  dayWidth: number;
  stepDays: number;
}) {
  const count = Math.ceil(width / (stepDays * dayWidth));
  return (
    <div className={className}>
      {Array.from({ length: count }, (_, index) => {
        const x = LEFT_PADDING + index * stepDays * dayWidth;
        return (
          <span key={index} style={{ left: x }} data-ruler-minute={domainStart + index * stepDays * DAY_MINUTES}>
            {formatRulerDate(domainStart + index * stepDays * DAY_MINUTES)}
          </span>
        );
      })}
    </div>
  );
}

function TimelineGrid({
  domainStart,
  width,
  cropTop,
  cropBottom,
  dayWidth,
  zoom,
}: {
  domainStart: number;
  width: number;
  cropTop: number;
  cropBottom: number;
  dayWidth: number;
  zoom: GanttZoom;
}) {
  const stepDays = zoom === "day" ? 1 : zoom === "week" ? 7 : zoom === "month" ? 28 : 91;
  const count = Math.ceil(width / (stepDays * dayWidth));
  return (
    <g className="gantt-timeline__grid">
      {Array.from({ length: count }, (_, index) => {
        const x = LEFT_PADDING + index * stepDays * dayWidth;
        const date = new Date((domainStart + index * stepDays * DAY_MINUTES) * 60_000);
        const period = date.getUTCDate() <= stepDays;
        return (
          <line
            key={index}
            data-grid-minute={domainStart + index * stepDays * DAY_MINUTES}
            className={period ? "gantt-timeline__period-rule" : undefined}
            x1={x}
            x2={x}
            y1={cropTop}
            y2={cropBottom}
          />
        );
      })}
    </g>
  );
}

function TimelineRow({
  row,
  y,
  rowHeight,
  domainStart,
  dayWidth,
  rowById,
  visibleIndexById,
  criticalEdges,
  hoveredTaskId,
  normalMarkerId,
  criticalMarkerId,
  activeMarkerId,
}: {
  row: GanttRow;
  y: number;
  rowHeight: number;
  domainStart: number;
  dayWidth: number;
  rowById: ReadonlyMap<string, GanttRow>;
  visibleIndexById: ReadonlyMap<string, number>;
  criticalEdges: ReadonlySet<string>;
  hoveredTaskId: string | null;
  normalMarkerId: string;
  criticalMarkerId: string;
  activeMarkerId: string;
}) {
  const x = minuteToX(parseLocalMinute(row.start), domainStart, dayWidth);
  const finishX = minuteToX(parseLocalMinute(row.finish), domainStart, dayWidth);
  // Guard against pre-start actuals (finish before start) producing a negative bar.
  const width = Math.max(2, finishX - x);
  // Progress fill is strictly proportioned from the Rust-provided percent.
  const progressPercent = Math.min(100, Math.max(0, row.percentComplete));
  const progressWidth = (width * progressPercent) / 100;
  const centerY = y + liveCenterOffset(row, rowHeight);
  const barHeight = row.summary ? 8 : 14;
  const barY = centerY - barHeight / 2;

  return (
    <g data-timeline-row-id={row.taskId}>
      <line
        className="gantt-timeline__row-rule"
        data-timeline-row-id={row.taskId}
        x1={0}
        x2="100%"
        y1={y + rowHeight}
        y2={y + rowHeight}
      />
      {row.predecessorIds.map((predecessorId) => {
        const predecessor = rowById.get(predecessorId);
        const predecessorIndex = visibleIndexById.get(predecessorId);
        if (!predecessor || predecessorIndex === undefined) return null;
        const predecessorY = HEADER_HEIGHT + predecessorIndex * rowHeight + liveCenterOffset(predecessor, rowHeight);
        const predecessorX = minuteToX(parseLocalMinute(predecessor.finish), domainStart, dayWidth);
        const dependencyId = `${predecessorId}->${row.taskId}`;
        const critical = criticalEdges.has(dependencyId);
        const active = hoveredTaskId === predecessorId || hoveredTaskId === row.taskId;
        return (
          <path
            key={predecessorId}
            className={dependencyClassName(critical, hoveredTaskId !== null, active)}
            data-dependency={dependencyId}
            data-predecessor-task-id={predecessorId}
            data-successor-task-id={row.taskId}
            data-approach-x={x - 8}
            data-finish-x={x}
            d={dependencyPath(predecessorX, predecessorY, x, centerY)}
            markerEnd={`url(#${active ? activeMarkerId : critical ? criticalMarkerId : normalMarkerId})`}
          />
        );
      })}
      {row.baseline && !row.summary ? (
        <rect
          className="gantt-timeline__baseline"
          data-baseline-task-id={row.taskId}
          x={minuteToX(parseLocalMinute(row.baseline.start), domainStart, dayWidth)}
          y={y + baselineOffset(rowHeight)}
          width={Math.max(
            2,
            minuteToX(parseLocalMinute(row.baseline.finish), domainStart, dayWidth) -
              minuteToX(parseLocalMinute(row.baseline.start), domainStart, dayWidth),
          )}
          height={6}
          rx={1}
        />
      ) : null}
      {row.milestone ? (
        <rect
          className="gantt-timeline__milestone"
          data-timeline-task-id={row.taskId}
          x={x - 5}
          y={centerY - 5}
          width={10}
          height={10}
          transform={`rotate(45 ${x} ${centerY})`}
        />
      ) : row.summary ? (
        <g className="gantt-timeline__summary" data-timeline-task-id={row.taskId}>
          <rect x={x} y={barY} width={width} height={barHeight} />
          <path d={`M ${x} ${barY + barHeight} v 4 M ${x + width} ${barY + barHeight} v 4`} />
        </g>
      ) : (
        <>
          <rect
            className={row.critical ? "gantt-timeline__task gantt-timeline__task--critical" : "gantt-timeline__task"}
            data-timeline-task-id={row.taskId}
            x={x}
            y={barY}
            width={width}
            height={barHeight}
            rx={2}
          />
          {progressWidth > 0 ? (
            <rect
              className="gantt-timeline__task-progress"
              data-progress-task-id={row.taskId}
              x={x}
              y={barY}
              width={progressWidth}
              height={barHeight}
              rx={2}
            />
          ) : null}
        </>
      )}
    </g>
  );
}

function timelineDomain(readModel: GanttReadModel): {
  start: number;
  finish: number;
} {
  const starts = [parseLocalMinute(readModel.scheduleStart)];
  const finishes = [parseLocalMinute(readModel.scheduleFinish)];
  for (const row of readModel.rows) {
    starts.push(parseLocalMinute(row.start));
    finishes.push(parseLocalMinute(row.finish));
    if (row.baseline) {
      starts.push(parseLocalMinute(row.baseline.start));
      finishes.push(parseLocalMinute(row.baseline.finish));
    }
  }
  const start = Math.min(...starts) - DAY_MINUTES;
  const finish = Math.max(...finishes) + DAY_MINUTES;
  return { start, finish: Math.max(start + DAY_MINUTES, finish) };
}

function dependencyClassName(critical: boolean, hasHover: boolean, active: boolean): string {
  return [
    "gantt-timeline__dependency",
    critical ? "gantt-timeline__dependency--critical" : "",
    hasHover && active ? "gantt-timeline__dependency--active" : "",
    hasHover && !active ? "gantt-timeline__dependency--muted" : "",
  ]
    .filter(Boolean)
    .join(" ");
}

function dependencyPath(startX: number, startY: number, finishX: number, finishY: number): string {
  const direction = finishY >= startY ? 1 : -1;
  const radius = 3;
  const laneX = Math.max(startX + 8, finishX + 8);
  const approachX = finishX - 8;
  const approachY = finishY - direction * 11;
  return [
    `M ${startX} ${startY}`,
    `H ${laneX - radius}`,
    `Q ${laneX} ${startY} ${laneX} ${startY + direction * radius}`,
    `V ${approachY - direction * radius}`,
    `Q ${laneX} ${approachY} ${laneX - radius} ${approachY}`,
    `H ${approachX + radius}`,
    `Q ${approachX} ${approachY} ${approachX} ${approachY + direction * radius}`,
    `V ${finishY - direction * radius}`,
    `Q ${approachX} ${finishY} ${approachX + radius} ${finishY}`,
    `H ${finishX}`,
  ].join(" ");
}

function liveCenterOffset(row: GanttRow, rowHeight: number): number {
  if (!row.baseline || row.summary) return rowHeight / 2;
  return (rowHeight - 22) / 2 + 7;
}

function baselineOffset(rowHeight: number): number {
  return (rowHeight - 22) / 2 + 16;
}

function minuteToX(minute: number, domainStart: number, dayWidth: number): number {
  return LEFT_PADDING + ((minute - domainStart) / DAY_MINUTES) * dayWidth;
}

function parseLocalMinute(value: string): number {
  const parsed = Date.parse(`${value.slice(0, 16)}Z`);
  return parsed / 60_000;
}

function formatMarkerDate(value: string): string {
  return value.slice(0, 10);
}

function formatRulerDate(minute: number): string {
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(new Date(minute * 60_000));
}
