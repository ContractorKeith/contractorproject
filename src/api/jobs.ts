import { invoke } from "@tauri-apps/api/core";

import type {
  CreateJobRequest,
  CreateTaskRequest,
  Job,
  ReorderTaskRequest,
  TaskHierarchy,
  TaskMutation,
  UpdateTaskRequest,
  AddDependencyRequest,
  UpdateScheduleRequest,
  UpdateTaskDurationRequest,
  RemoveDependencyRequest,
} from "../types/jobs";
import type { GanttReadModel } from "../types/gantt";

export interface JobClient {
  listJobs(): Promise<Job[]>;
  createJob(request: CreateJobRequest): Promise<Job>;
  listTasks(jobId: string): Promise<TaskHierarchy>;
  getSchedule?(jobId: string): Promise<GanttReadModel>;
  createTask(request: CreateTaskRequest): Promise<TaskHierarchy>;
  updateTask(request: UpdateTaskRequest): Promise<TaskHierarchy>;
  reorderTask(request: ReorderTaskRequest): Promise<TaskHierarchy>;
  updateSchedule?(request: UpdateScheduleRequest): Promise<Job>;
  updateTaskDuration?(request: UpdateTaskDurationRequest): Promise<TaskHierarchy>;
  addDependency?(request: AddDependencyRequest): Promise<TaskHierarchy>;
  removeDependency?(request: RemoveDependencyRequest): Promise<TaskHierarchy>;
}

export const tauriJobClient: JobClient = {
  listJobs: () => invoke<Job[]>("list_jobs"),
  createJob: (request) => invoke<Job>("create_job", { request }),
  listTasks: (jobId) => invoke<TaskHierarchy>("list_tasks", { jobId }),
  getSchedule: (jobId) => invoke<GanttReadModel>("get_schedule", { jobId }),
  async createTask(request) {
    const mutation = await invoke<TaskMutation>("create_task", { request });
    return invoke<TaskHierarchy>("list_tasks", { jobId: mutation.task.jobId });
  },
  async updateTask(request) {
    const mutation = await invoke<TaskMutation>("update_task", { request });
    return invoke<TaskHierarchy>("list_tasks", { jobId: mutation.task.jobId });
  },
  reorderTask: (request) => invoke<TaskHierarchy>("reorder_task", { request }),
  updateSchedule: (request) => invoke<Job>("update_schedule", { request }),
  async updateTaskDuration(request) {
    const mutation = await invoke<TaskMutation>("update_task_duration", { request });
    return invoke<TaskHierarchy>("list_tasks", { jobId: mutation.task.jobId });
  },
  addDependency: (request) => invoke<TaskHierarchy>("add_dependency", { request }),
  removeDependency: (request) => invoke<TaskHierarchy>("remove_dependency", { request }),
};
