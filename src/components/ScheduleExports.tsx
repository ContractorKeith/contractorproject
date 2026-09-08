import { useState } from "react";

import {
  createScheduleCsv,
  createScheduleReport,
  downloadSchedule,
  safeScheduleFilename,
} from "../scheduleExport";
import type { GanttReadModel } from "../types/gantt";

export function ScheduleExports({ readModel, jobName }: { readModel: GanttReadModel; jobName: string }) {
  const [error, setError] = useState<string | null>(null);

  function exportFile(kind: "csv" | "html") {
    try {
      const content = kind === "csv" ? createScheduleCsv(readModel, jobName) : createScheduleReport(readModel, jobName);
      downloadSchedule(content, safeScheduleFilename(jobName, kind), kind === "csv" ? "text/csv" : "text/html");
      setError(null);
    } catch {
      setError(`Couldn't download the schedule ${kind.toUpperCase()}. Try again.`);
    }
  }

  return <section aria-label="Schedule exports">
    <button type="button" onClick={() => exportFile("csv")}>Export CSV</button>
    <button type="button" onClick={() => exportFile("html")}>Download printable schedule</button>
    <p>Open the downloaded schedule, then use your browser&apos;s Print command.</p>
    {error ? <p role="alert">{error}</p> : null}
  </section>;
}
