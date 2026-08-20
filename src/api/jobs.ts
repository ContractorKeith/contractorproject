import { invoke } from "@tauri-apps/api/core";

import type {
  CreateJobRequest,
  CreateTaskRequest,
  Job,
  BackupResult,
  JobStatus,
  JobStatusRequest,
  ReorderTaskRequest,
  TaskHierarchy,
  TaskMutation,
  UpdateTaskRequest,
  AddDependencyRequest,
  CalendarExceptionRequest,
  UpdateScheduleRequest,
  UpdateTaskDurationRequest,
  UpdateTaskConstraintRequest,
  UpdateJobDataDateRequest,
  UpdateTaskProgressRequest,
  RemoveDependencyRequest,
  Baseline,
  CreateBaselineRequest,
  SetBaselineComparisonDefaultRequest,
} from "../types/jobs";
import type { GanttReadModel } from "../types/gantt";

export interface JobClient {
  listJobs(status?: JobStatus): Promise<Job[]>;
  createJob(request: CreateJobRequest): Promise<Job>;
  archiveJob?(request: JobStatusRequest): Promise<Job>;
  restoreJob?(request: JobStatusRequest): Promise<Job>;
  createVerifiedBackup?(): Promise<BackupResult | null>;
  listTasks(jobId: string): Promise<TaskHierarchy>;
  getSchedule?(jobId: string): Promise<GanttReadModel>;
  createTask(request: CreateTaskRequest): Promise<TaskHierarchy>;
  updateTask(request: UpdateTaskRequest): Promise<TaskHierarchy>;
  reorderTask(request: ReorderTaskRequest): Promise<TaskHierarchy>;
  updateSchedule?(request: UpdateScheduleRequest): Promise<Job>;
  addCalendarException?(request: CalendarExceptionRequest): Promise<Job>;
  removeCalendarException?(request: CalendarExceptionRequest): Promise<Job>;
  updateTaskDuration?(request: UpdateTaskDurationRequest): Promise<TaskHierarchy>;
  updateTaskConstraint?(request: UpdateTaskConstraintRequest): Promise<TaskMutation>;
  updateJobDataDate?(request: UpdateJobDataDateRequest): Promise<Job>;
  updateTaskProgress?(request: UpdateTaskProgressRequest): Promise<TaskMutation>;
  addDependency?(request: AddDependencyRequest): Promise<TaskHierarchy>;
  removeDependency?(request: RemoveDependencyRequest): Promise<TaskHierarchy>;
  listBaselines?(jobId: string): Promise<Baseline[]>;
  createBaseline?(request: CreateBaselineRequest): Promise<Baseline>;
  setBaselineComparisonDefault?(request: SetBaselineComparisonDefaultRequest): Promise<Baseline>;
}

export const tauriJobClient: JobClient = {
  listJobs: (status) =>
    status === undefined
      ? invoke<Job[]>("list_jobs")
      : invoke<Job[]>("list_jobs", { status }),
  createJob: (request) => invoke<Job>("create_job", { request }),
  archiveJob: (request) => invoke<Job>("archive_job", { request }),
  restoreJob: (request) => invoke<Job>("restore_job", { request }),
  createVerifiedBackup: () => invoke<BackupResult | null>("create_verified_backup"),
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
  addCalendarException: (request) => invoke<Job>("add_calendar_exception", { request }),
  removeCalendarException: (request) => invoke<Job>("remove_calendar_exception", { request }),
  async updateTaskDuration(request) {
    const mutation = await invoke<TaskMutation>("update_task_duration", { request });
    return invoke<TaskHierarchy>("list_tasks", { jobId: mutation.task.jobId });
  },
  updateTaskConstraint: (request) =>
    invoke<TaskMutation>("update_task_constraint", { request }),
  updateJobDataDate: (request) => invoke<Job>("update_job_data_date", { request }),
  updateTaskProgress: (request) =>
    invoke<TaskMutation>("update_task_progress", { request }),
  addDependency: (request) => invoke<TaskHierarchy>("add_dependency", { request }),
  removeDependency: (request) => invoke<TaskHierarchy>("remove_dependency", { request }),
  listBaselines: (jobId) => invoke<Baseline[]>("list_baselines", { jobId }),
  createBaseline: (request) => invoke<Baseline>("create_baseline", { request }),
  setBaselineComparisonDefault: (request) =>
    invoke<Baseline>("set_baseline_comparison_default", { request }),
};
