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

export interface CreateTaskRequest {
  jobId: string;
  parentTaskId: string | null;
  name: string;
  expectedJobVersion: number;
}

export interface UpdateTaskRequest {
  taskId: string;
  name: string;
  expectedVersion: number;
}

export interface ReorderTaskRequest {
  taskId: string;
  newParentTaskId: string | null;
  newSiblingIndex: number;
  expectedVersion: number;
  expectedJobVersion: number;
}

export interface TaskMutation {
  task: Task;
  jobVersion: number;
}
