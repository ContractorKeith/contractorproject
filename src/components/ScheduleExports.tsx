import { useState } from "react";

import {
  createScheduleCsv,
  createScheduleReport,
  downloadSchedule,
  safeScheduleFilename,
} from "../scheduleExport";
import type { GanttReadModel } from "../types/gantt";

export function ScheduleExports({ readModel, jobName, disabled = false }: { readModel: GanttReadModel; jobName: string; disabled?: boolean }) {
  const [error, setError] = useState<string | null>(null);

  function exportFile(kind: "csv" | "html") {
    if (disabled) return;
    try {
      const content = kind === "csv" ? createScheduleCsv(readModel, jobName) : createScheduleReport(readModel, jobName);
      downloadSchedule(content, safeScheduleFilename(jobName, kind), kind === "csv" ? "text/csv" : "text/html");
      setError(null);
    } catch {
      setError(`Couldn't download the schedule ${kind.toUpperCase()}. Try again.`);
    }
  }

  return <section className="schedule-exports" aria-label="Schedule exports">
    <button type="button" disabled={disabled} onClick={() => exportFile("csv")}>Export CSV</button>
    <button type="button" disabled={disabled} onClick={() => exportFile("html")}>Download printable schedule</button>
    <p>Exports include all tasks. Open the HTML file to print.</p>
    {error ? <p role="alert">{error}</p> : null}
  </section>;
}
