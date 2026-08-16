import { invoke } from "@tauri-apps/api/core";

import type {
  CreateJobRequest,
  CreateTaskRequest,
  Job,
  ReorderTaskRequest,
  TaskHierarchy,
  TaskMutation,
  UpdateTaskRequest,
} from "../types/jobs";

export interface JobClient {
  listJobs(): Promise<Job[]>;
  createJob(request: CreateJobRequest): Promise<Job>;
  listTasks(jobId: string): Promise<TaskHierarchy>;
  createTask(request: CreateTaskRequest): Promise<TaskHierarchy>;
  updateTask(request: UpdateTaskRequest): Promise<TaskHierarchy>;
  reorderTask(request: ReorderTaskRequest): Promise<TaskHierarchy>;
}

export const tauriJobClient: JobClient = {
  listJobs: () => invoke<Job[]>("list_jobs"),
  createJob: (request) => invoke<Job>("create_job", { request }),
  listTasks: (jobId) => invoke<TaskHierarchy>("list_tasks", { jobId }),
  async createTask(request) {
    const mutation = await invoke<TaskMutation>("create_task", { request });
    return invoke<TaskHierarchy>("list_tasks", { jobId: mutation.task.jobId });
  },
  async updateTask(request) {
    const mutation = await invoke<TaskMutation>("update_task", { request });
    return invoke<TaskHierarchy>("list_tasks", { jobId: mutation.task.jobId });
  },
  reorderTask: (request) => invoke<TaskHierarchy>("reorder_task", { request }),
};
