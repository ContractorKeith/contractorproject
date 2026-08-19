export const GANTT_READ_MODEL_VERSION = 5 as const;

export type GanttTaskKind = "summary" | "task" | "milestone";

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
  rows: GanttRow[];
}
