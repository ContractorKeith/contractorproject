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

export interface Task {
  id: string;
  jobId: string;
  parentTaskId: string | null;
  sortKey: number;
  name: string;
  createdAt: string;
  updatedAt: string;
  version: number;
}

export interface TaskHierarchy {
  jobId: string;
  jobVersion: number;
  tasks: Task[];
}
