import { invoke } from "@tauri-apps/api/core";

import type { CreateJobRequest, Job } from "../types/jobs";

export interface JobClient {
  listJobs(): Promise<Job[]>;
  createJob(request: CreateJobRequest): Promise<Job>;
}

export const tauriJobClient: JobClient = {
  listJobs: () => invoke<Job[]>("list_jobs"),
  createJob: (request) => invoke<Job>("create_job", { request }),
};
