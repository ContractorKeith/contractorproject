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
    };

    render(<App client={client} />);
    await user.selectOptions(screen.getByLabelText("Theme"), "dark");

    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(window.localStorage.getItem("contractorproject.theme")).toBe("dark");
  });
});
