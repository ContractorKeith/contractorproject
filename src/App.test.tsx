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
    };

    render(<App client={client} />);

    const openTasks = await screen.findByRole("button", {
      name: `View tasks for ${job.name}`,
    });
    expect(listTasks).not.toHaveBeenCalled();
    await user.click(openTasks);

    const taskList = await screen.findByRole("list", { name: `Tasks for ${job.name}` });
    expect(taskList).toHaveTextContent("Site work");
    expect(taskList).toHaveTextContent("Layout");
    expect(screen.getByText("Layout").closest("ol")).not.toBe(taskList);
    expect(listTasks).toHaveBeenCalledExactlyOnceWith(job.id);
    expect(openTasks).toHaveAttribute("aria-expanded", "true");
  });
});
