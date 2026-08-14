import { useMemo, useRef, useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";

import { createGanttFixture } from "./fixture";
import { flattenVisibleRows } from "./projection";
import { FullTreegrid } from "./schedule";

beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn();
});

describe("Gantt treegrid keyboard contract", () => {
  it("exposes hierarchy and moves one logical row with ArrowDown", async () => {
    const user = userEvent.setup();
    render(<TreegridHarness />);

    const firstCell = screen.getByRole("gridcell", { name: /1 Site work, summary, critical/i });
    firstCell.focus();
    await user.keyboard("{ArrowDown}");

    expect(screen.getByRole("treegrid", { name: "Work breakdown schedule" })).toHaveAttribute(
      "aria-rowcount",
      "1001",
    );
    expect(firstCell.closest("tr")).toHaveAttribute("aria-rowindex", "2");
    expect(firstCell).toHaveAccessibleName(/baseline Jan 6 to Feb 18, start variance -1 days/i);
    await waitFor(() => {
      expect(screen.getByRole("gridcell", { name: /1\.1 Site work package 1, summary/i })).toHaveFocus();
    });
  });

  it("collapses a focused summary without leaving focus on a removed child", async () => {
    const user = userEvent.setup();
    render(<TreegridHarness />);
    const firstCell = screen.getByRole("gridcell", { name: /1 Site work, summary, critical/i });
    firstCell.focus();

    await user.keyboard("{Enter}");

    expect(screen.getByRole("row", { name: /1 Site work/i })).toHaveAttribute("aria-expanded", "false");
    expect(firstCell).toHaveFocus();
    expect(screen.queryByRole("gridcell", { name: /1\.1 Site work package 1, summary/i })).not.toBeInTheDocument();
  });
});

function TreegridHarness() {
  const fixture = useMemo(() => createGanttFixture().slice(0, 12), []);
  const [collapsedIds, setCollapsedIds] = useState<Set<string>>(() => new Set());
  const rows = flattenVisibleRows(fixture, collapsedIds);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  function toggleCollapsed(id: string) {
    setCollapsedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <div ref={scrollRef}>
      <FullTreegrid
        rows={rows}
        collapsedIds={collapsedIds}
        toggleCollapsed={toggleCollapsed}
        scrollRef={scrollRef}
        timelineWidth={600}
        renderTimelineCell={() => (
          <td role="gridcell" tabIndex={-1} aria-hidden="true">
            timeline
          </td>
        )}
      />
    </div>
  );
}
