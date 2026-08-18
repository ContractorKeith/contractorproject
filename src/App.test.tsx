import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { JobClient } from "./api/jobs";
import type { TaskMutation, UpdateTaskConstraintRequest, WorkingCalendar } from "./types/jobs";

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

  it("sets, replaces, and clears each leaf constraint through the typed client without erasing the other", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const summary = fixtureTask(job, "summary", null, "Site work", 0, 1);
    const leaf = fixtureTask(job, "leaf", summary.id, "Excavate", 0, 1);
    const initial = { jobId: job.id, jobVersion: 1, tasks: [summary, leaf] };
    const withStart = {
      jobId: job.id,
      jobVersion: 2,
      tasks: [summary, { ...leaf, version: 2, startNoEarlierThan: "2026-09-01" }],
    };
    const withBoth = {
      jobId: job.id,
      jobVersion: 3,
      tasks: [summary, { ...leaf, version: 3, startNoEarlierThan: "2026-09-01", finishNoLaterThan: "2026-09-12" }],
    };
    const replacedStart = {
      jobId: job.id,
      jobVersion: 4,
      tasks: [summary, { ...leaf, version: 4, startNoEarlierThan: "2026-09-03", finishNoLaterThan: "2026-09-12" }],
    };
    const clearedStart = {
      jobId: job.id,
      jobVersion: 5,
      tasks: [summary, { ...leaf, version: 5, startNoEarlierThan: null, finishNoLaterThan: "2026-09-12" }],
    };
    let currentJob = job;
    const updateTaskConstraint = vi
      .fn()
      .mockImplementationOnce(() => {
        currentJob = { ...job, version: 2 };
        return Promise.resolve({ task: withStart.tasks[1], jobVersion: 2 });
      })
      .mockImplementationOnce(() => {
        currentJob = { ...job, version: 3 };
        return Promise.resolve({ task: withBoth.tasks[1], jobVersion: 3 });
      })
      .mockImplementationOnce(() => {
        currentJob = { ...job, version: 4 };
        return Promise.resolve({ task: replacedStart.tasks[1], jobVersion: 4 });
      })
      .mockImplementationOnce(() => {
        currentJob = { ...job, version: 5 };
        return Promise.resolve({ task: clearedStart.tasks[1], jobVersion: 5 });
      });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([currentJob])),
      createJob: vi.fn(),
      listTasks: vi.fn()
        .mockResolvedValueOnce(initial)
        .mockResolvedValueOnce(withStart)
        .mockResolvedValueOnce(withBoth)
        .mockResolvedValueOnce(replacedStart)
        .mockResolvedValueOnce(clearedStart),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(), updateTaskConstraint,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    expect(screen.queryByLabelText("Start no earlier than for Site work")).not.toBeInTheDocument();

    const start = screen.getByLabelText("Start no earlier than for Excavate");
    await user.type(start, "2026-09-01");
    await user.click(screen.getByRole("button", { name: "Save start constraint" }));
    await waitFor(() => expect(updateTaskConstraint).toHaveBeenLastCalledWith({
      taskId: leaf.id, kind: "start_no_earlier_than", value: "2026-09-01", expectedVersion: 1, expectedJobVersion: 1,
    }));

    const finish = screen.getByLabelText("Finish no later than for Excavate");
    await user.type(finish, "2026-09-12");
    await user.click(screen.getByRole("button", { name: "Save finish constraint" }));
    await waitFor(() => expect(updateTaskConstraint).toHaveBeenLastCalledWith({
      taskId: leaf.id, kind: "finish_no_later_than", value: "2026-09-12", expectedVersion: 2, expectedJobVersion: 2,
    }));

    await user.clear(start);
    await user.type(start, "2026-09-03");
    await user.click(screen.getByRole("button", { name: "Save start constraint" }));
    await waitFor(() => expect(updateTaskConstraint).toHaveBeenLastCalledWith({
      taskId: leaf.id, kind: "start_no_earlier_than", value: "2026-09-03", expectedVersion: 3, expectedJobVersion: 3,
    }));
    expect(finish).toHaveValue("2026-09-12");

    await user.click(screen.getByRole("button", { name: "Clear start constraint" }));
    await waitFor(() => expect(updateTaskConstraint).toHaveBeenLastCalledWith({
      taskId: leaf.id, kind: "start_no_earlier_than", value: null, expectedVersion: 4, expectedJobVersion: 4,
    }));
    expect(finish).toHaveValue("2026-09-12");
  });

  it("keeps a stale constraint draft visible until the user explicitly refreshes", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const task = fixtureTask(job, "leaf", null, "Excavate", 0, 1);
    const initial = { jobId: job.id, jobVersion: 1, tasks: [task] };
    const refreshedJob = { ...job, version: 2 };
    const refreshed = { jobId: job.id, jobVersion: 2, tasks: [{ ...task, version: 2, finishNoLaterThan: "2026-09-09" }] };
    let jobLoads = 0;
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([jobLoads++ >= 2 ? refreshedJob : job])),
      createJob: vi.fn(), listTasks: vi.fn().mockResolvedValueOnce(initial).mockResolvedValueOnce(refreshed),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
      updateTaskConstraint: vi.fn().mockRejectedValue({ kind: "version_conflict", message: "stale" }),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    const finish = screen.getByLabelText("Finish no later than for Excavate");
    await user.type(finish, "2026-09-12");
    await user.click(screen.getByRole("button", { name: "Save finish constraint" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Tasks changed elsewhere");
    expect(finish).toHaveValue("2026-09-12");

    await user.click(screen.getByRole("button", { name: "Refresh tasks" }));
    expect(finish).toHaveValue("2026-09-12");
    expect(client.listTasks).toHaveBeenCalledTimes(2);
  });

  it("disables a pending leaf constraint save and surfaces bounded validation errors", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const task = fixtureTask(job, "leaf", null, "Excavate", 0, 1);
    let rejectSave: ((reason: unknown) => void) | undefined;
    const updateTaskConstraint = vi.fn(
      (_request: UpdateTaskConstraintRequest) =>
        new Promise<TaskMutation>((_resolve, reject) => {
          rejectSave = reject;
        }),
    );
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]), createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [task] }),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(), updateTaskConstraint,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    const start = screen.getByLabelText("Start no earlier than for Excavate");
    await user.type(start, "2026-09-01");
    await user.click(screen.getByRole("button", { name: "Save start constraint" }));
    expect(start).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save finish constraint" })).toBeDisabled();
    rejectSave?.({ message: "invalid_constraint_value" });
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid_constraint_value");
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

  it("archives the active job with its displayed version and clears its selected task state", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 7 };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      archiveJob: vi.fn().mockResolvedValue({ ...job, status: "archived", version: 8 }),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: job.version, tasks: [] }),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    expect(await screen.findByText("No tasks yet.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Archive job" }));

    expect(client.archiveJob).toHaveBeenCalledWith({ jobId: job.id, expectedJobVersion: 7 });
    expect(screen.queryByRole("heading", { name: job.name })).not.toBeInTheDocument();
    expect(screen.queryByText("No tasks yet.")).not.toBeInTheDocument();
  });

  it("opens archived jobs with the keyboard, shows their count, and restores with the archived version", async () => {
    const user = userEvent.setup();
    const archived = { ...fixtureJob(), name: "Completed patio", status: "archived" as const, version: 4 };
    const restored = { ...archived, status: "draft" as const, version: 5 };
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation((status?: string) => Promise.resolve(status === "archived" ? [archived] : [])),
      createJob: vi.fn(),
      restoreJob: vi.fn().mockResolvedValue(restored),
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    const disclosure = await screen.findByRole("button", { name: "View archived jobs" });
    disclosure.focus();
    await user.keyboard("{Enter}");

    expect(await screen.findByText("1 archived job")).toBeVisible();
    expect(screen.getByRole("list", { name: "Archived jobs" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: `Restore ${archived.name}` }));
    expect(client.restoreJob).toHaveBeenCalledWith({ jobId: archived.id, expectedJobVersion: 4 });
    expect(await screen.findByRole("heading", { name: archived.name })).toBeVisible();
  });

  it("requires an explicit refresh after an archive version conflict", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const refreshed = { ...job, version: 2 };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValueOnce([job]).mockResolvedValueOnce([refreshed]),
      createJob: vi.fn(),
      archiveJob: vi.fn().mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: "Archive job" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Job changed elsewhere");
    expect(client.listJobs).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Refresh jobs" }));
    expect(client.listJobs).toHaveBeenCalledTimes(2);
  });

  it("keeps an archived job visible until an explicit refresh after a restore conflict", async () => {
    const user = userEvent.setup();
    const archived = { ...fixtureJob(), status: "archived" as const, version: 4 };
    const refreshed = { ...archived, version: 5 };
    let archivedLoads = 0;
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation((status?: string) => {
        if (status !== "archived") return Promise.resolve([]);
        archivedLoads += 1;
        return Promise.resolve([archivedLoads === 1 ? archived : refreshed]);
      }),
      createJob: vi.fn(),
      restoreJob: vi.fn().mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: "View archived jobs" }));
    await user.click(await screen.findByRole("button", { name: `Restore ${archived.name}` }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Job changed elsewhere");
    expect(screen.getByRole("button", { name: `Restore ${archived.name}` })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Refresh jobs" }));
    expect(await screen.findByRole("button", { name: `Restore ${refreshed.name}` })).toBeVisible();
    expect(client.restoreJob).toHaveBeenCalledTimes(1);
  });

  it("surfaces generic archive failures without removing the active job", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      archiveJob: vi.fn().mockRejectedValue(new Error("disk unavailable")),
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: "Archive job" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("disk unavailable");
    expect(screen.getByRole("heading", { name: job.name })).toBeVisible();
  });

  it("creates a verified backup from the accessible keyboard action and restores focus", async () => {
    const user = userEvent.setup();
    const createVerifiedBackup = vi
      .fn()
      .mockResolvedValue({
        destination: "/Users/tester/Documents/ContractorProject-backup-2026-08-16.sqlite3",
        createdAtUtc: "2026-08-16T18:00:00.000Z",
        byteSize: 4096,
        verified: true,
      });
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([]), createJob: vi.fn(), createVerifiedBackup,
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    const backup = await screen.findByRole("button", { name: "Create verified backup" });
    backup.focus();
    await user.keyboard("{Enter}");

    expect(createVerifiedBackup).toHaveBeenCalledOnce();
    expect(await screen.findByText(/Verified backup created:/)).toHaveTextContent("4,096 bytes");
    expect(backup).toHaveFocus();
  });

  it("supports Space, keeps the backup action disabled while pending, and reports cancellation", async () => {
    const user = userEvent.setup();
    let resolveBackup: ((value: null) => void) | undefined;
    const createVerifiedBackup = vi.fn(
      () => new Promise<null>((resolve) => { resolveBackup = resolve; }),
    );
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([]), createJob: vi.fn(), createVerifiedBackup,
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    const backup = await screen.findByRole("button", { name: "Create verified backup" });
    backup.focus();
    await user.keyboard(" ");

    expect(createVerifiedBackup).toHaveBeenCalledOnce();
    expect(backup).toBeDisabled();
    expect(backup).toHaveTextContent("Creating backup…");
    resolveBackup?.(null);
    expect(await screen.findByText("Backup cancelled. Your local data was not changed.")).toBeVisible();
    expect(backup).toHaveFocus();
  });

  it("surfaces a bounded verified-backup failure without changing the job list", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]), createJob: vi.fn(),
      createVerifiedBackup: vi.fn().mockRejectedValue({ message: "backup verification failed" }),
      listTasks: vi.fn(), createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: "Create verified backup" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't create verified backup.");
    expect(screen.getByRole("heading", { name: job.name })).toBeVisible();
  });

  it("sets and clears leaf progress through the typed client", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const leaf = {
      ...fixtureTask(job, "leaf", null, "Excavate", 0, 1),
      durationMinutes: 480,
    };
    const initial = { jobId: job.id, jobVersion: 1, tasks: [leaf] };
    const afterSet = {
      jobId: job.id,
      jobVersion: 2,
      tasks: [{ ...leaf, version: 2, percentComplete: 50, actualStart: "2026-08-17" }],
    };
    const afterClear = {
      jobId: job.id,
      jobVersion: 3,
      tasks: [{ ...leaf, version: 3, percentComplete: null, actualStart: null, actualFinish: null }],
    };
    let currentJob = job;
    const updateTaskProgress = vi
      .fn()
      .mockImplementationOnce(() => {
        currentJob = { ...job, version: 2 };
        return Promise.resolve({ task: afterSet.tasks[0], jobVersion: 2 });
      })
      .mockImplementationOnce(() => {
        currentJob = { ...job, version: 3 };
        return Promise.resolve({ task: afterClear.tasks[0], jobVersion: 3 });
      });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([currentJob])),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce(initial)
        .mockResolvedValueOnce(afterSet)
        .mockResolvedValueOnce(afterClear),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(), updateTaskProgress,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    await user.type(screen.getByLabelText("Percent complete for Excavate"), "50");
    await user.type(screen.getByLabelText("Actual start for Excavate"), "2026-08-17");
    await user.click(screen.getByRole("button", { name: "Save progress" }));
    await waitFor(() => expect(updateTaskProgress).toHaveBeenLastCalledWith({
      taskId: leaf.id, clear: false, percentComplete: 50, actualStart: "2026-08-17",
      actualFinish: null, expectedVersion: 1, expectedJobVersion: 1,
    }));

    await user.click(screen.getByRole("button", { name: "Clear progress" }));
    await waitFor(() => expect(updateTaskProgress).toHaveBeenLastCalledWith({
      taskId: leaf.id, clear: true, percentComplete: null, actualStart: null,
      actualFinish: null, expectedVersion: 2, expectedJobVersion: 2,
    }));
  });

  it("sets and clears the job data date through the typed client", async () => {
    const user = userEvent.setup();
    const baseJob = { ...fixtureJob(), version: 2, scheduleStart: "2026-08-17", calendar: defaultCalendar() };
    const leaf = {
      ...fixtureTask(baseJob, "leaf", null, "Excavate", 0, 1),
      durationMinutes: 480,
    };
    const hierarchyV2 = { jobId: baseJob.id, jobVersion: 2, tasks: [leaf], dependencies: [] };
    const hierarchyV3 = { ...hierarchyV2, jobVersion: 3 };
    const hierarchyV4 = { ...hierarchyV2, jobVersion: 4 };
    let currentJob = baseJob;
    const updateJobDataDate = vi
      .fn()
      .mockImplementationOnce(() => {
        currentJob = { ...baseJob, version: 3, dataDate: "2026-08-18" };
        return Promise.resolve(currentJob);
      })
      .mockImplementationOnce(() => {
        currentJob = { ...baseJob, version: 4, dataDate: null };
        return Promise.resolve(currentJob);
      });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([currentJob])),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce(hierarchyV2)
        .mockResolvedValueOnce(hierarchyV3)
        .mockResolvedValueOnce(hierarchyV4),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
      updateSchedule: vi.fn(), updateJobDataDate,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${baseJob.name}` }));
    await user.type(screen.getByLabelText(`Data date for ${baseJob.name}`), "2026-08-18");
    await user.click(screen.getByRole("button", { name: "Save data date" }));
    await waitFor(() => expect(updateJobDataDate).toHaveBeenLastCalledWith({
      jobId: baseJob.id, dataDate: "2026-08-18", expectedJobVersion: 2,
    }));

    await user.click(await screen.findByRole("button", { name: "Clear data date" }));
    await waitFor(() => expect(updateJobDataDate).toHaveBeenLastCalledWith({
      jobId: baseJob.id, dataDate: null, expectedJobVersion: 3,
    }));
  });

  it("rejects a non-integer percent complete before calling the client", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const leaf = {
      ...fixtureTask(job, "leaf", null, "Excavate", 0, 1),
      durationMinutes: 480,
    };
    const updateTaskProgress = vi.fn();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [leaf] }),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(), updateTaskProgress,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    fireEvent.change(screen.getByLabelText("Percent complete for Excavate"), {
      target: { value: "1.5" },
    });
    await user.click(screen.getByRole("button", { name: "Save progress" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Percent complete must be a whole number between 0 and 100.",
    );
    expect(updateTaskProgress).not.toHaveBeenCalled();
  });

  it("re-baselines a data-date draft after a refresh so the retried save succeeds", async () => {
    const user = userEvent.setup();
    const baseJob = { ...fixtureJob(), version: 2, scheduleStart: "2026-08-17", calendar: defaultCalendar() };
    const leaf = {
      ...fixtureTask(baseJob, "leaf", null, "Excavate", 0, 1),
      durationMinutes: 480,
    };
    const hierarchyV2 = { jobId: baseJob.id, jobVersion: 2, tasks: [leaf], dependencies: [] };
    const hierarchyV3 = { ...hierarchyV2, jobVersion: 3 };
    const hierarchyV4 = { ...hierarchyV2, jobVersion: 4 };
    const refreshedJob = { ...baseJob, version: 3 };
    const savedJob = { ...baseJob, version: 4, dataDate: "2026-08-18" };
    let currentJob = baseJob;
    const updateJobDataDate = vi
      .fn()
      .mockImplementationOnce(() => {
        // Another writer advanced the job version between load and save.
        currentJob = refreshedJob;
        return Promise.reject({ kind: "version_conflict", message: "stale" });
      })
      .mockImplementationOnce(() => {
        currentJob = savedJob;
        return Promise.resolve(savedJob);
      });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([currentJob])),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce(hierarchyV2)
        .mockResolvedValueOnce(hierarchyV3)
        .mockResolvedValueOnce(hierarchyV4),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
      updateSchedule: vi.fn(), updateJobDataDate,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${baseJob.name}` }));
    const field = screen.getByLabelText(`Data date for ${baseJob.name}`);
    await user.type(field, "2026-08-18");
    await user.click(screen.getByRole("button", { name: "Save data date" }));

    expect(await screen.findByText(/Schedule inputs changed elsewhere/)).toBeVisible();
    expect(field).toHaveValue("2026-08-18");

    await user.click(screen.getByRole("button", { name: "Refresh schedule" }));
    expect(field).toHaveValue("2026-08-18");

    await user.click(screen.getByRole("button", { name: "Save data date" }));
    await waitFor(() => expect(updateJobDataDate).toHaveBeenLastCalledWith({
      jobId: baseJob.id, dataDate: "2026-08-18", expectedJobVersion: 3,
    }));

    // The successful save must not re-raise the schedule conflict banner.
    expect(await screen.findByText("Data date saved.")).toBeVisible();
    expect(screen.queryByText(/Schedule inputs changed elsewhere/)).not.toBeInTheDocument();
  });

  it("creates a named baseline, surfaces a duplicate-name rejection, and lists it", async () => {
    const user = userEvent.setup();
    const baseJob = { ...fixtureJob(), version: 2, scheduleStart: "2026-08-17", calendar: defaultCalendar() };
    const leaf = { ...fixtureTask(baseJob, "leaf", null, "Excavate", 0, 1), durationMinutes: 480 };
    let currentJob = baseJob;
    let currentBaselines: import("./types/jobs").Baseline[] = [];
    const createBaseline = vi
      .fn()
      .mockImplementationOnce(() =>
        Promise.reject({
          kind: "validation_failed",
          message: "a baseline with this name already exists for the job",
        }),
      )
      .mockImplementationOnce(() => {
        currentJob = { ...baseJob, version: 3 };
        currentBaselines = [
          {
            id: "baseline-1",
            jobId: baseJob.id,
            name: "Original plan",
            createdAt: "2026-08-18T12:00:00.000Z",
            isComparisonDefault: true,
          },
        ];
        return Promise.resolve(currentBaselines[0]);
      });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([currentJob])),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockImplementation(() =>
          Promise.resolve({ jobId: baseJob.id, jobVersion: currentJob.version, tasks: [leaf], dependencies: [] }),
        ),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      listBaselines: vi.fn().mockImplementation(() => Promise.resolve(currentBaselines)),
      createBaseline,
      setBaselineComparisonDefault: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${baseJob.name}` }));
    expect(await screen.findByText("No baselines yet.")).toBeVisible();

    const nameField = screen.getByLabelText(`New baseline name for ${baseJob.name}`);
    await user.type(nameField, "Original plan");
    await user.click(screen.getByRole("button", { name: "Create baseline" }));

    // The typed duplicate-name rejection surfaces without wedging the form.
    expect(await screen.findByText(/a baseline with this name already exists/)).toBeVisible();
    expect(createBaseline).toHaveBeenLastCalledWith({
      jobId: baseJob.id, name: "Original plan", expectedJobVersion: 2,
    });

    await user.click(screen.getByRole("button", { name: "Create baseline" }));
    await waitFor(() =>
      expect(screen.getByText("Original plan")).toBeInTheDocument(),
    );
    expect(screen.getByText("Comparison default")).toBeVisible();
  });

  it("switches the comparison default baseline and reloads the snapshot", async () => {
    const user = userEvent.setup();
    const baseJob = { ...fixtureJob(), version: 5, scheduleStart: "2026-08-17", calendar: defaultCalendar() };
    const leaf = { ...fixtureTask(baseJob, "leaf", null, "Excavate", 0, 1), durationMinutes: 480 };
    let currentJob = baseJob;
    let currentBaselines: import("./types/jobs").Baseline[] = [
      { id: "b1", jobId: baseJob.id, name: "First", createdAt: "2026-08-18T12:00:00.000Z", isComparisonDefault: true },
      { id: "b2", jobId: baseJob.id, name: "Second", createdAt: "2026-08-18T13:00:00.000Z", isComparisonDefault: false },
    ];
    const setBaselineComparisonDefault = vi.fn().mockImplementation(() => {
      currentJob = { ...baseJob, version: 6 };
      currentBaselines = currentBaselines.map((baseline) => ({
        ...baseline,
        isComparisonDefault: baseline.id === "b2",
      }));
      return Promise.resolve(currentBaselines[1]);
    });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([currentJob])),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockImplementation(() =>
          Promise.resolve({ jobId: baseJob.id, jobVersion: currentJob.version, tasks: [leaf], dependencies: [] }),
        ),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      listBaselines: vi.fn().mockImplementation(() => Promise.resolve(currentBaselines)),
      createBaseline: vi.fn(),
      setBaselineComparisonDefault,
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${baseJob.name}` }));
    await screen.findByText("First");

    // The non-default baseline exposes a switch control; the default shows a badge.
    const switchButtons = await screen.findAllByRole("button", { name: "Set as comparison default" });
    expect(switchButtons).toHaveLength(1);
    await user.click(switchButtons[0]!);
    await waitFor(() =>
      expect(setBaselineComparisonDefault).toHaveBeenCalledWith({
        jobId: baseJob.id, baselineId: "b2", expectedJobVersion: 5,
      }),
    );
    expect(await screen.findByText("Comparison baseline updated.")).toBeVisible();
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
    dataDate: null as string | null,
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
    contractVersion: 4 as const, jobId, jobVersion: 1,
    scheduleStart: "2026-08-17T08:00:00", scheduleFinish: "2026-08-17T16:00:00",
    dataDate: null, baselineId: null, rowCount: 1, criticalTaskIds: ["task"], criticalPath: ["task"],
    rows: [{ taskId: "task", parentTaskId: null, logicalIndex: 0, depth: 1, positionInSet: 1, setSize: 1, sortKey: 0, wbs: "1", name: "Excavate", kind: "task" as const, hasChildren: false, durationMinutes: 480, start: "2026-08-17T08:00:00", finish: "2026-08-17T16:00:00", totalFloatMinutes: 0, startNoEarlierThan: null, finishNoLaterThan: null, constraintViolated: false, critical: true, milestone: false, summary: false, percentComplete: 0, actualStart: null, actualFinish: null, progressStatus: "notStarted" as const, predecessorIds: [], baseline: null }],
  };
}
