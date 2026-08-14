import { useMemo, useRef } from "react";

import type { SpikeTaskRow } from "../fixture";
import {
  FullTreegrid,
  ROW_HEIGHT,
  VIEWPORT_HEIGHT,
  WBS_WIDTH,
  zoomWidths,
  type Zoom,
} from "../schedule";

interface RowEmbeddedDomProps {
  rows: readonly SpikeTaskRow[];
  collapsedIds: ReadonlySet<string>;
  toggleCollapsed: (id: string) => void;
  zoom: Zoom;
  viewportRef: React.RefObject<HTMLDivElement | null>;
  horizontalViewportRef: React.RefObject<HTMLDivElement | null>;
}

export function RowEmbeddedDom({
  rows,
  collapsedIds,
  toggleCollapsed,
  zoom,
  viewportRef,
  horizontalViewportRef,
}: RowEmbeddedDomProps) {
  const localRef = useRef<HTMLDivElement | null>(null);
  const setViewport = (element: HTMLDivElement | null) => {
    localRef.current = element;
    viewportRef.current = element;
    horizontalViewportRef.current = element;
  };
  const dayWidth = zoomWidths[zoom];
  const timelineWidth = Math.ceil(230 * dayWidth);
  const rowIndexById = useMemo(
    () => new Map(rows.map((row, index) => [row.id, index])),
    [rows],
  );

  return (
    <section className="schedule-variant row-dom-variant" aria-labelledby="variant-title">
      <div
        className="row-dom-scrollport"
        ref={setViewport}
        style={{ height: VIEWPORT_HEIGHT }}
        data-testid="schedule-scrollport"
      >
        <FullTreegrid
          rows={rows}
          collapsedIds={collapsedIds}
          toggleCollapsed={toggleCollapsed}
          scrollRef={localRef}
          timelineWidth={timelineWidth}
          renderTimelineCell={(row) => (
            <td
              className="inline-timeline-cell"
              role="gridcell"
              tabIndex={-1}
              aria-hidden="true"
              style={{ width: timelineWidth, minWidth: timelineWidth }}
            >
              <div className="inline-timeline-track" style={{ width: timelineWidth, height: ROW_HEIGHT }}>
                <span className="inline-row-marker" data-timeline-row-id={row.id} />
                {row.kind !== "summary" ? (
                  <span
                    className="inline-baseline"
                    style={{
                      left: row.baselineStartDay * dayWidth,
                      width: Math.max(row.baselineDurationDays * dayWidth, 6),
                    }}
                  />
                ) : null}
                <span
                  className={`inline-task inline-task--${row.kind}${row.critical ? " inline-task--critical" : ""}`}
                  style={{
                    left: row.startDay * dayWidth,
                    width: Math.max(row.durationDays * dayWidth, 8),
                  }}
                />
              </div>
            </td>
          )}
        />
        <svg
          className="row-dom-dependencies"
          aria-hidden="true"
          width={timelineWidth}
          height={rows.length * ROW_HEIGHT}
          style={{ left: WBS_WIDTH, top: 34 }}
        >
          {rows.flatMap((row, rowIndex) =>
            row.predecessorIds.map((predecessorId) => {
              const predecessorIndex = rowIndexById.get(predecessorId);
              const predecessor = predecessorIndex === undefined ? undefined : rows[predecessorIndex];
              if (!predecessor || predecessorIndex === undefined) return null;
              const predecessorX = (predecessor.startDay + predecessor.durationDays) * dayWidth;
              const successorX = row.startDay * dayWidth;
              const predecessorY = predecessorIndex * ROW_HEIGHT + ROW_HEIGHT / 2;
              const successorY = rowIndex * ROW_HEIGHT + ROW_HEIGHT / 2;
              return (
                <path
                  key={`${row.id}-${predecessorId}`}
                  className={row.critical ? "dependency dependency--critical" : "dependency"}
                  d={`M ${predecessorX} ${predecessorY} H ${successorX - 4} V ${successorY} H ${successorX}`}
                />
              );
            }),
          )}
        </svg>
      </div>
    </section>
  );
}
