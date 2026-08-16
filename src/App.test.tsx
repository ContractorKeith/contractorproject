import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { JobClient } from "./api/jobs";
import type { WorkingCalendar } from "./types/jobs";

describe("job workspace", () => {
  beforeEach(() => {
    window.localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it("creates a first job and shows it in the workspace", async () => {
    const user = userEvent.setup();
    const createdJob = {
      id: "019ccab7-bb9f-7000-8000-000000000001",
      name: "Ridgeline Fence — Phase 2",
      status: "draft" as const,
      timezone: "America/New_York",
      createdAt: "2026-08-14T15:00:00.000Z",
      updatedAt: "2026-08-14T15:00:00.000Z",
      version: 1,
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([]),
      createJob: vi.fn().mockResolvedValue(createdJob),
      listTasks: vi.fn(),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
    };

    render(<App client={client} />);

    expect(
      await screen.findByRole("heading", { name: "No jobs yet" }),
    ).toBeVisible();
    await user.type(screen.getByLabelText("Job name"), createdJob.name);
    await user.click(screen.getByRole("button", { name: "Create job" }));

    expect(
      await screen.findByRole("heading", { name: createdJob.name }),
    ).toBeVisible();
    expect(client.createJob).toHaveBeenCalledWith({
      name: createdJob.name,
      timezone: expect.any(String),
    });
  });

  it("lets the user override the system theme", async () => {
    const user = userEvent.setup();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([]),
      createJob: vi.fn(),
      listTasks: vi.fn(),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.selectOptions(screen.getByLabelText("Theme"), "dark");

    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(window.localStorage.getItem("contractorproject.theme")).toBe("dark");
  });

  it("loads one job hierarchy on demand and renders nested tasks in application order", async () => {
    const user = userEvent.setup();
    const job = {
      id: "019ccab7-bb9f-7000-8000-000000000001",
      name: "Ridgeline Fence — Phase 2",
      status: "draft" as const,
      timezone: "America/New_York",
      createdAt: "2026-08-14T15:00:00.000Z",
      updatedAt: "2026-08-14T15:00:00.000Z",
      version: 3,
    };
    const listTasks = vi.fn().mockResolvedValue({
      jobId: job.id,
      jobVersion: 3,
      tasks: [
        {
          id: "task-parent",
          jobId: job.id,
          parentTaskId: null,
          sortKey: 0,
          name: "Site work",
          createdAt: job.createdAt,
          updatedAt: job.updatedAt,
          version: 1,
        },
        {
          id: "task-child",
          jobId: job.id,
          parentTaskId: "task-parent",
          sortKey: 0,
          name: "Layout",
          createdAt: job.createdAt,
          updatedAt: job.updatedAt,
          version: 1,
        },
      ],
    });
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks,
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
    };

    render(<App client={client} />);

    const openTasks = await screen.findByRole("button", {
      name: `View tasks for ${job.name}`,
    });
    expect(listTasks).not.toHaveBeenCalled();
    await user.click(openTasks);

    const taskList = await screen.findByRole("list", {
      name: `Tasks for ${job.name}`,
    });
    expect(screen.getByDisplayValue("Site work")).toBeVisible();
    const nestedTask = screen.getByDisplayValue("Layout");
    expect(nestedTask).toBeVisible();
    expect(nestedTask.closest("ol")).not.toBe(taskList);
    expect(listTasks).toHaveBeenCalledExactlyOnceWith(job.id);
    expect(openTasks).toHaveAttribute("aria-expanded", "true");
  });

  it("loads the persisted schedule projection for the selected job", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [] }),
      getSchedule: vi.fn().mockResolvedValue(ganttReadModel(job.id)),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    expect(await screen.findByRole("treegrid", { name: `Schedule for ${job.name}` })).toBeVisible();
    expect(client.getSchedule).toHaveBeenCalledWith(job.id);
  });

  it("shows schedule loading and empty states", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const getSchedule = vi.fn().mockImplementation(() => new Promise(() => undefined));
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [] }),
      getSchedule,
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    const { unmount } = render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    expect(await screen.findByText("Loading schedule…")).toBeVisible();
    unmount();

    getSchedule.mockResolvedValue({
      ...ganttReadModel(job.id),
      rowCount: 0,
      criticalTaskIds: [],
      criticalPath: [],
      rows: [],
    });
    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    expect(await screen.findByText("No scheduled tasks yet.")).toBeVisible();
  });

  it("surfaces schedule validation errors in the selected job", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [] }),
      getSchedule: vi.fn().mockRejectedValue(new Error("set a schedule start before viewing the schedule")),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      `Couldn't build schedule for ${job.name}. set a schedule start before viewing the schedule`,
    );
  });

  it("creates nested tasks, edits a task, and reorders it with keyboard-accessible controls", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 3 };
    const root = fixtureTask(job, "root", null, "Site work", 0, 1);
    const child = fixtureTask(job, "child", "root", "Layout", 0, 1);
    const hierarchy = { jobId: job.id, jobVersion: 3, tasks: [root, child] };
    const afterChild = {
      jobId: job.id,
      jobVersion: 4,
      tasks: [
        ...hierarchy.tasks,
        fixtureTask(job, "child-two", "root", "Excavation", 1, 1),
      ],
    };
    const afterEdit = {
      ...afterChild,
      jobVersion: 5,
      tasks: afterChild.tasks.map((task) =>
        task.id === "child"
          ? { ...task, name: "Layout and stakes", version: 2 }
          : task,
      ),
    };
    const afterReorder = {
      ...afterEdit,
      jobVersion: 6,
      tasks: [
        afterEdit.tasks[0],
        afterEdit.tasks[2],
        { ...afterEdit.tasks[1], sortKey: 1, version: 3 },
      ],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue(hierarchy),
      createTask: vi.fn().mockResolvedValue(afterChild),
      updateTask: vi.fn().mockResolvedValue(afterEdit),
      reorderTask: vi.fn().mockResolvedValue(afterReorder),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `View tasks for ${job.name}` }),
    );
    await user.type(
      screen.getByLabelText("New child task for Site work"),
      "Excavation",
    );
    await user.click(
      screen.getAllByRole("button", { name: "Add subtask" })[0]!,
    );
    expect(client.createTask).toHaveBeenCalledWith({
      jobId: job.id,
      parentTaskId: root.id,
      name: "Excavation",
      expectedJobVersion: 3,
    });

    const edit = screen.getByLabelText("Task name for Layout");
    await user.clear(edit);
    await user.type(edit, "Layout and stakes");
    await user.click(screen.getAllByRole("button", { name: "Save" })[1]!);
    expect(client.updateTask).toHaveBeenCalledWith({
      taskId: child.id,
      name: "Layout and stakes",
      expectedVersion: 1,
    });

    await user.click(screen.getAllByRole("button", { name: "Move down" })[1]!);
    expect(client.reorderTask).toHaveBeenCalledWith({
      taskId: child.id,
      newParentTaskId: root.id,
      newSiblingIndex: 1,
      expectedVersion: 2,
      expectedJobVersion: 5,
    });
  });

  it("retains attempted input after a version conflict and refreshes only when asked", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 2 };
    const task = fixtureTask(job, "task", null, "Site work", 0, 1);
    const untouched = fixtureTask(job, "untouched", null, "Closeout", 1, 1);
    const hierarchy = {
      jobId: job.id,
      jobVersion: 2,
      tasks: [task, untouched],
    };
    const refreshed = {
      ...hierarchy,
      jobVersion: 3,
      tasks: [
        { ...task, name: "Changed elsewhere", version: 2 },
        { ...untouched, name: "Remote closeout", version: 2 },
      ],
    };
    const refreshedJob = {
      ...job,
      version: refreshed.jobVersion,
      scheduleStart: "2026-08-24",
      calendar: defaultCalendar(),
    };
    const client: JobClient = {
      listJobs: vi
        .fn()
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([refreshedJob]),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce(hierarchy)
        .mockResolvedValueOnce(refreshed)
        .mockResolvedValueOnce(refreshed),
      createTask: vi.fn(),
      updateTask: vi
        .fn()
        .mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `View tasks for ${job.name}` }),
    );
    const edit = screen.getByLabelText("Task name for Site work");
    await user.clear(edit);
    await user.type(edit, "My attempted name");
    await user.click(edit.closest("form")!.querySelector("button")!);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Tasks changed elsewhere",
    );
    expect(edit).toHaveValue("My attempted name");
    expect(client.listTasks).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Refresh tasks" }));
    expect(client.listTasks).toHaveBeenCalledTimes(3);
    expect(edit).toHaveValue("My attempted name");
    expect(await screen.findByDisplayValue("Remote closeout")).toBeVisible();
    expect(screen.getByLabelText(`Schedule start for ${job.name}`)).toHaveValue(
      "2026-08-24",
    );
  });

  it("reparents a task across valid parents using the authoritative hierarchy version", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 4 };
    const source = fixtureTask(job, "source", null, "Source phase", 0, 1);
    const target = fixtureTask(job, "target", null, "Target phase", 1, 1);
    const child = fixtureTask(job, "child", source.id, "Layout", 0, 2);
    const hierarchy = {
      jobId: job.id,
      jobVersion: 4,
      tasks: [source, child, target],
    };
    const moved = {
      jobId: job.id,
      jobVersion: 5,
      tasks: [
        source,
        target,
        { ...child, parentTaskId: target.id, version: 3 },
      ],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue(hierarchy),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn().mockResolvedValue(moved),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `View tasks for ${job.name}` }),
    );
    await user.selectOptions(
      screen.getByLabelText("New parent for Layout"),
      target.id,
    );
    const moveButton = screen.getAllByRole("button", {
      name: "Move to parent",
    })[1]!;
    moveButton.focus();
    await user.keyboard("{Enter}");

    expect(client.reorderTask).toHaveBeenCalledWith({
      taskId: child.id,
      newParentTaskId: target.id,
      newSiblingIndex: 0,
      expectedVersion: 2,
      expectedJobVersion: 4,
    });
    expect(screen.getByDisplayValue("Layout").closest("ol")).not.toBe(
      screen.getByRole("list", { name: `Tasks for ${job.name}` }),
    );
  });

  it("keeps another dirty draft bound to its original version across a refresh", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 3 };
    const first = fixtureTask(job, "first", null, "First task", 0, 1);
    const second = fixtureTask(job, "second", null, "Second task", 1, 1);
    const hierarchy = { jobId: job.id, jobVersion: 3, tasks: [first, second] };
    const refreshed = {
      jobId: job.id,
      jobVersion: 5,
      tasks: [
        { ...first, name: "Remote first", version: 2 },
        { ...second, name: "Remote second", version: 2 },
      ],
    };
    const refreshedJob = { ...job, version: refreshed.jobVersion };
    const client: JobClient = {
      listJobs: vi
        .fn()
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([refreshedJob]),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce(hierarchy)
        .mockResolvedValueOnce(refreshed),
      createTask: vi.fn(),
      updateTask: vi
        .fn()
        .mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `View tasks for ${job.name}` }),
    );
    const firstEdit = screen.getByLabelText("Task name for First task");
    const secondEdit = screen.getByLabelText("Task name for Second task");
    await user.clear(secondEdit);
    await user.type(secondEdit, "My second draft");
    await user.clear(firstEdit);
    await user.type(firstEdit, "My first draft");
    await user.click(firstEdit.closest("form")!.querySelector("button")!);
    await user.click(
      await screen.findByRole("button", { name: "Refresh tasks" }),
    );

    expect(secondEdit).toHaveValue("My second draft");
    expect(
      await screen.findByText(
        "Tasks changed elsewhere. Your pending change is still here.",
      ),
    ).toBeVisible();
    await user.click(secondEdit.closest("form")!.querySelector("button")!);
    expect(client.updateTask).toHaveBeenLastCalledWith({
      taskId: second.id,
      name: "My second draft",
      expectedVersion: 1,
    });
  });

  it("edits persisted schedule inputs and keeps the shared job version synchronized", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 3, calendar: defaultCalendar() };
    const first = {
      ...fixtureTask(job, "first", null, "Excavate", 0, 1),
      durationMinutes: null,
    };
    const second = {
      ...fixtureTask(job, "second", null, "Inspection", 1, 1),
      durationMinutes: 0,
    };
    const hierarchy = {
      jobId: job.id,
      jobVersion: 3,
      tasks: [first, second],
      dependencies: [],
    };
    const afterDuration = {
      ...hierarchy,
      jobVersion: 4,
      tasks: [{ ...first, durationMinutes: 480, version: 2 }, second],
    };
    const afterSchedule = {
      ...job,
      version: 5,
      scheduleStart: "2026-08-17",
      calendar: {
        ...defaultCalendar(),
        workingWeekdays: [
          ...defaultCalendar().workingWeekdays,
          "saturday" as const,
        ],
      },
    };
    const afterDependency = {
      ...afterDuration,
      jobVersion: 6,
      dependencies: [
        {
          predecessorTaskId: first.id,
          successorTaskId: second.id,
          lagMinutes: 60,
        },
      ],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue(hierarchy),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateTaskDuration: vi.fn().mockResolvedValue(afterDuration),
      updateSchedule: vi.fn().mockResolvedValue(afterSchedule),
      addDependency: vi.fn().mockResolvedValue(afterDependency),
      removeDependency: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `View tasks for ${job.name}` }),
    );
    await user.type(screen.getByLabelText("Duration for Excavate"), "480");
    await user.click(
      screen.getAllByRole("button", { name: "Save duration" })[0]!,
    );
    expect(client.updateTaskDuration).toHaveBeenCalledWith({
      taskId: first.id,
      durationMinutes: 480,
      expectedVersion: 1,
      expectedJobVersion: 3,
    });

    await user.type(
      screen.getByLabelText(`Schedule start for ${job.name}`),
      "2026-08-17",
    );
    await user.click(screen.getByLabelText("Sat"));
    await user.click(
      screen.getByRole("button", { name: "Save schedule settings" }),
    );
    await waitFor(() =>
      expect(client.updateSchedule).toHaveBeenCalledWith(
        expect.objectContaining({
          expectedJobVersion: 4,
          scheduleStart: "2026-08-17",
          calendar: expect.objectContaining({
            workingWeekdays: expect.arrayContaining(["saturday"]),
          }),
        }),
      ),
    );

    await user.selectOptions(
      screen.getByLabelText("Dependency predecessor"),
      first.id,
    );
    await user.selectOptions(
      screen.getByLabelText("Dependency successor"),
      second.id,
    );
    await user.clear(screen.getByLabelText("Dependency lag minutes"));
    await user.type(screen.getByLabelText("Dependency lag minutes"), "60");
    await user.click(screen.getByRole("button", { name: "Add dependency" }));
    await waitFor(() =>
      expect(client.addDependency).toHaveBeenCalledWith({
        jobId: job.id,
        predecessorTaskId: first.id,
        successorTaskId: second.id,
        lagMinutes: 60,
        expectedJobVersion: 5,
      }),
    );
  });

  it("preserves stale schedule input and exposes an explicit refresh path", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 3, calendar: defaultCalendar() };
    const hierarchy = {
      jobId: job.id,
      jobVersion: 3,
      tasks: [],
      dependencies: [],
    };
    const refreshedJob = { ...job, version: 4, scheduleStart: "2026-08-24" };
    const refreshedHierarchy = { ...hierarchy, jobVersion: 4 };
    const client: JobClient = {
      listJobs: vi
        .fn()
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([job])
        .mockResolvedValueOnce([refreshedJob]),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce(hierarchy)
        .mockResolvedValueOnce(refreshedHierarchy),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn().mockRejectedValue({
        kind: "version_conflict",
        message: "stale schedule",
      }),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `View tasks for ${job.name}` }),
    );
    const start = screen.getByLabelText(`Schedule start for ${job.name}`);
    await user.type(start, "2026-08-17");
    await user.click(
      screen.getByRole("button", { name: "Save schedule settings" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Your pending values are still here",
    );
    expect(start).toHaveValue("2026-08-17");

    await user.click(screen.getByRole("button", { name: "Refresh schedule" }));
    expect(client.listJobs).toHaveBeenCalledTimes(3);
    expect(client.listTasks).toHaveBeenCalledTimes(2);
    expect(start).toHaveValue("2026-08-17");
  });
});

function defaultCalendar(): WorkingCalendar {
  return {
    workingWeekdays: ["monday", "tuesday", "wednesday", "thursday", "friday"],
    workdayStartMinute: 480,
    workdayDurationMinutes: 480,
  };
}

function fixtureJob() {
  return {
    id: "019ccab7-bb9f-7000-8000-000000000001",
    name: "Ridgeline Fence — Phase 2",
    status: "draft" as const,
    timezone: "America/New_York",
    createdAt: "2026-08-14T15:00:00.000Z",
    updatedAt: "2026-08-14T15:00:00.000Z",
    version: 1,
  };
}

function fixtureTask(
  job: ReturnType<typeof fixtureJob>,
  id: string,
  parentTaskId: string | null,
  name: string,
  sortKey: number,
  version: number,
) {
  return {
    id,
    jobId: job.id,
    parentTaskId,
    name,
    sortKey,
    version,
    createdAt: job.createdAt,
    updatedAt: job.updatedAt,
  };
}

function ganttReadModel(jobId: string) {
  return {
    contractVersion: 1 as const, jobId, jobVersion: 1,
    scheduleStart: "2026-08-17T08:00:00", scheduleFinish: "2026-08-17T16:00:00",
    baselineId: null, rowCount: 1, criticalTaskIds: ["task"], criticalPath: ["task"],
    rows: [{ taskId: "task", parentTaskId: null, logicalIndex: 0, depth: 1, positionInSet: 1, setSize: 1, sortKey: 0, wbs: "1", name: "Excavate", kind: "task" as const, hasChildren: false, durationMinutes: 480, start: "2026-08-17T08:00:00", finish: "2026-08-17T16:00:00", totalFloatMinutes: 0, critical: true, milestone: false, summary: false, predecessorIds: [], baseline: null }],
  };
}
