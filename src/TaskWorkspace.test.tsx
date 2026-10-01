import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { JobClient } from "./api/jobs";
import type { Job, Task, TaskHierarchy, WorkingCalendar } from "./types/jobs";

const calendar: WorkingCalendar = {
  workingWeekdays: ["monday", "tuesday", "wednesday", "thursday", "friday"],
  workdayStartMinute: 480,
  workdayDurationMinutes: 360,
};

function job(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    name: "Oak House",
    status: "draft",
    timezone: "America/New_York",
    createdAt: "2026-09-08T12:00:00.000Z",
    updatedAt: "2026-09-08T12:00:00.000Z",
    version: 1,
    scheduleStart: "2026-09-08",
    calendar,
    ...overrides,
  };
}

function task(id: string, name: string, durationMinutes: number): Task {
  return {
    id,
    jobId: "job-1",
    parentTaskId: null,
    sortKey: id === "layout" ? 0 : 1,
    name,
    createdAt: "2026-09-08T12:00:00.000Z",
    updatedAt: "2026-09-08T12:00:00.000Z",
    version: 1,
    durationMinutes,
  };
}

function hierarchy(tasks: Task[]): TaskHierarchy {
  return { jobId: "job-1", jobVersion: 1, tasks };
}

function schedule() {
  const rows = [
    ["layout", "Layout", 0, 360],
    ["inspection", "Inspection", 1, 1],
  ].map(([taskId, name, logicalIndex, durationMinutes]) => ({
    taskId: taskId as string,
    parentTaskId: null,
    logicalIndex: logicalIndex as number,
    depth: 1,
    positionInSet: (logicalIndex as number) + 1,
    setSize: 2,
    sortKey: logicalIndex as number,
    wbs: String((logicalIndex as number) + 1),
    name: name as string,
    kind: "task" as const,
    hasChildren: false,
    durationMinutes: durationMinutes as number,
    start: "2026-09-08T08:00:00",
    finish: "2026-09-08T14:00:00",
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
    progressStatus: "notStarted" as const,
    predecessors: [],
    baseline: null,
    explanation: {
      kind: "scheduled" as const,
      taskId: taskId as string,
      primaryDriver: { kind: "scheduleStart" as const },
      otherBindingDrivers: [],
      startedActualStart: null,
      calendarGap: null,
      totalFloatMinutes: 0,
      critical: true,
      lateFinishLimit: { kind: "projectFinish" as const },
    },
  }));
  return {
    contractVersion: 7 as const,
    jobId: "job-1",
    jobVersion: 1,
    scheduleStart: "2026-09-08T08:00:00",
    scheduleFinish: "2026-09-08T14:00:00",
    dataDate: null,
    baselineId: null,
    rowCount: rows.length,
    criticalTaskIds: ["layout", "inspection"],
    criticalPath: ["layout", "inspection"],
    calendar: { workingWeekdays: calendar.workingWeekdays, exceptionDates: [] },
    rows,
  };
}

function client(overrides: Partial<JobClient> = {}): JobClient {
  const tasks = [task("layout", "Layout", 360), task("inspection", "Inspection", 1)];
  return {
    listJobs: vi.fn().mockResolvedValue([job()]),
    createJob: vi.fn(),
    listTasks: vi.fn().mockResolvedValue(hierarchy(tasks)),
    createTask: vi.fn(),
    updateTask: vi.fn(),
    reorderTask: vi.fn(),
    updateSchedule: vi.fn().mockResolvedValue(job()),
    getSchedule: vi.fn().mockResolvedValue(schedule()),
    updateTaskDuration: vi.fn().mockResolvedValue(hierarchy(tasks)),
    ...overrides,
  };
}

async function openWorkspace(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Open schedule for Oak House" }));
}

describe("task workspace", () => {
  it("opens a newly created job directly to Add task and actionable schedule setup", async () => {
    const user = userEvent.setup();
    const created = job();
    delete created.scheduleStart;
    delete created.calendar;
    const appClient = client({
      listJobs: vi.fn().mockResolvedValue([]),
      createJob: vi.fn().mockResolvedValue(created),
      listTasks: vi.fn().mockResolvedValue(hierarchy([])),
    });

    render(<App client={appClient} />);
    await user.click(screen.getByRole("button", { name: "Open settings" }));
    await user.type(screen.getByLabelText("Job name"), created.name);
    await user.click(screen.getByRole("button", { name: "Create job" }));

    expect(await screen.findByRole("button", { name: "Add task" })).toBeVisible();
    expect(screen.getByText("Start your schedule")).toBeVisible();
    expect(screen.getByLabelText(`Schedule start for ${created.name}`)).toBeVisible();
    expect(appClient.listTasks).toHaveBeenCalledWith(created.id);
  });

  it("uses the Gantt row to select one editor and keeps advanced controls closed", async () => {
    const user = userEvent.setup();
    render(<App client={client()} />);
    await openWorkspace(user);
    await screen.findByRole("treegrid", { name: "Schedule for Oak House" });

    screen.getByRole("rowheader", { name: /2 Inspection, task/ }).focus();
    expect(await screen.findByRole("heading", { name: "Edit Inspection" })).toBeVisible();
    expect(screen.getAllByLabelText(/Task name for /)).toHaveLength(1);
    expect(screen.getByLabelText("Task name for Inspection")).toBeVisible();
    expect(screen.getByText("Advanced task details").closest("details")).not.toHaveAttribute("open");
  });

  it("edits duration only in working days, preserves existing minute values, and rejects fractional-minute days", async () => {
    const user = userEvent.setup();
    const updateTaskDuration = vi.fn().mockResolvedValue(hierarchy([
      task("layout", "Layout", 360),
      task("inspection", "Inspection", 1),
    ]));
    render(<App client={client({ updateTaskDuration })} />);
    await openWorkspace(user);
    await screen.findByRole("treegrid", { name: "Schedule for Oak House" });

    const duration = screen.getByLabelText("Duration for Layout");
    await user.clear(duration);
    await user.type(duration, "2");
    await user.click(screen.getByRole("button", { name: "Save duration" }));
    expect(updateTaskDuration).toHaveBeenCalledWith(expect.objectContaining({
      taskId: "layout", durationMinutes: 720,
    }));

    screen.getByRole("rowheader", { name: /2 Inspection, task/ }).focus();
    await screen.findByRole("heading", { name: "Edit Inspection" });
    const oneMinute = screen.getByLabelText("Duration for Inspection");
    expect(oneMinute).toHaveValue(1 / 360);
    expect(screen.queryByLabelText("Duration unit for Inspection")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save duration" })).toBeDisabled();

    await user.clear(oneMinute);
    await user.type(oneMinute, "0.0001");
    await user.click(screen.getByRole("button", { name: "Save duration" }));
    expect(await screen.findByText("Enter a valid duration in working days.")).toBeVisible();
    expect(updateTaskDuration).toHaveBeenCalledTimes(1);
  });
});
