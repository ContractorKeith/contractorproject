import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import type { GanttReadModel, GanttRow } from "../types/gantt";
import { GanttTreegrid } from "./GanttTreegrid";

function row(overrides: Partial<GanttRow> & Pick<GanttRow, "taskId" | "logicalIndex" | "wbs" | "name">): GanttRow {
  return {
    parentTaskId: null,
    depth: 0,
    positionInSet: 1,
    setSize: 1,
    sortKey: overrides.logicalIndex,
    kind: "task",
    hasChildren: false,
    durationMinutes: 480,
    start: "2026-08-17T08:00:00",
    finish: "2026-08-17T17:00:00",
    totalFloatMinutes: 0,
    startNoEarlierThan: null,
    finishNoLaterThan: null,
    constraintViolated: false,
    critical: true,
    milestone: false,
    summary: false,
    percentComplete: 0,
    actualStart: null,
    actualFinish: null,
    progressStatus: "notStarted",
    predecessorIds: [],
    baseline: null,
    ...overrides,
  };
}

function model(rows: GanttRow[]): GanttReadModel {
  return {
    contractVersion: 4,
    jobId: "job-1",
    jobVersion: 4,
    scheduleStart: "2026-08-17T08:00:00",
    scheduleFinish: "2026-08-19T10:00:00",
    dataDate: null,
    baselineId: "baseline-1",
    rowCount: rows.length,
    criticalTaskIds: rows.filter((item) => item.critical).map((item) => item.taskId),
    criticalPath: ["task-a", "task-c"],
    rows,
  };
}

const fixtureRows = [
  row({
    taskId: "summary",
    logicalIndex: 0,
    wbs: "1",
    name: "Site work",
    kind: "summary",
    hasChildren: true,
    summary: true,
    durationMinutes: 960,
  }),
  row({
    taskId: "task-a",
    parentTaskId: "summary",
    logicalIndex: 1,
    depth: 1,
    positionInSet: 1,
    setSize: 2,
    wbs: "1.1",
    name: "Layout",
    predecessorIds: ["survey-control"],
    startNoEarlierThan: "2026-08-18",
    finishNoLaterThan: "2026-08-20",
    constraintViolated: true,
    percentComplete: 42,
    actualStart: "2026-08-17T08:00:00",
    progressStatus: "inProgress",
    baseline: {
      start: "2026-08-17T07:00:00",
      finish: "2026-08-17T16:00:00",
      durationMinutes: 420,
      startVarianceMinutes: 60,
      finishVarianceMinutes: 60,
      durationVarianceMinutes: 60,
    },
  }),
  row({
    taskId: "task-b",
    parentTaskId: "summary",
    logicalIndex: 2,
    depth: 1,
    positionInSet: 2,
    setSize: 2,
    wbs: "1.2",
    name: "Inspection",
    kind: "milestone",
    durationMinutes: 0,
    milestone: true,
    critical: false,
    totalFloatMinutes: 240,
    start: "2026-08-17T17:00:00",
    finish: "2026-08-17T17:00:00",
    baseline: {
      start: "2026-08-16T17:00:00",
      finish: "2026-08-16T17:00:00",
      durationMinutes: 0,
      startVarianceMinutes: 1_440,
      finishVarianceMinutes: 1_440,
      durationVarianceMinutes: 0,
    },
  }),
  row({
    taskId: "task-c",
    logicalIndex: 3,
    positionInSet: 2,
    setSize: 2,
    wbs: "2",
    name: "Closeout",
    critical: false,
    totalFloatMinutes: -60,
    start: "2026-08-17T17:00:00",
    finish: "2026-08-18T10:00:00",
    durationMinutes: 120,
    predecessorIds: ["task-a", "task-b"],
  }),
];

describe("GanttTreegrid", () => {
  it("renders stable treegrid semantics and every authoritative schedule fact as text", () => {
    render(<GanttTreegrid readModel={model(fixtureRows)} />);

    const grid = screen.getByRole("treegrid", {
      name: "Work breakdown schedule",
    });
    expect(grid).toHaveAttribute("aria-rowcount", "5");
    expect(grid).toHaveAttribute("aria-colcount", "8");

    const layoutRow = screen.getByRole("row", { name: /1\.1 Layout/ });
    expect(layoutRow).toHaveAttribute("aria-rowindex", "3");
    expect(layoutRow).toHaveAttribute("aria-level", "2");
    expect(layoutRow).toHaveAttribute("aria-posinset", "1");
    expect(layoutRow).toHaveAttribute("aria-setsize", "2");
    expect(layoutRow).not.toHaveAttribute("aria-expanded");
    expect(layoutRow).toHaveTextContent("2026-08-17 08:00");
    // The visible baseline fact shows the civil date only; the clock time stays
    // in the accessible name.
    expect(layoutRow).toHaveTextContent("Baseline 2026-08-17 +60 min");
    expect(
      screen.getByRole("gridcell", {
        name: /1\.1 Layout, start, 2026-08-17 08:00, baseline start 2026-08-17 07:00, variance \+60 min/,
      }),
    ).toBeInTheDocument();
    // Duration cell surfaces the Rust-derived baseline duration variance fact
    // (unit dropped from the first number to shorten the narrow-cell fragment).
    expect(screen.getByTestId("duration-baseline-task-a")).toHaveTextContent(
      "Baseline 420 +60 min",
    );
    expect(
      screen.getByRole("gridcell", {
        name: /1\.1 Layout, duration, 480 min, baseline duration 420 min, variance \+60 min/,
      }),
    ).toBeInTheDocument();
    // The milestone row suppresses the duration baseline fact and its label.
    expect(screen.queryByTestId("duration-baseline-task-b")).not.toBeInTheDocument();
    expect(
      screen.getByRole("gridcell", { name: /1\.2 Inspection, duration, milestone, no baseline/ }),
    ).toBeInTheDocument();
    expect(layoutRow).toHaveTextContent("Critical");
    expect(layoutRow).toHaveTextContent("survey-control");
    expect(layoutRow).toHaveTextContent("≥ 2026-08-18");
    expect(layoutRow).toHaveTextContent("≤ 2026-08-20");
    expect(screen.getByRole("rowheader", { name: /start no earlier than 2026-08-18, finish no later than 2026-08-20, constraint violated/ })).toBeInTheDocument();
    expect(screen.getByTestId("constraint-float-task-a")).toHaveTextContent("Constraintviolated");

    // The % Done column renders visible percent text and an accessible progress fact.
    expect(screen.getByTestId("progress-task-a")).toHaveTextContent("42%");
    expect(
      screen.getByRole("gridcell", { name: /1\.1 Layout, percent complete, 42 percent complete, in progress, actual start 2026-08-17 08:00/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("gridcell", { name: /1 Site work, percent complete, 0 percent complete, not started/ }),
    ).toBeInTheDocument();

    const milestoneRow = screen.getByRole("row", { name: /1\.2 Inspection/ });
    expect(milestoneRow).toHaveTextContent("Milestone");
    expect(milestoneRow).toHaveTextContent("+240 min");

    const negativeFloat = screen.getByRole("gridcell", {
      name: /2 Closeout, total float, -60 min/,
    });
    expect(negativeFloat).toHaveTextContent("-60 min");
    expect(negativeFloat.querySelector("svg")).toBeInTheDocument();

    const summaryRow = screen.getByRole("row", { name: /1 Site work/ });
    expect(summaryRow).toHaveAttribute("aria-expanded", "true");
    expect(within(summaryRow).getByRole("button", { name: "Collapse Site work" })).toHaveAttribute("tabindex", "-1");
    expect(grid.querySelectorAll('[role="rowheader"][tabindex="0"], [role="gridcell"][tabindex="0"]')).toHaveLength(1);

    const timeline = screen.getByTestId("gantt-timeline");
    expect(timeline).toHaveAttribute("aria-hidden", "true");
    expect(timeline.querySelector('[data-timeline-task-id="summary"]')).toHaveClass("gantt-timeline__summary");
    expect(timeline.querySelector('[data-timeline-task-id="task-a"]')).toHaveClass("gantt-timeline__task--critical");
    expect(timeline.querySelector('[data-baseline-task-id="task-a"]')).toBeInTheDocument();
    const liveBar = timeline.querySelector<SVGRectElement>('[data-timeline-task-id="task-a"]')!;
    const baselineBar = timeline.querySelector<SVGRectElement>('[data-baseline-task-id="task-a"]')!;
    expect(Number(liveBar.getAttribute("y")) + Number(liveBar.getAttribute("height"))).toBeLessThan(
      Number(baselineBar.getAttribute("y")),
    );
    expect(timeline.querySelector('[data-timeline-task-id="task-b"]')).toHaveClass("gantt-timeline__milestone");
    const milestone = timeline.querySelector<SVGRectElement>('[data-timeline-task-id="task-b"]')!;
    const milestoneBaseline = timeline.querySelector<SVGRectElement>('[data-baseline-task-id="task-b"]')!;
    const milestoneCenter = Number(milestone.getAttribute("y")) + Number(milestone.getAttribute("height")) / 2;
    const rotatedMilestoneBottom = milestoneCenter + Math.sqrt(50);
    expect(rotatedMilestoneBottom).toBeLessThan(Number(milestoneBaseline.getAttribute("y")));
    const tightDependency = timeline.querySelector<SVGPathElement>('[data-dependency="task-a->task-c"]')!;
    expect(tightDependency).toBeInTheDocument();
    expect(Number(tightDependency.dataset.approachX)).toBeLessThan(Number(tightDependency.dataset.finishX));
    expect(tightDependency.getAttribute("d")).toMatch(/Q .* H /);
    expect(timeline.querySelector('[data-dependency="task-b->task-c"]')).toBeInTheDocument();
  });

  it("draws proportional progress fill and an accessible data-date marker", () => {
    const statused = model([
      row({
        taskId: "task-a",
        logicalIndex: 0,
        wbs: "1",
        name: "Layout",
        percentComplete: 50,
        actualStart: "2026-08-17T08:00:00",
        progressStatus: "inProgress",
        start: "2026-08-17T08:00:00",
        finish: "2026-08-18T16:00:00",
      }),
    ]);
    statused.dataDate = "2026-08-18T08:00:00";
    render(<GanttTreegrid readModel={statused} />);

    const timeline = screen.getByTestId("gantt-timeline");
    const bar = timeline.querySelector<SVGRectElement>('[data-timeline-task-id="task-a"]')!;
    const fill = timeline.querySelector<SVGRectElement>('[data-progress-task-id="task-a"]')!;
    expect(fill).toBeInTheDocument();
    // The fill is exactly half of the bar width and shares its left edge.
    expect(fill.getAttribute("x")).toBe(bar.getAttribute("x"));
    expect(Number(fill.getAttribute("width"))).toBeCloseTo(Number(bar.getAttribute("width")) / 2, 5);

    const marker = screen.getByTestId("gantt-data-date-marker");
    expect(marker).toHaveAccessibleName("Data date 2026-08-18");
    expect(marker).toHaveTextContent("Data date 2026-08-18");
  });

  it("renders no progress fill or marker for an unstatused projection", () => {
    render(<GanttTreegrid readModel={model(fixtureRows)} />);
    expect(screen.queryByTestId("gantt-data-date-marker")).not.toBeInTheDocument();
    const timeline = screen.getByTestId("gantt-timeline");
    expect(timeline.querySelector('[data-progress-task-id="task-c"]')).toBeNull();
  });

  it("moves one roving cell focus, collapses hierarchy, and reaches offscreen logical rows", async () => {
    const user = userEvent.setup();
    render(<GanttTreegrid readModel={model(fixtureRows)} viewportHeight={96} />);

    const summaryTask = screen.getByRole("rowheader", {
      name: /1 Site work, task/,
    });
    summaryTask.focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("gridcell", { name: /1 Site work, duration/ })).toHaveFocus();

    await user.keyboard("{ArrowDown}{Home}");
    expect(screen.getByRole("gridcell", { name: /1\.1 Layout, WBS/ })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("rowheader", { name: /1\.1 Layout, task/ })).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(summaryTask).toHaveFocus();

    await user.keyboard(" ");
    expect(screen.queryByRole("row", { name: /1\.1 Layout/ })).not.toBeInTheDocument();
    expect(summaryTask).toHaveFocus();
    expect(screen.getByRole("row", { name: /1 Site work/ })).toHaveAttribute("aria-expanded", "false");

    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("row", { name: /1\.1 Layout/ })).toBeInTheDocument();
    expect(screen.getByRole("row", { name: /1 Site work/ })).toHaveAttribute("aria-expanded", "true");

    await user.keyboard("{Enter}");
    expect(screen.queryByRole("row", { name: /1\.1 Layout/ })).not.toBeInTheDocument();
    expect(screen.getByRole("row", { name: /1 Site work/ })).toHaveAttribute("aria-expanded", "false");

    await user.keyboard("{Control>}{End}{/Control}");
    expect(screen.getByRole("gridcell", { name: /2 Closeout, total float/ })).toHaveFocus();
  });

  it("keeps focus on the same task when a new projection reorders it", () => {
    const initial = model(fixtureRows);
    const { rerender } = render(<GanttTreegrid readModel={initial} />);
    const focused = screen.getByRole("rowheader", {
      name: /1\.2 Inspection, task/,
    });
    focused.focus();

    const reordered = [fixtureRows[0]!, fixtureRows[2]!, fixtureRows[1]!, fixtureRows[3]!].map(
      (item, logicalIndex) => ({ ...item, logicalIndex }),
    );
    rerender(<GanttTreegrid readModel={model(reordered)} />);

    expect(screen.getByRole("rowheader", { name: /1\.2 Inspection, task/ })).toHaveFocus();
  });

  it("does not steal external focus when a new projection removes the active task", () => {
    const initial = model(fixtureRows);
    const { rerender } = render(
      <>
        <button type="button">Outside action</button>
        <GanttTreegrid readModel={initial} />
      </>,
    );
    screen.getByRole("rowheader", { name: /1\.2 Inspection, task/ }).focus();
    const outside = screen.getByRole("button", { name: "Outside action" });
    outside.focus();

    rerender(
      <>
        <button type="button">Outside action</button>
        <GanttTreegrid readModel={model(fixtureRows.filter((item) => item.taskId !== "task-b"))} />
      </>,
    );

    expect(outside).toHaveFocus();
  });
});
