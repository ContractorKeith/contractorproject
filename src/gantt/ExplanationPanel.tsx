import type {
  GanttDependencyType,
  GanttLateFinishLimit,
  GanttRow,
  GanttScheduleDriver,
  GanttTaskExplanation,
} from "../types/gantt";
import { formatInstant, formatSignedDays } from "./format";
import "./explanationPanel.css";

export interface ExplanationPanelProps {
  /** The focused schedule row, or null when nothing is focused. */
  row: GanttRow | null;
  /** Resolve relationship identities to names in the contractor workspace. */
  taskNames?: ReadonlyMap<string, string>;
  /** Job calendar workday length in minutes. Defaults for older fixtures. */
  workdayDurationMinutes?: number;
}

/**
 * Non-modal, hairline-framed region that surfaces the focused task's Rust-derived
 * schedule explanation as deterministic FACT rows (DESIGN.md §6). It renders the
 * scheduler's typed facts verbatim and never does schedule math. The model-prose
 * treatment stays reserved for the future AI assistant layer.
 */
export function ExplanationPanel({ row, taskNames, workdayDurationMinutes = 480 }: ExplanationPanelProps) {
  const label = row ? `Schedule explanation for ${row.name}` : "Schedule explanation";
  return (
    <section className="explanation-panel" role="region" aria-label={label}>
      <h2 className="explanation-panel__title">
        <span className="explanation-panel__title-label">Schedule explanation</span>
        {row ? (
          <span className="explanation-panel__title-task">
            {row.name}{taskNames ? "" : ` · ${row.taskId}`}
          </span>
        ) : null}
      </h2>
      <div className="explanation-panel__facts">
        {row ? (
          explanationFactLines(row.explanation, taskNames, workdayDurationMinutes).map((line, index) => (
            <p className="explanation-panel__fact" key={index}>
              {line}
            </p>
          ))
        ) : (
          <p className="explanation-panel__fact explanation-panel__fact--empty">
            Focus a task to see what drives it.
          </p>
        )}
      </div>
    </section>
  );
}

/** The ordered deterministic fact lines for one explanation. */
function explanationFactLines(explanation: GanttTaskExplanation, names: ReadonlyMap<string, string> | undefined, workdayDurationMinutes: number): string[] {
  switch (explanation.kind) {
    case "summary":
      return ["Derived from children"];
    case "complete":
      return [
        `Complete · actual ${formatInstant(explanation.actualStart)} – ${formatInstant(
          explanation.actualFinish,
        )}`,
      ];
    case "scheduled": {
      const lines = [`Driver ${driverText(explanation.primaryDriver, names, workdayDurationMinutes)}`];
      for (const driver of explanation.otherBindingDrivers) {
        lines.push(`Also ${driverText(driver, names, workdayDurationMinutes)}`);
      }
      if (explanation.startedActualStart) {
        lines.push(`Started ${formatInstant(explanation.startedActualStart)}`);
      }
      if (explanation.calendarGap) {
        const { nonWorkingDayCount, fromDate, toDate } = explanation.calendarGap;
        const noun = nonWorkingDayCount === 1 ? "non-working day" : "non-working days";
        lines.push(`${nonWorkingDayCount} ${noun} between ${fromDate} and ${toDate}`);
      }
      lines.push(floatText(explanation.totalFloatMinutes, explanation.critical, workdayDurationMinutes));
      lines.push(limitText(explanation.lateFinishLimit, names, workdayDurationMinutes));
      return lines;
    }
  }
}

/** Driver rendering for a `Driver`/`Also` fact line. */
function driverText(driver: GanttScheduleDriver, names: ReadonlyMap<string, string> | undefined, workdayDurationMinutes: number): string {
  switch (driver.kind) {
    case "scheduleStart":
      return "Starts at schedule start";
    case "startConstraint":
      return `Start no earlier than ${driver.date}${appliedFragment(
        driver.date,
        driver.normalizedDate,
      )}`;
    case "dataDate":
      return `Pushed to data date ${formatInstant(driver.date)}`;
    case "predecessor":
      // FS/SS bind the successor start; FF/SF bind its remaining-work finish.
      return `${finishAnchored(driver.dependencyType) ? "Finish after" : "After"} ${linkText(
        names?.get(driver.taskId) ?? driver.taskId,
        driver.dependencyType,
        driver.lagMinutes,
        workdayDurationMinutes,
      )}`;
  }
}

/** Float rationale line, e.g. `Float +1 day` or `Float -1 day · critical`. */
function floatText(totalFloatMinutes: number, critical: boolean, workdayDurationMinutes: number): string {
  const base = `Float ${formatSignedDays(totalFloatMinutes, workdayDurationMinutes)}`;
  return critical ? `${base} · critical` : base;
}

/** Late-finish limit fact line. */
function limitText(limit: GanttLateFinishLimit, names: ReadonlyMap<string, string> | undefined, workdayDurationMinutes: number): string {
  switch (limit.kind) {
    case "deadline":
      return `Finish limited by deadline ${limit.date}${appliedFragment(
        limit.date,
        limit.normalizedDate,
      )}`;
    case "successor":
      return `Finish limited by successor ${linkText(
        names?.get(limit.taskId) ?? limit.taskId,
        limit.dependencyType,
        limit.lagMinutes,
        workdayDurationMinutes,
      )}`;
    case "projectFinish":
      return "Finish limited by project finish";
  }
}

// The ` · applied <date>` fragment, shown only when normalization moved the date.
function appliedFragment(date: string, normalizedDate: string): string {
  return normalizedDate !== date ? ` · applied ${normalizedDate}` : "";
}

// FF/SF links anchor on a finish; FS/SS anchor on a start.
function finishAnchored(type: GanttDependencyType): boolean {
  return type === "FF" || type === "SF";
}

// Typed-link fragment mirroring the Predecessors-cell format, e.g. `B FS +1 day`.
// The lag fragment is dropped at zero (`B FS`).
function linkText(taskId: string, type: GanttDependencyType, lagMinutes: number, workdayDurationMinutes: number): string {
  if (lagMinutes === 0) return `${taskId} ${type}`;
  return `${taskId} ${type} ${formatSignedDays(lagMinutes, workdayDurationMinutes)}`;
}
