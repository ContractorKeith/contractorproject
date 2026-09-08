import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { JobClient } from "./api/jobs";
import type { GanttReadModel } from "./types/gantt";
import type { Job, Task, TaskHierarchy } from "./types/jobs";

const job = (version = 1): Job => ({ id: "job", name: "Oak House", status: "draft", timezone: "America/New_York", createdAt: "2026-08-17T00:00:00Z", updatedAt: "2026-08-17T00:00:00Z", version, scheduleStart: "2026-08-17", calendar: { workingWeekdays: ["monday"], workdayStartMinute: 480, workdayDurationMinutes: 360 } });
const task = (id: string, name: string, version = 1): Task => ({ id, jobId: "job", parentTaskId: null, sortKey: 0, name, createdAt: "", updatedAt: "", version, durationMinutes: 1 });
const hierarchy = (version = 1, tasks: Task[] = [task("a", "Layout"), task("b", "Inspection")]): TaskHierarchy => ({ jobId: "job", jobVersion: version, tasks });
const emptySchedule = { contractVersion: 7 as const, jobId: "job", jobVersion: 1, scheduleStart: "2026-08-17T08:00:00", scheduleFinish: "2026-08-17T08:00:00", dataDate: null, baselineId: null, rowCount: 0, criticalTaskIds: [], criticalPath: [], calendar: { workingWeekdays: ["monday"] as Array<"monday">, exceptionDates: [] }, rows: [] };
const scheduled = (): GanttReadModel => ({ ...emptySchedule, rowCount: 2, rows: ["a", "b"].map((id, logicalIndex) => ({ taskId: id, parentTaskId: null, logicalIndex, depth: 0, positionInSet: logicalIndex + 1, setSize: 2, sortKey: logicalIndex, wbs: String(logicalIndex + 1), name: id === "a" ? "Layout" : "Inspection", kind: "task" as const, hasChildren: false, durationMinutes: 60, start: "2026-08-17T08:00:00", finish: "2026-08-17T09:00:00", totalFloatMinutes: 0, startNoEarlierThan: null, finishNoLaterThan: null, constraintViolated: false, critical: false, milestone: false, summary: false, percentComplete: 0, actualStart: null, actualFinish: null, progressStatus: "notStarted" as const, predecessors: [], baseline: null, explanation: { kind: "scheduled" as const, taskId: id, primaryDriver: { kind: "scheduleStart" as const }, otherBindingDrivers: [], startedActualStart: null, calendarGap: null, totalFloatMinutes: 0, critical: false, lateFinishLimit: { kind: "projectFinish" as const } } })) });

function client(overrides: Partial<JobClient> = {}): JobClient {
  return {
    listJobs: vi.fn().mockResolvedValue([job()]),
    createJob: vi.fn(), listTasks: vi.fn().mockResolvedValue(hierarchy()), getSchedule: vi.fn().mockResolvedValue(emptySchedule),
    createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(), updateTaskDuration: vi.fn().mockResolvedValue(hierarchy()),
    ...overrides,
  };
}

async function open(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Open schedule for Oak House" }));
}

describe("workspace state guards", () => {
  it("retries a failed task load in place", async () => {
    const listTasks = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(hierarchy());
    const appClient = client({ listTasks });
    const user = userEvent.setup();
    render(<App client={appClient} />);
    await open(user);
    await user.click(await screen.findByRole("button", { name: "Retry loading tasks" }));
    await screen.findByRole("heading", { name: "Edit Layout" });
    expect(listTasks).toHaveBeenCalledTimes(2);
  });

  it("keeps a dirty editor selected when a cancelled picker switch is retried", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
    render(<App client={client()} />);
    await open(user);
    await screen.findByRole("heading", { name: "Edit Layout" });
    await user.type(screen.getByLabelText("Task name for Layout"), " revised");
    await user.click(screen.getByRole("button", { name: "Inspection" }));
    expect(screen.getByRole("heading", { name: "Edit Layout" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Edit selected task" }));
    expect(await screen.findByRole("heading", { name: "Edit Inspection" })).toBeVisible();
    expect(confirm).toHaveBeenCalledTimes(2);
  });

  it("keeps a Gantt-selected leaf open after its save refreshes the schedule", async () => {
    const refreshed = hierarchy(2, [task("a", "Layout", 2), task("b", "Inspection", 2)]);
    const appClient = client({ getSchedule: vi.fn().mockResolvedValue(scheduled()), updateTaskDuration: vi.fn().mockResolvedValue(refreshed) });
    const user = userEvent.setup();
    render(<App client={appClient} />);
    await open(user);
    await user.click(await screen.findByRole("rowheader", { name: /2 Inspection/ }));
    expect(await screen.findByRole("heading", { name: "Edit Inspection" })).toBeVisible();
    const duration = screen.getByLabelText("Duration for Inspection");
    await user.clear(duration);
    await user.type(duration, "2");
    await user.click(screen.getByRole("button", { name: "Save duration" }));
    expect(await screen.findByRole("heading", { name: "Edit Inspection" })).toBeVisible();
  });

  it("keeps the fallback-selected task when its first saved duration makes the Gantt computable", async () => {
    const initial = hierarchy(1, [task("a", "Layout"), { ...task("b", "Inspection"), durationMinutes: null }]);
    const computed = hierarchy(2, [task("a", "Layout", 2), { ...task("b", "Inspection", 2), durationMinutes: 60 }]);
    const getSchedule = vi.fn()
      .mockRejectedValueOnce({ kind: "validation", code: "summary_without_children", message: "summary task must have at least one child" })
      .mockResolvedValue({ ...scheduled(), jobVersion: 2 });
    const user = userEvent.setup();
    render(<App client={client({ listTasks: vi.fn().mockResolvedValue(initial), getSchedule, updateTaskDuration: vi.fn().mockResolvedValue(computed) })} />);
    await open(user);

    await user.click(await screen.findByRole("button", { name: "Inspection" }));
    expect(await screen.findByRole("heading", { name: "Edit Inspection" })).toBeVisible();
    await user.type(screen.getByLabelText("Duration for Inspection"), "1");
    await user.click(screen.getByRole("button", { name: "Save duration" }));

    await screen.findByRole("treegrid", { name: "Schedule for Oak House" });
    expect(screen.getByRole("heading", { name: "Edit Inspection" })).toBeVisible();
    expect(screen.getByRole("gridcell", { name: /2 Inspection, WBS/ })).toHaveAttribute("tabindex", "0");
  });

  it("keeps a typed draft mounted and disabled when refresh removes its task", async () => {
    const updateTask = vi.fn().mockRejectedValue({ kind: "version_conflict" });
    const listTasks = vi.fn().mockResolvedValueOnce(hierarchy()).mockResolvedValueOnce(hierarchy(1, [task("b", "Inspection")]));
    const appClient = client({ updateTask, listTasks });
    const user = userEvent.setup();
    render(<App client={appClient} />);
    await open(user);
    const name = await screen.findByLabelText("Task name for Layout");
    await user.clear(name);
    await user.type(name, "Layout revised");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.click(await screen.findByRole("button", { name: "Refresh tasks" }));
    expect(await screen.findByText(/was removed elsewhere/)).toBeVisible();
    expect(screen.getByLabelText("Task name for Layout")).toHaveValue("Layout revised");
    expect(screen.getByLabelText("Task name for Layout")).toBeDisabled();
  });

  it("saves started progress with actual dates without opening advanced task details", async () => {
    const updateTaskProgress = vi.fn().mockResolvedValue({ task: task("a", "Layout"), jobVersion: 1 });
    const user = userEvent.setup();
    render(<App client={client({ updateTaskProgress })} />);
    await open(user);
    await user.clear(await screen.findByLabelText("Percent complete for Layout"));
    await user.type(screen.getByLabelText("Percent complete for Layout"), "25");
    fireEvent.change(screen.getByLabelText("Actual start for Layout"), { target: { value: "2026-08-17" } });
    await user.click(screen.getByRole("button", { name: "Save progress" }));
    expect(updateTaskProgress).toHaveBeenCalledWith({
      taskId: "a", clear: false, percentComplete: 25, actualStart: "2026-08-17", actualFinish: null,
      expectedVersion: 1, expectedJobVersion: 1,
    });
    expect(screen.getByText("Advanced task details").closest("details")).not.toHaveProperty("open", true);
  });

  it("keeps exact persisted minutes clean across working-day units", async () => {
    const thirteenMinutes = hierarchy(1, [{ ...task("a", "Layout"), durationMinutes: 13 }]);
    const user = userEvent.setup();
    render(<App client={client({ listTasks: vi.fn().mockResolvedValue(thirteenMinutes) })} />);
    await open(user);
    const unit = await screen.findByLabelText("Duration unit for Layout");
    const save = screen.getByRole("button", { name: "Save duration" });
    expect(save).toBeDisabled();
    await user.selectOptions(unit, "hours");
    expect(save).toBeDisabled();
    await user.selectOptions(unit, "minutes");
    expect(screen.getByLabelText("Duration for Layout")).toHaveValue(13);
    expect(save).toBeDisabled();
  });

  it("keeps 31 minutes clean under an eight-hour workday", async () => {
    const eightHourJob = { ...job(), calendar: { ...job().calendar!, workdayDurationMinutes: 480 } };
    const thirtyOneMinutes = hierarchy(1, [{ ...task("a", "Layout"), durationMinutes: 31 }]);
    const user = userEvent.setup();
    render(<App client={client({ listJobs: vi.fn().mockResolvedValue([eightHourJob]), listTasks: vi.fn().mockResolvedValue(thirtyOneMinutes) })} />);
    await open(user);
    const unit = await screen.findByLabelText("Duration unit for Layout");
    await user.selectOptions(unit, "hours");
    expect(screen.getByRole("button", { name: "Save duration" })).toBeDisabled();
    await user.selectOptions(unit, "minutes");
    expect(screen.getByLabelText("Duration for Layout")).toHaveValue(31);
  });

  it("rejects meaningful fractional minutes rather than rounding them", async () => {
    const user = userEvent.setup();
    render(<App client={client()} />);
    await open(user);
    await user.selectOptions(await screen.findByLabelText("Duration unit for Layout"), "minutes");
    const duration = screen.getByLabelText("Duration for Layout");
    await user.clear(duration);
    await user.type(duration, "0.001");
    await user.click(screen.getByRole("button", { name: "Save duration" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("resolves to a whole minute");
  });

  it("refreshes the active hierarchy before a post-conflict duration save", async () => {
    const currentJob = job(2);
    const currentHierarchy = hierarchy(2, [task("a", "Layout", 2)]);
    const listJobs = vi.fn()
      .mockResolvedValueOnce([job()])
      .mockResolvedValueOnce([job()])
      .mockResolvedValueOnce([currentJob])
      .mockResolvedValueOnce([currentJob]);
    const listTasks = vi.fn().mockResolvedValueOnce(hierarchy()).mockResolvedValueOnce(currentHierarchy);
    const updateTaskDuration = vi.fn().mockResolvedValue(currentHierarchy);
    const appClient = client({ listJobs, listTasks, updateTaskDuration, archiveJob: vi.fn().mockRejectedValue({ kind: "version_conflict" }) });
    const user = userEvent.setup();
    render(<App client={appClient} />);
    await open(user);
    await screen.findByRole("heading", { name: "Edit Layout" });
    await user.click(screen.getByRole("button", { name: "Archive job" }));
    const taskName = screen.getByLabelText("Task name for Layout");
    await user.clear(taskName);
    await user.type(taskName, "Layout draft");
    await user.click(await screen.findByRole("button", { name: "Refresh jobs" }));
    await waitFor(() => expect(listTasks).toHaveBeenCalledTimes(2));
    expect(screen.getByLabelText("Task name for Layout")).toHaveValue("Layout draft");
    const duration = screen.getByLabelText("Duration for Layout");
    await user.clear(duration);
    await user.type(duration, "2");
    await user.click(screen.getByRole("button", { name: "Save duration" }));
    await waitFor(() => expect(updateTaskDuration).toHaveBeenCalledWith(expect.objectContaining({ expectedVersion: 2, expectedJobVersion: 2, durationMinutes: 720 })));
  });
});
