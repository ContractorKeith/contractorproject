export type JobStatus = "draft" | "archived";

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
  /** Canonical YYYY-MM-DD job-local data date; null while the job is unstatused. */
  dataDate?: string | null;
}

export type CalendarWeekday = "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday";
export interface WorkingCalendar { workingWeekdays: CalendarWeekday[]; workdayStartMinute: number; workdayDurationMinutes: number; }

export interface CreateJobRequest {
  name: string;
  timezone: string;
}

export interface JobStatusRequest {
  jobId: string;
  expectedJobVersion: number;
}

export interface BackupResult {
  destination: string;
  createdAtUtc: string;
  byteSize: number;
  verified: boolean;
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
  startNoEarlierThan?: string | null;
  finishNoLaterThan?: string | null;
  /** Percent complete in [0, 100]; null is unstatused (equivalent to 0 with no actuals). */
  percentComplete?: number | null;
  actualStart?: string | null;
  actualFinish?: string | null;
}

export interface TaskHierarchy {
  jobId: string;
  jobVersion: number;
  tasks: Task[];
  dependencies?: FinishStartDependency[];
}
/** The four dependency relationship types, as canonical two-letter codes. */
export type DependencyType = "FS" | "SS" | "FF" | "SF";
export interface FinishStartDependency { predecessorTaskId: string; successorTaskId: string; dependencyType: DependencyType; lagMinutes: number; }

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
export type TaskConstraintKind = "start_no_earlier_than" | "finish_no_later_than";
export interface UpdateTaskConstraintRequest {
  taskId: string;
  kind: TaskConstraintKind;
  value: string | null;
  expectedVersion: number;
  expectedJobVersion: number;
}
export interface UpdateJobDataDateRequest {
  jobId: string;
  /** null clears the data date; a set value is a canonical ISO date-only string. */
  dataDate: string | null;
  expectedJobVersion: number;
}
export interface UpdateTaskProgressRequest {
  taskId: string;
  /** When true the progress columns are cleared and percent/actuals are ignored. */
  clear: boolean;
  percentComplete: number | null;
  actualStart: string | null;
  actualFinish: string | null;
  expectedVersion: number;
  expectedJobVersion: number;
}
export interface Baseline {
  id: string;
  jobId: string;
  name: string;
  createdAt: string;
  isComparisonDefault: boolean;
}
export interface CreateBaselineRequest { jobId: string; name: string; expectedJobVersion: number; }
export interface SetBaselineComparisonDefaultRequest { jobId: string; baselineId: string; expectedJobVersion: number; }
export interface AddDependencyRequest { jobId: string; predecessorTaskId: string; successorTaskId: string; dependencyType?: DependencyType; lagMinutes: number; expectedJobVersion: number; }
export interface RemoveDependencyRequest { jobId: string; predecessorTaskId: string; successorTaskId: string; dependencyType?: DependencyType; expectedJobVersion: number; }
