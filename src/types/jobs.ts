export type JobStatus = "draft";

export interface Job {
  id: string;
  name: string;
  status: JobStatus;
  timezone: string;
  createdAt: string;
  updatedAt: string;
  version: number;
}

export interface CreateJobRequest {
  name: string;
  timezone: string;
}
