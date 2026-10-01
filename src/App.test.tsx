import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { JobClient } from "./api/jobs";
import type { TaskMutation, UpdateTaskConstraintRequest, WorkingCalendar } from "./types/jobs";

describe("job workspace", () => {
  let revealDetails: ((event: MouseEvent) => void) | undefined;
  beforeEach(() => {
    vi.setSystemTime(new Date("2026-08-20T12:00:00"));
    window.localStorage.clear();
    delete document.documentElement.dataset.theme;
    // Legacy operation tests predate the workspace disclosures. They keep
    // exercising the same command paths, so open those named regions after the
    // job workspace has mounted rather than weakening their payload assertions.
    revealDetails = (event) => {
      if (!(event.target instanceof Element) || !event.target.closest(".task-disclosure")) return;
      setTimeout(() => document.querySelectorAll<HTMLDetailsElement>(".advanced-schedule, .task-editor__advanced").forEach((detail) => { detail.open = true; }), 0);
    };
    document.addEventListener("click", revealDetails);
  });

  afterEach(() => {
    if (revealDetails) document.removeEventListener("click", revealDetails);
    vi.useRealTimers();
  });

  it("opens an accessible settings dialog and returns focus when closed", async () => {
    const user = userEvent.setup();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([]), createJob: vi.fn(), listTasks: vi.fn(),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };
    render(<App client={client} />);
    const trigger = await screen.findByRole("button", { name: "Open settings" });
    await user.click(trigger);
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Close settings" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
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
    await user.click(screen.getByRole("button", { name: "Open settings" }));
    await user.type(screen.getByLabelText("Job name"), createdJob.name);
    await user.click(screen.getByRole("button", { name: "Create job" }));

    expect(
      await screen.findByRole("heading", { name: createdJob.name }),
    ).toBeVisible();
    expect(client.createJob).toHaveBeenCalledWith({
      name: createdJob.name,
      timezone: expect.any(String),
    });
    expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
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
    await user.click(screen.getByRole("button", { name: "Switch to dark theme" }));

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
      name: `Open schedule for ${job.name}`,
    });
    expect(listTasks).not.toHaveBeenCalled();
    await user.click(openTasks);

    const taskList = await screen.findByRole("list", {
      name: `Tasks for ${job.name}`,
    });
    expect(screen.getByRole("button", { name: "Site work" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Layout" })).toBeVisible();
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
    expect(await screen.findByRole("treegrid", { name: `Schedule for ${job.name}` })).toBeVisible();
    expect(client.getSchedule).toHaveBeenCalledWith(job.id);
  });

  it("renders the focused task's facts in the explanation panel", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [] }),
      getSchedule: vi.fn().mockResolvedValue(ganttReadModelTwoRows(job.id)),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
    await screen.findByRole("treegrid", { name: `Schedule for ${job.name}` });

    // Focusing the second task's name cell drives the panel to its facts.
    screen.getByRole("rowheader", { name: /2 Backfill, task/ }).focus();
    const panel = await screen.findByRole("region", {
      name: "Schedule explanation for Backfill",
    });
    expect(panel).toHaveTextContent("Finish limited by deadline 2026-08-20");
  });

  it("recovers the panel to the surviving task when the focused task leaves the projection", async () => {
    const user = userEvent.setup();
    // A tiny in-memory store: the first projection has two tasks; a version bump
    // (a calendar-exception add) reloads a projection missing the focused task.
    let version = 3;
    const base = { ...fixtureJob(), calendar: defaultCalendar() };
    const jobState = () => ({ ...base, version, calendarExceptions: [] as string[] });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([jobState()])),
      createJob: vi.fn(),
      listTasks: vi.fn().mockImplementation(() =>
        Promise.resolve({ jobId: base.id, jobVersion: version, tasks: [], dependencies: [] }),
      ),
      getSchedule: vi.fn().mockImplementation(() =>
        // Before the bump two tasks exist; after it only "task" remains.
        Promise.resolve(version === 3 ? ganttReadModelTwoRows(base.id) : ganttReadModel(base.id)),
      ),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      addCalendarException: vi.fn().mockImplementation(async () => {
        version += 1;
        return jobState();
      }),
      removeCalendarException: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${base.name}` }));
    await screen.findByRole("treegrid", { name: `Schedule for ${base.name}` });
    screen.getByRole("rowheader", { name: /2 Backfill, task/ }).focus();
    await screen.findByRole("region", { name: "Schedule explanation for Backfill" });

    // Bump the job version so the schedule reloads without "task-2".
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${base.name}`),
      "2026-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));

    // The stale focused task no longer resolves to a row; its facts drop out.
    await waitFor(() =>
      expect(screen.queryByText("Finish limited by deadline 2026-08-20")).not.toBeInTheDocument(),
    );
    // The treegrid's focus recovery re-points the roving cell at the first
    // visible row of the reloaded projection, so the panel settles on the
    // surviving task's facts rather than a stale or empty state.
    await new Promise((resolve) => setTimeout(resolve, 30));
    await waitFor(() => {
      expect(
        screen.getByRole("region", { name: "Schedule explanation for Excavate" }),
      ).toBeInTheDocument();
    });
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
    expect(await screen.findByText("No scheduled tasks yet.")).toBeVisible();
  });

  it("keeps expected missing schedule inputs instructional", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [] }),
      getSchedule: vi.fn().mockRejectedValue({ kind: "validation", code: "schedule_start_required", message: "set a schedule start before viewing the schedule" }),
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
    expect(await screen.findByText("Choose a schedule start to calculate task dates.")).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps missing leaf durations instructional and retries genuine schedule failures", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const getSchedule = vi.fn()
      .mockRejectedValueOnce({ kind: "validation", code: "summary_without_children", message: "summary task task must have at least one child" })
      .mockRejectedValueOnce(new Error("scheduler unavailable"))
      .mockResolvedValue(ganttReadModel(job.id));
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue({ jobId: job.id, jobVersion: 1, tasks: [] }),
      getSchedule,
      createTask: vi.fn(), updateTask: vi.fn(), reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
    expect(await screen.findByText("Add a duration to every leaf task to calculate the schedule.")).toBeVisible();

    // A later schedule refresh can still fail unexpectedly, and offers an in-place retry.
    await user.click(screen.getByRole("button", { name: `Back to jobs for ${job.name}` }));
    await user.click(screen.getByRole("button", { name: `Open schedule for ${job.name}` }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't build schedule for");
    await user.click(screen.getByRole("button", { name: "Retry schedule" }));
    expect(await screen.findByRole("treegrid", { name: `Schedule for ${job.name}` })).toBeVisible();
    expect(getSchedule).toHaveBeenCalledTimes(3);
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
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.click(screen.getByRole("button", { name: "Site work" }));
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

    await user.click(screen.getByRole("button", { name: "Layout" }));
    const edit = screen.getByLabelText("Task name for Layout");
    await user.clear(edit);
    await user.type(edit, "Layout and stakes");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(client.updateTask).toHaveBeenCalledWith({
      taskId: child.id,
      name: "Layout and stakes",
      expectedVersion: 1,
    });

    await user.click(screen.getByRole("button", { name: "Move down" }));
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
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    const edit = screen.getByLabelText("Task name for Site work");
    fireEvent.change(edit, { target: { value: "My attempted name" } });
    await user.click(edit.closest("form")!.querySelector("button")!);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Tasks changed elsewhere",
    );
    expect(edit).toHaveValue("My attempted name");
    expect(client.listTasks).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Refresh tasks" }));
    expect(client.listTasks).toHaveBeenCalledTimes(3);
    expect(edit).toHaveValue("My attempted name");
    expect(screen.getByRole("button", { name: "Remote closeout" })).toBeVisible();
    await waitFor(() =>
      expect(screen.getByLabelText(`Schedule start for ${job.name}`)).toHaveValue(
        "2026-08-24",
      ),
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
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.click(screen.getByRole("button", { name: "Layout" }));
    await user.selectOptions(
      screen.getByLabelText("New parent for Layout"),
      target.id,
    );
    const moveButton = screen.getByRole("button", { name: "Move to parent" });
    moveButton.focus();
    await user.keyboard("{Enter}");

    expect(client.reorderTask).toHaveBeenCalledWith({
      taskId: child.id,
      newParentTaskId: target.id,
      newSiblingIndex: 0,
      expectedVersion: 2,
      expectedJobVersion: 4,
    });
    expect(screen.getByRole("button", { name: "Layout" })).toBeVisible();
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
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    const firstEdit = screen.getByLabelText("Task name for First task");
    fireEvent.change(firstEdit, { target: { value: "My first draft" } });
    await user.click(firstEdit.closest("form")!.querySelector("button")!);
    await user.click(
      await screen.findByRole("button", { name: "Refresh tasks" }),
    );

    expect(client.updateTask).toHaveBeenCalledWith({
      taskId: first.id,
      name: "My first draft",
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
    expect(screen.queryByLabelText("Start no earlier than for Site work")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Duration for Site work")).not.toBeInTheDocument();
    expect(screen.getByText("Dates and duration come from subtasks.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Excavate" }));

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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
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
          dependencyType: "FS" as const,
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
      removeDependency: vi
        .fn()
        .mockResolvedValue({ ...afterDependency, jobVersion: 7, dependencies: [] }),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.selectOptions(screen.getByLabelText("Duration unit for Excavate"), "minutes");
    await user.type(screen.getByLabelText("Duration for Excavate"), "480");
    await user.click(screen.getByRole("button", { name: "Save duration" }));
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
    await user.selectOptions(screen.getByLabelText("Dependency type"), "SS");
    await user.clear(screen.getByLabelText("Dependency lag minutes"));
    await user.type(screen.getByLabelText("Dependency lag minutes"), "-60");
    await user.click(screen.getByRole("button", { name: "Add dependency" }));
    await waitFor(() =>
      expect(client.addDependency).toHaveBeenCalledWith({
        jobId: job.id,
        predecessorTaskId: first.id,
        successorTaskId: second.id,
        dependencyType: "SS",
        lagMinutes: -60,
        expectedJobVersion: 5,
      }),
    );

    // The removal control is keyed and labeled by (predecessor, successor, type).
    await user.click(
      screen.getByRole("button", {
        name: `Remove FS dependency from ${first.name} to ${second.name}`,
      }),
    );
    await waitFor(() =>
      expect(client.removeDependency).toHaveBeenCalledWith({
        jobId: job.id,
        predecessorTaskId: first.id,
        successorTaskId: second.id,
        dependencyType: "FS",
        expectedJobVersion: 6,
      }),
    );
  });

  it("rejects a non-integer lag client-side without calling the store", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 3, calendar: defaultCalendar() };
    const first = fixtureTask(job, "first", null, "Excavate", 0, 1);
    const second = fixtureTask(job, "second", null, "Inspection", 1, 1);
    const hierarchy = {
      jobId: job.id,
      jobVersion: 3,
      tasks: [first, second],
      dependencies: [],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue(hierarchy),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      addDependency: vi.fn(),
      removeDependency: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.selectOptions(screen.getByLabelText("Dependency predecessor"), first.id);
    await user.selectOptions(screen.getByLabelText("Dependency successor"), second.id);
    // A decimal (or any non-integer that would reach NaN, like a bare "-") must be
    // rejected client-side rather than sent to the store.
    await user.clear(screen.getByLabelText("Dependency lag minutes"));
    await user.type(screen.getByLabelText("Dependency lag minutes"), "1.5");
    await user.click(screen.getByRole("button", { name: "Add dependency" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /whole number of minutes/i,
    );
    expect(client.addDependency).not.toHaveBeenCalled();
  });

  it("surfaces a typed dependency rejection from the server path", async () => {
    const user = userEvent.setup();
    const job = { ...fixtureJob(), version: 3, calendar: defaultCalendar() };
    const first = fixtureTask(job, "first", null, "Excavate", 0, 1);
    const second = fixtureTask(job, "second", null, "Inspection", 1, 1);
    const hierarchy = {
      jobId: job.id,
      jobVersion: 3,
      tasks: [first, second],
      dependencies: [],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValue(hierarchy),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      addDependency: vi
        .fn()
        .mockRejectedValue({ message: "dependency_cycle", kind: "validation" }),
      removeDependency: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.selectOptions(screen.getByLabelText("Dependency predecessor"), first.id);
    await user.selectOptions(screen.getByLabelText("Dependency successor"), second.id);
    await user.selectOptions(screen.getByLabelText("Dependency type"), "SF");
    await user.click(screen.getByRole("button", { name: "Add dependency" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("dependency_cycle");
    expect(client.addDependency).toHaveBeenCalledWith(
      expect.objectContaining({ dependencyType: "SF", lagMinutes: 0 }),
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
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
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

  it("adds and removes a dated calendar exception against the current job version", async () => {
    const user = userEvent.setup();
    // A tiny in-memory job store so add/remove reloads see matching versions.
    let exceptions = ["2026-11-27"];
    let version = 3;
    const base = { ...fixtureJob(), calendar: defaultCalendar() };
    const jobState = () => ({
      ...base,
      version,
      calendarExceptions: [...exceptions].sort(),
    });
    const hierState = () => ({
      jobId: base.id,
      jobVersion: version,
      tasks: [],
      dependencies: [],
    });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([jobState()])),
      createJob: vi.fn(),
      listTasks: vi.fn().mockImplementation(() => Promise.resolve(hierState())),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      addCalendarException: vi.fn().mockImplementation(async (request) => {
        exceptions = [...exceptions, request.date];
        version += 1;
        return jobState();
      }),
      removeCalendarException: vi.fn().mockImplementation(async (request) => {
        exceptions = exceptions.filter((date) => date !== request.date);
        version += 1;
        return jobState();
      }),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${base.name}` }),
    );
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${base.name}`),
      "2026-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));
    await waitFor(() =>
      expect(client.addCalendarException).toHaveBeenCalledWith({
        jobId: base.id,
        date: "2026-11-26",
        expectedJobVersion: 3,
      }),
    );
    // The new closure appears in the sorted list with its own remove control.
    const removeAdded = await screen.findByRole("button", {
      name: "Remove calendar exception 2026-11-26",
    });
    expect(removeAdded).toBeVisible();

    await user.click(removeAdded);
    await waitFor(() =>
      expect(client.removeCalendarException).toHaveBeenCalledWith({
        jobId: base.id,
        date: "2026-11-26",
        expectedJobVersion: 4,
      }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", {
          name: "Remove calendar exception 2026-11-26",
        }),
      ).not.toBeInTheDocument(),
    );
  });

  it("surfaces a typed duplicate calendar-exception rejection", async () => {
    const user = userEvent.setup();
    const job = {
      ...fixtureJob(),
      version: 3,
      calendar: defaultCalendar(),
      calendarExceptions: ["2026-11-26"],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValue({ jobId: job.id, jobVersion: 3, tasks: [], dependencies: [] }),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      addCalendarException: vi
        .fn()
        .mockRejectedValue({ message: "calendar_exception_exists", kind: "validation" }),
      removeCalendarException: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${job.name}`),
      "2026-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));

    expect(await screen.findByText("calendar_exception_exists")).toBeVisible();
  });

  it("recovers from a version conflict when managing calendar exceptions", async () => {
    const user = userEvent.setup();
    const job = {
      ...fixtureJob(),
      version: 3,
      calendar: defaultCalendar(),
      calendarExceptions: [] as string[],
    };
    const refreshedJob = { ...job, version: 4 };
    const client: JobClient = {
      listJobs: vi
        .fn()
        .mockResolvedValueOnce([job])
        .mockResolvedValue([refreshedJob]),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValueOnce({ jobId: job.id, jobVersion: 3, tasks: [], dependencies: [] })
        .mockResolvedValue({ jobId: job.id, jobVersion: 4, tasks: [], dependencies: [] }),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      addCalendarException: vi
        .fn()
        .mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      removeCalendarException: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${job.name}`),
      "2026-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));

    expect(await screen.findByText(/changed elsewhere/i)).toBeVisible();
    // The recovery reloaded the job snapshot (a second listTasks) onto v4.
    expect((client.listTasks as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it("surfaces an out-of-range typed calendar-exception rejection", async () => {
    const user = userEvent.setup();
    const job = {
      ...fixtureJob(),
      version: 3,
      calendar: defaultCalendar(),
      calendarExceptions: [] as string[],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi
        .fn()
        .mockResolvedValue({ jobId: job.id, jobVersion: 3, tasks: [], dependencies: [] }),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      addCalendarException: vi.fn().mockRejectedValue({
        message: "calendar_exception_out_of_range",
        kind: "validation",
      }),
      removeCalendarException: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${job.name}` }),
    );
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${job.name}`),
      "1975-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));

    expect(await screen.findByText("calendar_exception_out_of_range")).toBeVisible();
  });

  it("rebases a dirty schedule-start draft when a calendar exception is added, without a conflict", async () => {
    const user = userEvent.setup();
    let exceptions: string[] = [];
    let version = 3;
    const base = { ...fixtureJob(), calendar: defaultCalendar() };
    const jobState = () => ({
      ...base,
      version,
      scheduleStart: null as string | null,
      calendarExceptions: [...exceptions].sort(),
    });
    const hierState = () => ({
      jobId: base.id,
      jobVersion: version,
      tasks: [],
      dependencies: [],
    });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([jobState()])),
      createJob: vi.fn(),
      listTasks: vi.fn().mockImplementation(() => Promise.resolve(hierState())),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn().mockImplementation(async () => {
        version += 1;
        return jobState();
      }),
      addCalendarException: vi.fn().mockImplementation(async (request) => {
        exceptions = [...exceptions, request.date];
        version += 1;
        return jobState();
      }),
      removeCalendarException: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${base.name}` }),
    );
    const startInput = screen.getByLabelText(`Schedule start for ${base.name}`);
    await user.type(startInput, "2026-08-17");
    // Add a closure while the schedule-start draft is dirty.
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${base.name}`),
      "2026-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));
    await waitFor(() =>
      expect(client.addCalendarException).toHaveBeenCalledWith({
        jobId: base.id,
        date: "2026-11-26",
        expectedJobVersion: 3,
      }),
    );
    // No conflict banner is raised and the dirty draft is preserved.
    expect(
      screen.queryByText(/Your pending values are still here/i),
    ).not.toBeInTheDocument();
    expect(startInput).toHaveValue("2026-08-17");
    // Saving now uses the advanced job version, not the stale v3.
    await user.click(
      screen.getByRole("button", { name: "Save schedule settings" }),
    );
    await waitFor(() =>
      expect(client.updateSchedule).toHaveBeenCalledWith(
        expect.objectContaining({
          scheduleStart: "2026-08-17",
          expectedJobVersion: 4,
        }),
      ),
    );
  });

  it("rebases a dirty data-date draft when a calendar exception is added, without a conflict", async () => {
    const user = userEvent.setup();
    let exceptions: string[] = [];
    let version = 3;
    const base = { ...fixtureJob(), calendar: defaultCalendar() };
    const jobState = () => ({
      ...base,
      version,
      dataDate: null as string | null,
      calendarExceptions: [...exceptions].sort(),
    });
    const hierState = () => ({
      jobId: base.id,
      jobVersion: version,
      tasks: [],
      dependencies: [],
    });
    const client: JobClient = {
      listJobs: vi.fn().mockImplementation(() => Promise.resolve([jobState()])),
      createJob: vi.fn(),
      listTasks: vi.fn().mockImplementation(() => Promise.resolve(hierState())),
      createTask: vi.fn(),
      updateTask: vi.fn(),
      reorderTask: vi.fn(),
      updateSchedule: vi.fn(),
      updateJobDataDate: vi.fn().mockImplementation(async () => {
        version += 1;
        return jobState();
      }),
      addCalendarException: vi.fn().mockImplementation(async (request) => {
        exceptions = [...exceptions, request.date];
        version += 1;
        return jobState();
      }),
      removeCalendarException: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(
      await screen.findByRole("button", { name: `Open schedule for ${base.name}` }),
    );
    const dataDateInput = screen.getByLabelText(`Data date for ${base.name}`);
    await user.type(dataDateInput, "2026-08-18");
    // Add a closure while the data-date draft is dirty.
    await user.type(
      screen.getByLabelText(`Calendar exception date for ${base.name}`),
      "2026-11-26",
    );
    await user.click(screen.getByRole("button", { name: "Add closure" }));
    await waitFor(() =>
      expect(client.addCalendarException).toHaveBeenCalledWith({
        jobId: base.id,
        date: "2026-11-26",
        expectedJobVersion: 3,
      }),
    );
    expect(
      screen.queryByText(/Your pending values are still here/i),
    ).not.toBeInTheDocument();
    expect(dataDateInput).toHaveValue("2026-08-18");
    // Saving the data date now uses the advanced job version, not the stale v3.
    await user.click(screen.getByRole("button", { name: "Save data date" }));
    await waitFor(() =>
      expect(client.updateJobDataDate).toHaveBeenCalledWith(
        expect.objectContaining({
          dataDate: "2026-08-18",
          expectedJobVersion: 4,
        }),
      ),
    );
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
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
    await user.click(await screen.findByRole("button", { name: "Open settings" }));
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
    await user.click(await screen.findByRole("button", { name: "Open settings" }));
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
    await user.click(await screen.findByRole("button", { name: "Open settings" }));
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${baseJob.name}` }));
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${job.name}` }));
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${baseJob.name}` }));
    (screen.getByText("Schedule setup").closest("details") as HTMLDetailsElement).open = true;
    const field = screen.getByLabelText(`Data date for ${baseJob.name}`);
    await user.type(field, "2026-08-18");
    await user.click(screen.getByRole("button", { name: "Save data date" }));
    (screen.getByText("Schedule setup").closest("details") as HTMLDetailsElement).open = true;

    expect(await screen.findByText(/Schedule inputs changed elsewhere/)).toBeVisible();
    expect(field).toHaveValue("2026-08-18");

    await user.click(screen.getByRole("button", { name: "Refresh schedule" }));
    expect(field).toHaveValue("2026-08-18");

    await user.click(screen.getByRole("button", { name: "Save data date" }));
    await waitFor(() => expect(updateJobDataDate).toHaveBeenLastCalledWith({
      jobId: baseJob.id, dataDate: "2026-08-18", expectedJobVersion: 3,
    }));

    // The successful save must not re-raise the schedule conflict banner.
    (screen.getByText("Schedule setup").closest("details") as HTMLDetailsElement).open = true;
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${baseJob.name}` }));
    (screen.getByText("Schedule setup").closest("details") as HTMLDetailsElement).open = true;
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
    // The created date renders as a local ISO date, not a locale-formatted one.
    expect(screen.getByText("Created 2026-08-18")).toBeVisible();
  });

  it("switches the comparison default baseline and reloads the snapshot", async () => {
    const user = userEvent.setup();
    const baseJob = { ...fixtureJob(), version: 5, scheduleStart: "2026-08-17", calendar: defaultCalendar() };
    const leaf = { ...fixtureTask(baseJob, "leaf", null, "Excavate", 0, 1), durationMinutes: 480 };
    let currentJob = baseJob;
    let currentBaselines: import("./types/jobs").Baseline[] = [
      { id: "b1", jobId: baseJob.id, name: "First", createdAt: "2026-08-18T12:00:00.000Z", isComparisonDefault: true },
      { id: "b2", jobId: baseJob.id, name: "Second", createdAt: "2026-08-18T13:00:00.000Z", isComparisonDefault: false },
      { id: "b3", jobId: baseJob.id, name: "Third", createdAt: "2026-08-18T14:00:00.000Z", isComparisonDefault: false },
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
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${baseJob.name}` }));
    await screen.findByText("First");

    // Each non-default baseline exposes a distinctly named switch control; the
    // default ("First") shows a badge instead of a button.
    expect(screen.queryByRole("button", { name: 'Set "First" as comparison default' })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: 'Set "Third" as comparison default' })).toBeInTheDocument();
    const switchButton = await screen.findByRole("button", { name: 'Set "Second" as comparison default' });
    await user.click(switchButton);
    await waitFor(() =>
      expect(setBaselineComparisonDefault).toHaveBeenCalledWith({
        jobId: baseJob.id, baselineId: "b2", expectedJobVersion: 5,
      }),
    );
    expect(await screen.findByText("Comparison baseline updated.")).toBeVisible();
  });

  it("does not raise a phantom schedule conflict after a sibling baseline create", async () => {
    const user = userEvent.setup();
    const baseJob = { ...fixtureJob(), version: 2, scheduleStart: "2026-08-17", calendar: defaultCalendar() };
    const leaf = { ...fixtureTask(baseJob, "leaf", null, "Excavate", 0, 1), durationMinutes: 480 };
    let currentJob = baseJob;
    let currentBaselines: import("./types/jobs").Baseline[] = [];
    const updateJobDataDate = vi.fn().mockImplementation(() => {
      currentJob = { ...currentJob, version: currentJob.version + 1, dataDate: "2026-08-18" };
      return Promise.resolve(currentJob);
    });
    const createBaseline = vi.fn().mockImplementation(() => {
      // Creating a baseline bumps the job version WITHOUT changing schedule inputs.
      currentJob = { ...currentJob, version: currentJob.version + 1 };
      currentBaselines = [
        { id: "b1", jobId: baseJob.id, name: "Plan", createdAt: "2026-08-18T12:00:00.000Z", isComparisonDefault: true },
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
      updateSchedule: vi.fn(), updateJobDataDate,
      listBaselines: vi.fn().mockImplementation(() => Promise.resolve(currentBaselines)),
      createBaseline,
      setBaselineComparisonDefault: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `Open schedule for ${baseJob.name}` }));

    // Draft a data date (dirty, based on job version 2) without saving it.
    const dateField = screen.getByLabelText(`Data date for ${baseJob.name}`);
    await user.type(dateField, "2026-08-18");

    // Create a baseline; its version bump must NOT wedge the dirty data-date draft.
    await user.type(screen.getByLabelText(`New baseline name for ${baseJob.name}`), "Plan");
    await user.click(screen.getByRole("button", { name: "Create baseline" }));
    expect(await screen.findByText(/Baseline .* created/)).toBeVisible();
    expect(screen.queryByText(/Schedule inputs changed elsewhere/)).not.toBeInTheDocument();
    expect(dateField).toHaveValue("2026-08-18");

    // The retried save re-baselines to the bumped version and succeeds.
    await user.click(screen.getByRole("button", { name: "Save data date" }));
    await waitFor(() =>
      expect(updateJobDataDate).toHaveBeenLastCalledWith({
        jobId: baseJob.id, dataDate: "2026-08-18", expectedJobVersion: 3,
      }),
    );
    (screen.getByText("Schedule setup").closest("details") as HTMLDetailsElement).open = true;
    expect(await screen.findByText("Data date saved.")).toBeVisible();
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
    contractVersion: 7 as const, jobId, jobVersion: 1,
    scheduleStart: "2026-08-17T08:00:00", scheduleFinish: "2026-08-17T16:00:00",
    dataDate: null, baselineId: null, rowCount: 1, criticalTaskIds: ["task"], criticalPath: ["task"],
    calendar: { workingWeekdays: ["monday", "tuesday", "wednesday", "thursday", "friday"], exceptionDates: [] },
    rows: [{ taskId: "task", parentTaskId: null, logicalIndex: 0, depth: 1, positionInSet: 1, setSize: 1, sortKey: 0, wbs: "1", name: "Excavate", kind: "task" as const, hasChildren: false, durationMinutes: 480, start: "2026-08-17T08:00:00", finish: "2026-08-17T16:00:00", totalFloatMinutes: 0, startNoEarlierThan: null, finishNoLaterThan: null, constraintViolated: false, critical: true, milestone: false, summary: false, percentComplete: 0, actualStart: null, actualFinish: null, progressStatus: "notStarted" as const, predecessors: [], baseline: null, explanation: { kind: "scheduled" as const, taskId: "task", primaryDriver: { kind: "scheduleStart" as const }, otherBindingDrivers: [], startedActualStart: null, calendarGap: null, totalFloatMinutes: 0, critical: true, lateFinishLimit: { kind: "projectFinish" as const } } }],
  };
}

// A two-root projection whose second task carries a distinctive deadline limit so
// the explanation panel's focused-task facts are unambiguous.
function ganttReadModelTwoRows(jobId: string) {
  const base = ganttReadModel(jobId);
  return {
    ...base,
    rowCount: 2,
    criticalTaskIds: ["task", "task-2"],
    criticalPath: ["task", "task-2"],
    rows: [
      base.rows[0]!,
      {
        ...base.rows[0]!,
        taskId: "task-2",
        logicalIndex: 1,
        positionInSet: 2,
        setSize: 2,
        sortKey: 1,
        wbs: "2",
        name: "Backfill",
        finishNoLaterThan: "2026-08-20",
        explanation: {
          kind: "scheduled" as const,
          taskId: "task-2",
          primaryDriver: { kind: "scheduleStart" as const },
          otherBindingDrivers: [],
          startedActualStart: null,
          calendarGap: null,
          totalFloatMinutes: 0,
          critical: true,
          lateFinishLimit: {
            kind: "deadline" as const,
            date: "2026-08-20",
            normalizedDate: "2026-08-20",
          },
        },
      },
    ],
  };
}
