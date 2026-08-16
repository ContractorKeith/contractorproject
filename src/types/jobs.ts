export type JobStatus = "draft";

export interface Job {
  id: string;
  name: string;
  status: JobStatus;
  timezone: string;
  createdAt: string;
  updatedAt: string;
  version: number;
  scheduleStart?: string | null;
  calendar?: WorkingCalendar;
}

export type CalendarWeekday = "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday";
export interface WorkingCalendar { workingWeekdays: CalendarWeekday[]; workdayStartMinute: number; workdayDurationMinutes: number; }

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
  durationMinutes?: number | null;
}

export interface TaskHierarchy {
  jobId: string;
  jobVersion: number;
  tasks: Task[];
  dependencies?: FinishStartDependency[];
}
export interface FinishStartDependency { predecessorTaskId: string; successorTaskId: string; lagMinutes: number; }

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
export interface UpdateScheduleRequest { jobId: string; scheduleStart: string | null; calendar: WorkingCalendar; expectedJobVersion: number; }
export interface UpdateTaskDurationRequest { taskId: string; durationMinutes: number | null; expectedVersion: number; expectedJobVersion: number; }
export interface AddDependencyRequest { jobId: string; predecessorTaskId: string; successorTaskId: string; lagMinutes: number; expectedJobVersion: number; }
export interface RemoveDependencyRequest { jobId: string; predecessorTaskId: string; successorTaskId: string; expectedJobVersion: number; }
