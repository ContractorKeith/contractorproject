import { invoke } from "@tauri-apps/api/core";

import type { CreateJobRequest, Job, TaskHierarchy } from "../types/jobs";

export interface JobClient {
  listJobs(): Promise<Job[]>;
  createJob(request: CreateJobRequest): Promise<Job>;
  listTasks(jobId: string): Promise<TaskHierarchy>;
}

export const tauriJobClient: JobClient = {
  listJobs: () => invoke<Job[]>("list_jobs"),
  createJob: (request) => invoke<Job>("create_job", { request }),
  listTasks: (jobId) => invoke<TaskHierarchy>("list_tasks", { jobId }),
};
