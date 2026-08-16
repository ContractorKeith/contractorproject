import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import type { JobClient } from "./api/jobs";

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

    expect(await screen.findByRole("heading", { name: "No jobs yet" })).toBeVisible();
    await user.type(screen.getByLabelText("Job name"), createdJob.name);
    await user.click(screen.getByRole("button", { name: "Create job" }));

    expect(await screen.findByRole("heading", { name: createdJob.name })).toBeVisible();
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

    const taskList = await screen.findByRole("list", { name: `Tasks for ${job.name}` });
    expect(screen.getByDisplayValue("Site work")).toBeVisible();
    const nestedTask = screen.getByDisplayValue("Layout");
    expect(nestedTask).toBeVisible();
    expect(nestedTask.closest("ol")).not.toBe(taskList);
    expect(listTasks).toHaveBeenCalledExactlyOnceWith(job.id);
    expect(openTasks).toHaveAttribute("aria-expanded", "true");
  });

  it("creates nested tasks, edits a task, and reorders it with keyboard-accessible controls", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const root = fixtureTask(job, "root", null, "Site work", 0, 1);
    const child = fixtureTask(job, "child", "root", "Layout", 0, 1);
    const hierarchy = { jobId: job.id, jobVersion: 3, tasks: [root, child] };
    const afterChild = {
      jobId: job.id,
      jobVersion: 4,
      tasks: [...hierarchy.tasks, fixtureTask(job, "child-two", "root", "Excavation", 1, 1)],
    };
    const afterEdit = {
      ...afterChild,
      jobVersion: 5,
      tasks: afterChild.tasks.map((task) =>
        task.id === "child" ? { ...task, name: "Layout and stakes", version: 2 } : task,
      ),
    };
    const afterReorder = {
      ...afterEdit,
      jobVersion: 6,
      tasks: [afterEdit.tasks[0], afterEdit.tasks[2], { ...afterEdit.tasks[1], sortKey: 1, version: 3 }],
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
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    await user.type(screen.getByLabelText("New child task for Site work"), "Excavation");
    await user.click(screen.getAllByRole("button", { name: "Add subtask" })[0]!);
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
    const job = fixtureJob();
    const task = fixtureTask(job, "task", null, "Site work", 0, 1);
    const untouched = fixtureTask(job, "untouched", null, "Closeout", 1, 1);
    const hierarchy = { jobId: job.id, jobVersion: 2, tasks: [task, untouched] };
    const refreshed = {
      ...hierarchy,
      jobVersion: 3,
      tasks: [
        { ...task, name: "Changed elsewhere", version: 2 },
        { ...untouched, name: "Remote closeout", version: 2 },
      ],
    };
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValueOnce(hierarchy).mockResolvedValueOnce(refreshed),
      createTask: vi.fn(),
      updateTask: vi.fn().mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    const edit = screen.getByLabelText("Task name for Site work");
    await user.clear(edit);
    await user.type(edit, "My attempted name");
    await user.click(edit.closest("form")!.querySelector("button")!);
    expect(await screen.findByRole("alert")).toHaveTextContent("Tasks changed elsewhere");
    expect(edit).toHaveValue("My attempted name");
    expect(client.listTasks).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Refresh tasks" }));
    expect(client.listTasks).toHaveBeenCalledTimes(2);
    expect(edit).toHaveValue("My attempted name");
    expect(await screen.findByDisplayValue("Remote closeout")).toBeVisible();
  });

  it("reparents a task across valid parents using the authoritative hierarchy version", async () => {
    const user = userEvent.setup();
    const job = fixtureJob();
    const source = fixtureTask(job, "source", null, "Source phase", 0, 1);
    const target = fixtureTask(job, "target", null, "Target phase", 1, 1);
    const child = fixtureTask(job, "child", source.id, "Layout", 0, 2);
    const hierarchy = { jobId: job.id, jobVersion: 4, tasks: [source, child, target] };
    const moved = {
      jobId: job.id,
      jobVersion: 5,
      tasks: [source, target, { ...child, parentTaskId: target.id, version: 3 }],
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
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    await user.selectOptions(screen.getByLabelText("New parent for Layout"), target.id);
    const moveButton = screen.getAllByRole("button", { name: "Move to parent" })[1]!;
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
    const job = fixtureJob();
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
    const client: JobClient = {
      listJobs: vi.fn().mockResolvedValue([job]),
      createJob: vi.fn(),
      listTasks: vi.fn().mockResolvedValueOnce(hierarchy).mockResolvedValueOnce(refreshed),
      createTask: vi.fn(),
      updateTask: vi.fn().mockRejectedValue({ kind: "version_conflict", message: "stale" }),
      reorderTask: vi.fn(),
    };

    render(<App client={client} />);
    await user.click(await screen.findByRole("button", { name: `View tasks for ${job.name}` }));
    const firstEdit = screen.getByLabelText("Task name for First task");
    const secondEdit = screen.getByLabelText("Task name for Second task");
    await user.clear(secondEdit);
    await user.type(secondEdit, "My second draft");
    await user.clear(firstEdit);
    await user.type(firstEdit, "My first draft");
    await user.click(firstEdit.closest("form")!.querySelector("button")!);
    await user.click(await screen.findByRole("button", { name: "Refresh tasks" }));

    expect(secondEdit).toHaveValue("My second draft");
    expect(await screen.findByText("Tasks changed elsewhere. Your pending change is still here.")).toBeVisible();
    await user.click(secondEdit.closest("form")!.querySelector("button")!);
    expect(client.updateTask).toHaveBeenLastCalledWith({
      taskId: second.id,
      name: "My second draft",
      expectedVersion: 1,
    });
  });
});

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
  return { id, jobId: job.id, parentTaskId, name, sortKey, version, createdAt: job.createdAt, updatedAt: job.updatedAt };
}
