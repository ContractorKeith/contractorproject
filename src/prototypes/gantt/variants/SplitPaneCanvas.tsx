import { useEffect, useMemo, useRef, useState } from "react";
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

interface SplitPaneCanvasProps {
  rows: readonly SpikeTaskRow[];
  collapsedIds: ReadonlySet<string>;
  toggleCollapsed: (id: string) => void;
  zoom: Zoom;
  viewportRef: React.RefObject<HTMLDivElement | null>;
  horizontalViewportRef: React.RefObject<HTMLDivElement | null>;
}

export function SplitPaneCanvas({
  rows,
  collapsedIds,
  toggleCollapsed,
  zoom,
  viewportRef,
  horizontalViewportRef,
}: SplitPaneCanvasProps) {
  const tableRef = useRef<HTMLDivElement | null>(null);
  const timelineRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const syncSource = useRef<"table" | "timeline" | null>(null);
  const syncFrame = useRef<number | null>(null);
  const [timelineScroll, setTimelineScroll] = useState({ top: 0, left: 0 });
  const setTableViewport = (element: HTMLDivElement | null) => {
    tableRef.current = element;
    viewportRef.current = element;
  };
  const setTimelineViewport = (element: HTMLDivElement | null) => {
    timelineRef.current = element;
    horizontalViewportRef.current = element;
  };
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => tableRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();
  const dayWidth = zoomWidths[zoom];
  const timelineWidth = Math.ceil(230 * dayWidth);
  const rowIndexById = useMemo(
    () => new Map(rows.map((row, index) => [row.id, index])),
    [rows],
  );

  function synchronize(source: "table" | "timeline") {
    syncSource.current = source;
    if (syncFrame.current !== null) cancelAnimationFrame(syncFrame.current);
    syncFrame.current = requestAnimationFrame(() => {
      if (source === "table" && timelineRef.current && tableRef.current) {
        timelineRef.current.scrollTop = tableRef.current.scrollTop;
      } else if (source === "timeline" && timelineRef.current && tableRef.current) {
        tableRef.current.scrollTop = timelineRef.current.scrollTop;
      }
      if (timelineRef.current) {
        setTimelineScroll({ top: timelineRef.current.scrollTop, left: timelineRef.current.scrollLeft });
      }
      syncSource.current = null;
    });
  }

  useEffect(() => {
    const canvas = canvasRef.current;
    const timeline = timelineRef.current;
    if (!canvas || !timeline) return;
    const width = Math.max(1, timeline.clientWidth);
    const height = Math.max(1, timeline.clientHeight);
    const devicePixelRatio = window.devicePixelRatio || 1;
    canvas.width = Math.ceil(width * devicePixelRatio);
    canvas.height = Math.ceil(height * devicePixelRatio);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
    context.clearRect(0, 0, width, height);
    const styles = getComputedStyle(document.documentElement);
    const normal = styles.getPropertyValue("--sched-normal").trim() || "#749dc4";
    const critical = styles.getPropertyValue("--sched-critical").trim() || "#2c455d";
    const summary = styles.getPropertyValue("--sched-summary").trim() || "#424244";
    const baseline = styles.getPropertyValue("--sched-baseline").trim() || "#b7b7ba";
    const rule = styles.getPropertyValue("--color-divider").trim() || "#d4d4d7";
    context.lineWidth = 1;

    for (const virtualRow of virtualItems) {
      const row = rows[virtualRow.index]!;
      const y = virtualRow.start - timelineScroll.top;
      context.strokeStyle = rule;
      context.beginPath();
      context.moveTo(0, y + ROW_HEIGHT - 0.5);
      context.lineTo(width, y + ROW_HEIGHT - 0.5);
      context.stroke();
      const x = row.startDay * dayWidth - timelineScroll.left;
      const barWidth = Math.max(row.durationDays * dayWidth, 8);
      for (const predecessorId of row.predecessorIds) {
        const predecessorIndex = rowIndexById.get(predecessorId);
        const predecessor = predecessorIndex === undefined ? undefined : rows[predecessorIndex];
        if (!predecessor || predecessorIndex === undefined) continue;
        const predecessorX =
          (predecessor.startDay + predecessor.durationDays) * dayWidth - timelineScroll.left;
        const predecessorY = predecessorIndex * ROW_HEIGHT - timelineScroll.top + ROW_HEIGHT / 2;
        context.strokeStyle = row.critical ? critical : styles.getPropertyValue("--color-neutral-600").trim();
        context.beginPath();
        context.moveTo(predecessorX, predecessorY);
        context.lineTo(x - 4, predecessorY);
        context.lineTo(x - 4, y + ROW_HEIGHT / 2);
        context.lineTo(x, y + ROW_HEIGHT / 2);
        context.stroke();
      }
      context.fillStyle = baseline;
      context.fillRect(
        row.baselineStartDay * dayWidth - timelineScroll.left,
        y + ROW_HEIGHT - 7,
        Math.max(row.baselineDurationDays * dayWidth, 6),
        4,
      );
      context.fillStyle = row.kind === "summary" ? summary : row.critical ? critical : normal;
      if (row.kind === "milestone") {
        context.save();
        context.translate(x + 5, y + 14);
        context.rotate(Math.PI / 4);
        context.fillRect(-5, -5, 10, 10);
        context.restore();
      } else {
        context.fillRect(x, y + (row.kind === "summary" ? 10 : 7), barWidth, row.kind === "summary" ? 8 : 14);
      }
    }
  }, [dayWidth, rowIndexById, rows, timelineScroll, virtualItems]);

  return (
    <section className="schedule-variant split-variant" aria-labelledby="variant-title">
      <div className="split-pane" style={{ height: VIEWPORT_HEIGHT + 34 }}>
        <div className="split-pane__table" style={{ width: WBS_WIDTH }}>
          <div className="split-pane__heading split-pane__heading--wbs">
            <ScheduleColumnHeader label="Work breakdown · separately scrolled" />
          </div>
          <div
            ref={setTableViewport}
            className="split-scrollport split-scrollport--table"
            style={{ height: VIEWPORT_HEIGHT }}
            onScroll={() => {
              if (syncSource.current !== "timeline") synchronize("table");
            }}
            data-testid="schedule-scrollport"
          >
            <VirtualTreegrid
              rows={rows}
              collapsedIds={collapsedIds}
              toggleCollapsed={toggleCollapsed}
              virtualItems={virtualItems}
              totalSize={totalSize}
              scrollRef={tableRef}
              scrollToIndex={(index) => virtualizer.scrollToIndex(index, { align: "auto" })}
            />
          </div>
        </div>
        <div className="split-pane__timeline">
          <div className="split-pane__heading split-pane__heading--timeline" title="Canvas timeline · rAF synchronized">
            <TimelineScale zoom={zoom} offset={timelineScroll.left} />
          </div>
          <div
            ref={setTimelineViewport}
            className="split-scrollport split-scrollport--timeline"
            style={{ height: VIEWPORT_HEIGHT }}
            onScroll={() => {
              if (syncSource.current !== "table") synchronize("timeline");
            }}
          >
            <div className="canvas-content" style={{ width: timelineWidth, height: totalSize }}>
              <canvas
                ref={canvasRef}
                className="canvas-timeline"
                aria-hidden="true"
                style={{ top: timelineScroll.top, left: timelineScroll.left }}
              />
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
