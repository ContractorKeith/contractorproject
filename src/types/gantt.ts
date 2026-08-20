export const GANTT_READ_MODEL_VERSION = 7 as const;

export type GanttTaskKind = "summary" | "task" | "milestone";

/** Weekly working day, snake-case to mirror the Rust `CalendarWeekday` codes. */
export type GanttCalendarWeekday =
  | "monday"
  | "tuesday"
  | "wednesday"
  | "thursday"
  | "friday"
  | "saturday"
  | "sunday";

/**
 * Rendering-only working-calendar facts (contract v6). `workingWeekdays` is the
 * weekly working-day set in canonical Monday-to-Sunday order; `exceptionDates`
 * are the job's dated non-working civil days (`YYYY-MM-DD`), sorted and unique.
 * The timeline paints non-working shading from these facts and derives no
 * schedule math from them.
 */
export interface GanttCalendar {
  workingWeekdays: GanttCalendarWeekday[];
  exceptionDates: string[];
}

/** The four dependency relationship types, as their canonical two-letter codes. */
export type GanttDependencyType = "FS" | "SS" | "FF" | "SF";

/** A typed predecessor link on a successor row. `taskId` is the predecessor task. */
export interface GanttPredecessorLink {
  taskId: string;
  dependencyType: GanttDependencyType;
  /** Signed working-minute lag. */
  lagMinutes: number;
}

/** Rust-derived progress status. React renders this fact and never derives it. */
export type GanttProgressStatus = "completed" | "inProgress" | "notStarted";

export interface GanttBaselineComparison {
  start: string;
  finish: string;
  durationMinutes: number;
  startVarianceMinutes: number;
  finishVarianceMinutes: number;
  /** Signed current-minus-baseline duration difference in minutes. */
  durationVarianceMinutes: number;
}

/**
 * One forward-pass lower bound on a scheduled leaf's remaining work. Kind-tagged
 * to mirror the Rust `ScheduleDriver` serde output exactly (contract v7).
 */
export type GanttScheduleDriver =
  | { kind: "scheduleStart" }
  | { kind: "startConstraint"; date: string; normalizedDate: string }
  | { kind: "dataDate"; date: string }
  | {
      kind: "predecessor";
      taskId: string;
      dependencyType: GanttDependencyType;
      lagMinutes: number;
    };

/**
 * A run of non-working civil days between a driver's reference date and the
 * leaf's arrival date. Endpoints ascend; the count covers `[fromDate, toDate)`.
 * Rust emits this as a plain (untagged) struct.
 */
export interface GanttCalendarGap {
  fromDate: string;
  toDate: string;
  nonWorkingDayCount: number;
}

/** What bounded a scheduled leaf's late finish (one level only), kind-tagged. */
export type GanttLateFinishLimit =
  | { kind: "deadline"; date: string; normalizedDate: string }
  | {
      kind: "successor";
      taskId: string;
      dependencyType: GanttDependencyType;
      lagMinutes: number;
    }
  | { kind: "projectFinish" };

/**
 * Deterministic, typed explanation of why one task starts and finishes when it
 * does (contract v7). Kind-tagged to mirror the Rust `TaskExplanation` serde
 * output exactly. React renders these facts and never derives them.
 */
export type GanttTaskExplanation =
  | { kind: "summary"; taskId: string }
  | { kind: "complete"; taskId: string; actualStart: string; actualFinish: string }
  | {
      kind: "scheduled";
      taskId: string;
      primaryDriver: GanttScheduleDriver;
      otherBindingDrivers: GanttScheduleDriver[];
      startedActualStart: string | null;
      calendarGap: GanttCalendarGap | null;
      totalFloatMinutes: number;
      critical: boolean;
      lateFinishLimit: GanttLateFinishLimit;
    };

export interface GanttRow {
  taskId: string;
  parentTaskId: string | null;
  logicalIndex: number;
  depth: number;
  positionInSet: number;
  setSize: number;
  sortKey: number;
  wbs: string;
  name: string;
  kind: GanttTaskKind;
  hasChildren: boolean;
  durationMinutes: number;
  start: string;
  finish: string;
  totalFloatMinutes: number;
  startNoEarlierThan: string | null;
  finishNoLaterThan: string | null;
  constraintViolated: boolean;
  critical: boolean;
  milestone: boolean;
  summary: boolean;
  /** Percent complete: leaf canonical value, summary duration-weighted rollup. */
  percentComplete: number;
  /** Normalized actual start/finish instants; null when absent (always null on summaries). */
  actualStart: string | null;
  actualFinish: string | null;
  progressStatus: GanttProgressStatus;
  /** Typed predecessor links, sorted by predecessor id then dependency type. */
  predecessors: GanttPredecessorLink[];
  baseline: GanttBaselineComparison | null;
  /** Deterministic, typed explanation of why this task is placed (contract v7). */
  explanation: GanttTaskExplanation;
}

export interface GanttReadModel {
  contractVersion: typeof GANTT_READ_MODEL_VERSION;
  jobId: string;
  jobVersion: number;
  scheduleStart: string;
  scheduleFinish: string;
  /** Normalized job-local data-date instant, or null when the job is unstatused. */
  dataDate: string | null;
  baselineId: string | null;
  rowCount: number;
  criticalTaskIds: string[];
  criticalPath: string[];
  /** Rendering-only working-calendar facts for non-working-time shading. */
  calendar: GanttCalendar;
  rows: GanttRow[];
}
