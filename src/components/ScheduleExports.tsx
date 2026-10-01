import { useState } from "react";

import {
  createScheduleCsv,
  createScheduleReport,
  saveSchedule,
  safeScheduleFilename,
} from "../scheduleExport";
import type { GanttReadModel } from "../types/gantt";

export function ScheduleExports({ readModel, jobName, disabled = false, workdayDurationMinutes = 480 }: { readModel: GanttReadModel; jobName: string; disabled?: boolean; workdayDurationMinutes?: number }) {
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function exportFile(kind: "csv" | "html") {
    if (disabled || saving) return;
    setSaving(true);
    try {
      const content = kind === "csv" ? createScheduleCsv(readModel, jobName, workdayDurationMinutes) : createScheduleReport(readModel, jobName, workdayDurationMinutes);
      await saveSchedule(content, safeScheduleFilename(jobName, kind), kind);
      setError(null);
    } catch (cause) {
      const detail = typeof cause === "string" ? cause : cause instanceof Error ? cause.message : String(cause);
      setError(`Couldn't save the schedule ${kind.toUpperCase()}: ${detail}`);
    } finally {
      setSaving(false);
    }
  }

  return <section className="schedule-exports" aria-label="Schedule exports">
    <button type="button" disabled={disabled || saving} onClick={() => void exportFile("csv")}>Export CSV</button>
    <button type="button" disabled={disabled || saving} onClick={() => void exportFile("html")}>Download printable schedule</button>
    <p>Exports include all tasks. Open the HTML file to print. Choose a new filename; existing files are kept.</p>
    {error ? <p role="alert">{error}</p> : null}
  </section>;
}
