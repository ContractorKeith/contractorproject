import { Fragment, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import type { SpikeTaskRow } from "../fixture";
import {
  OVERSCAN,
  ROW_HEIGHT,
  ScheduleColumnHeader,
  TimelineScale,
  VIEWPORT_HEIGHT,
  VirtualTreegrid,
  WBS_WIDTH,
  zoomWidths,
  type Zoom,
} from "../schedule";

interface SharedViewportSvgProps {
  rows: readonly SpikeTaskRow[];
  allRows: readonly SpikeTaskRow[];
  collapsedIds: ReadonlySet<string>;
  toggleCollapsed: (id: string) => void;
  zoom: Zoom;
  viewportRef: React.RefObject<HTMLDivElement | null>;
  horizontalViewportRef: React.RefObject<HTMLDivElement | null>;
}

export function SharedViewportSvg({
  rows,
  allRows,
  collapsedIds,
  toggleCollapsed,
  zoom,
  viewportRef,
  horizontalViewportRef,
}: SharedViewportSvgProps) {
  const localRef = useRef<HTMLDivElement | null>(null);
  const [scrollLeft, setScrollLeft] = useState(0);
  const setViewport = (element: HTMLDivElement | null) => {
    localRef.current = element;
    viewportRef.current = element;
    horizontalViewportRef.current = element;
  };
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => localRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();
  const dayWidth = zoomWidths[zoom];
  const timelineWidth = Math.ceil(230 * dayWidth);
  const byId = useMemo(() => new Map(allRows.map((row) => [row.id, row])), [allRows]);
  const visibleIndexById = useMemo(
    () => new Map(rows.map((row, index) => [row.id, index])),
    [rows],
  );

  return (
    <section className="schedule-variant" aria-labelledby="variant-title">
      <TimelineHeader zoom={zoom} scrollLeft={scrollLeft} />
      <div
        className="shared-scrollport"
        ref={setViewport}
        style={{ height: VIEWPORT_HEIGHT }}
        data-testid="schedule-scrollport"
        onScroll={(event) => setScrollLeft(event.currentTarget.scrollLeft)}
      >
        <div
          className="shared-content"
          style={{ width: WBS_WIDTH + timelineWidth, height: totalSize }}
        >
          <VirtualTreegrid
            rows={rows}
            collapsedIds={collapsedIds}
            toggleCollapsed={toggleCollapsed}
            virtualItems={virtualItems}
            totalSize={totalSize}
            scrollRef={localRef}
            scrollToIndex={(index) => virtualizer.scrollToIndex(index, { align: "auto" })}
          />
          <svg
            className="svg-timeline"
            width={timelineWidth}
            height={totalSize}
            viewBox={`0 0 ${timelineWidth} ${totalSize}`}
            aria-hidden="true"
            style={{ left: WBS_WIDTH }}
          >
            <TimelineGrid width={timelineWidth} height={totalSize} dayWidth={dayWidth} />
            {virtualItems.map((virtualRow) => {
              const row = rows[virtualRow.index]!;
              return (
                <SvgTask
                  key={row.id}
                  row={row}
                  rowIndex={virtualRow.index}
                  y={virtualRow.start}
                  dayWidth={dayWidth}
                  byId={byId}
                  visibleIndexById={visibleIndexById}
                />
              );
            })}
          </svg>
        </div>
      </div>
    </section>
  );
}

function TimelineHeader({ zoom, scrollLeft }: { zoom: Zoom; scrollLeft: number }) {
  return (
    <div className="schedule-header" aria-hidden="true">
      <div className="wbs-header">
        <ScheduleColumnHeader label="Work breakdown · 1,000 task fixture" />
      </div>
      <div className="timeline-ruler-viewport">
        <TimelineScale zoom={zoom} offset={scrollLeft} />
      </div>
    </div>
  );
}

function TimelineGrid({ width, height, dayWidth }: { width: number; height: number; dayWidth: number }) {
  const step = dayWidth >= 8 ? 7 : dayWidth >= 2 ? 28 : 91;
  return (
    <g className="timeline-grid">
      {Array.from({ length: Math.ceil(230 / step) }, (_, index) => (
        <line
          key={index}
          x1={index * step * dayWidth}
          x2={index * step * dayWidth}
          y1={0}
          y2={height}
        />
      ))}
      <line x1={0} x2={width} y1={0} y2={0} />
    </g>
  );
}

function SvgTask({
  row,
  rowIndex,
  y,
  dayWidth,
  byId,
  visibleIndexById,
}: {
  row: SpikeTaskRow;
  rowIndex: number;
  y: number;
  dayWidth: number;
  byId: ReadonlyMap<string, SpikeTaskRow>;
  visibleIndexById: ReadonlyMap<string, number>;
}) {
  const x = row.startDay * dayWidth;
  const width = Math.max(row.durationDays * dayWidth, 8);
  const baselineX = row.baselineStartDay * dayWidth;
  const baselineWidth = Math.max(row.baselineDurationDays * dayWidth, 6);
  const barY = y + (ROW_HEIGHT - 14) / 2;

  return (
    <g data-timeline-row-id={row.id} data-row-index={rowIndex}>
      <line
        className="timeline-row-rule"
        data-timeline-row-id={row.id}
        x1={0}
        x2="100%"
        y1={y + ROW_HEIGHT}
        y2={y + ROW_HEIGHT}
      />
      {row.predecessorIds.map((predecessorId) => {
        const predecessor = byId.get(predecessorId);
        const predecessorIndex = visibleIndexById.get(predecessorId);
        if (!predecessor || predecessorIndex === undefined) return <Fragment key={predecessorId} />;
        return (
          <path
            key={predecessorId}
            className={row.critical ? "dependency dependency--critical" : "dependency"}
            d={`M ${(predecessor.startDay + predecessor.durationDays) * dayWidth} ${
              predecessorIndex * ROW_HEIGHT + ROW_HEIGHT / 2
            } H ${x - 4} V ${y + ROW_HEIGHT / 2} H ${x}`}
          />
        );
      })}
      {row.kind !== "summary" ? (
        <rect
          className="baseline-bar"
          x={baselineX}
          y={y + ROW_HEIGHT - 7}
          width={baselineWidth}
          height={4}
        />
      ) : null}
      {row.kind === "milestone" ? (
        <rect
          className="milestone-bar"
          x={x}
          y={y + 9}
          width={10}
          height={10}
          transform={`rotate(45 ${x + 5} ${y + 14})`}
        />
      ) : (
        <rect
          className={`task-bar task-bar--${row.kind}${row.critical ? " task-bar--critical" : ""}`}
          x={x}
          y={barY}
          width={width}
          height={row.kind === "summary" ? 8 : 14}
          rx={row.kind === "summary" ? 0 : 2}
        />
      )}
      {dayWidth >= 8 ? (
        <text className="bar-label" x={x + width + 5} y={y + 18}>
          {row.name}
        </text>
      ) : null}
    </g>
  );
}
