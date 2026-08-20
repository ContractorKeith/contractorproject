import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { GanttRow, GanttTaskExplanation } from "../types/gantt";
import { ExplanationPanel } from "./ExplanationPanel";

function row(name: string, taskId: string, explanation: GanttTaskExplanation): GanttRow {
  return {
    taskId,
    parentTaskId: null,
    logicalIndex: 0,
    depth: 0,
    positionInSet: 1,
    setSize: 1,
    sortKey: 0,
    wbs: "1",
    name,
    kind: "task",
    hasChildren: false,
    durationMinutes: 480,
    start: "2026-01-05T08:00:00",
    finish: "2026-01-05T16:00:00",
    totalFloatMinutes: 0,
    startNoEarlierThan: null,
    finishNoLaterThan: null,
    constraintViolated: false,
    critical: false,
    milestone: false,
    summary: false,
    percentComplete: 0,
    actualStart: null,
    actualFinish: null,
    progressStatus: "notStarted",
    predecessors: [],
    baseline: null,
    explanation,
  };
}

describe("ExplanationPanel", () => {
  it("shows the empty state and a generic region name with no focused task", () => {
    render(<ExplanationPanel row={null} />);
    const region = screen.getByRole("region", { name: "Schedule explanation" });
    expect(region).toHaveTextContent("Focus a task to see what drives it.");
  });

  it("names the region for the focused task and titles it with name and id", () => {
    render(
      <ExplanationPanel row={row("Layout", "task-a", { kind: "summary", taskId: "task-a" })} />,
    );
    expect(
      screen.getByRole("region", { name: "Schedule explanation for Layout" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading")).toHaveTextContent("Layout · task-a");
  });

  it("renders a summary explanation as a derived-from-children fact", () => {
    render(
      <ExplanationPanel row={row("Site work", "s1", { kind: "summary", taskId: "s1" })} />,
    );
    expect(screen.getByText("Derived from children")).toBeInTheDocument();
  });

  it("renders a complete explanation with its normalized actuals", () => {
    render(
      <ExplanationPanel
        row={row("Excavate", "a", {
          kind: "complete",
          taskId: "a",
          actualStart: "2026-01-05T08:00:00",
          actualFinish: "2026-01-05T16:00:00",
        })}
      />,
    );
    expect(
      screen.getByText("Complete · actual 2026-01-05 08:00 – 2026-01-05 16:00"),
    ).toBeInTheDocument();
  });

  it("renders a scheduled explanation with driver, also, float, and limit facts", () => {
    render(
      <ExplanationPanel
        row={row("Backfill", "b", {
          kind: "scheduled",
          taskId: "b",
          primaryDriver: { kind: "predecessor", taskId: "A", dependencyType: "FS", lagMinutes: 480 },
          otherBindingDrivers: [
            { kind: "startConstraint", date: "2026-01-07", normalizedDate: "2026-01-07" },
          ],
          startedActualStart: "2026-01-05T08:00:00",
          calendarGap: null,
          totalFloatMinutes: 480,
          critical: false,
          lateFinishLimit: { kind: "successor", taskId: "C", dependencyType: "FS", lagMinutes: 480 },
        })}
      />,
    );
    expect(screen.getByText("Driver After A FS +480 min")).toBeInTheDocument();
    expect(screen.getByText("Also Start no earlier than 2026-01-07")).toBeInTheDocument();
    expect(screen.getByText("Started 2026-01-05 08:00")).toBeInTheDocument();
    expect(screen.getByText("Float +480 min")).toBeInTheDocument();
    expect(screen.getByText("Finish limited by successor C FS +480 min")).toBeInTheDocument();
  });

  it("appends the critical marker to negative float and names the deadline limit", () => {
    render(
      <ExplanationPanel
        row={row("Pour", "p", {
          kind: "scheduled",
          taskId: "p",
          primaryDriver: { kind: "scheduleStart" },
          otherBindingDrivers: [],
          startedActualStart: null,
          calendarGap: null,
          totalFloatMinutes: -480,
          critical: true,
          lateFinishLimit: { kind: "deadline", date: "2026-01-05", normalizedDate: "2026-01-05" },
        })}
      />,
    );
    expect(screen.getByText("Driver Starts at schedule start")).toBeInTheDocument();
    expect(screen.getByText("Float -480 min · critical")).toBeInTheDocument();
    expect(screen.getByText("Finish limited by deadline 2026-01-05")).toBeInTheDocument();
  });

  it("reads finish-anchored predecessor drivers as `Finish after`", () => {
    render(
      <ExplanationPanel
        row={row("Backfill", "b", {
          kind: "scheduled",
          taskId: "b",
          primaryDriver: { kind: "predecessor", taskId: "A", dependencyType: "FF", lagMinutes: 0 },
          otherBindingDrivers: [
            { kind: "predecessor", taskId: "C", dependencyType: "SF", lagMinutes: 120 },
          ],
          startedActualStart: null,
          calendarGap: null,
          totalFloatMinutes: 0,
          critical: true,
          lateFinishLimit: { kind: "projectFinish" },
        })}
      />,
    );
    expect(screen.getByText("Driver Finish after A FF")).toBeInTheDocument();
    expect(screen.getByText("Also Finish after C SF +120 min")).toBeInTheDocument();
  });

  it("shows the normalized applied date on a moved deadline limit", () => {
    render(
      <ExplanationPanel
        row={row("Pour", "d", {
          kind: "scheduled",
          taskId: "d",
          primaryDriver: { kind: "scheduleStart" },
          otherBindingDrivers: [],
          startedActualStart: null,
          calendarGap: null,
          totalFloatMinutes: -480,
          critical: true,
          lateFinishLimit: { kind: "deadline", date: "2026-01-10", normalizedDate: "2026-01-09" },
        })}
      />,
    );
    expect(
      screen.getByText("Finish limited by deadline 2026-01-10 · applied 2026-01-09"),
    ).toBeInTheDocument();
  });

  it("shows the normalized applied date only when a start constraint moved", () => {
    render(
      <ExplanationPanel
        row={row("Layout", "l", {
          kind: "scheduled",
          taskId: "l",
          primaryDriver: { kind: "startConstraint", date: "2026-01-07", normalizedDate: "2026-01-08" },
          otherBindingDrivers: [],
          startedActualStart: null,
          calendarGap: null,
          totalFloatMinutes: 0,
          critical: true,
          lateFinishLimit: { kind: "projectFinish" },
        })}
      />,
    );
    expect(
      screen.getByText("Driver Start no earlier than 2026-01-07 · applied 2026-01-08"),
    ).toBeInTheDocument();
    expect(screen.getByText("Finish limited by project finish")).toBeInTheDocument();
  });

  it("renders a singular non-working day gap", () => {
    render(
      <ExplanationPanel
        row={row("Layout", "g1", {
          kind: "scheduled",
          taskId: "g1",
          primaryDriver: { kind: "startConstraint", date: "2026-01-07", normalizedDate: "2026-01-08" },
          otherBindingDrivers: [],
          startedActualStart: null,
          calendarGap: { fromDate: "2026-01-07", toDate: "2026-01-08", nonWorkingDayCount: 1 },
          totalFloatMinutes: 0,
          critical: true,
          lateFinishLimit: { kind: "projectFinish" },
        })}
      />,
    );
    expect(
      screen.getByText("1 non-working day between 2026-01-07 and 2026-01-08"),
    ).toBeInTheDocument();
  });

  it("renders a plural non-working day gap and the data-date driver", () => {
    render(
      <ExplanationPanel
        row={row("Layout", "g2", {
          kind: "scheduled",
          taskId: "g2",
          primaryDriver: { kind: "dataDate", date: "2026-01-12T08:00:00" },
          otherBindingDrivers: [],
          startedActualStart: null,
          calendarGap: { fromDate: "2026-01-09", toDate: "2026-01-12", nonWorkingDayCount: 2 },
          totalFloatMinutes: 0,
          critical: true,
          lateFinishLimit: { kind: "projectFinish" },
        })}
      />,
    );
    expect(screen.getByText("Driver Pushed to data date 2026-01-12 08:00")).toBeInTheDocument();
    expect(
      screen.getByText("2 non-working days between 2026-01-09 and 2026-01-12"),
    ).toBeInTheDocument();
  });
});
