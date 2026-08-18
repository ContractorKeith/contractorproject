export const GANTT_READ_MODEL_VERSION = 4 as const;

export type GanttTaskKind = "summary" | "task" | "milestone";

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
  predecessorIds: string[];
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
