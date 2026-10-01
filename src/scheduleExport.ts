import { invoke, isTauri } from "@tauri-apps/api/core";

import type { GanttPredecessorLink, GanttReadModel, GanttRow } from "./types/gantt";
import { formatDayQuantity, formatDays, formatSignedCalendarDays, formatSignedDays } from "./gantt/format";

const CSV_HEADERS = [
  "Job name",
  "WBS",
  "Task",
  "Kind",
  "Start",
  "Finish",
  "Duration (working days)",
  "Percent complete",
  "Status",
  "Critical",
  "Deadline attention",
  "Float (working days)",
  "Predecessors",
  "Baseline start",
  "Baseline finish",
  "Baseline duration (working days)",
  "Baseline start variance (calendar days)",
  "Baseline finish variance (calendar days)",
  "Baseline duration variance (working days)",
];

/** Prefix text that a spreadsheet could otherwise interpret as a formula. */
export function formulaSafeText(value: string): string {
  return /^[\u0000-\u001f\u007f\s]*[=+\-@]/.test(value) ? `'${value}` : value;
}

/** CSV quoting for textual cells. Numeric schedule facts intentionally bypass this. */
export function csvText(value: string): string {
  const safe = formulaSafeText(value);
  return /[",\r\n]/.test(safe) ? `"${safe.replaceAll('"', '""')}"` : safe;
}

export function createScheduleCsv(readModel: GanttReadModel, jobName: string, workdayDurationMinutes = 480): string {
  const names = new Map(readModel.rows.map((row) => [row.taskId, row.name]));
  const records = readModel.rows.map((row) => [
    csvText(jobName),
    csvText(row.wbs),
    csvText(row.name),
    csvText(row.kind),
    csvText(civilDate(row.start)),
    csvText(civilDate(row.finish)),
    row.durationMinutes / workdayDurationMinutes,
    row.percentComplete,
    csvText(row.progressStatus),
    row.critical ? "Yes" : "No",
    csvText(deadlineAttention(row)),
    row.totalFloatMinutes / workdayDurationMinutes,
    csvText(predecessorText(row.predecessors, names, workdayDurationMinutes, true)),
    csvText(row.baseline ? civilDate(row.baseline.start) : ""),
    csvText(row.baseline ? civilDate(row.baseline.finish) : ""),
    row.baseline ? row.baseline.durationMinutes / workdayDurationMinutes : "",
    row.baseline ? row.baseline.startVarianceMinutes / 1_440 : "",
    row.baseline ? row.baseline.finishVarianceMinutes / 1_440 : "",
    row.baseline ? row.baseline.durationVarianceMinutes / workdayDurationMinutes : "",
  ].join(","));
  return `\uFEFF${CSV_HEADERS.join(",")}\r\n${records.join("\r\n")}\r\n`;
}

export function createScheduleReport(readModel: GanttReadModel, jobName: string, workdayDurationMinutes = 480): string {
  const names = new Map(readModel.rows.map((row) => [row.taskId, row.name]));
  const baselineContext = readModel.baselineId ? "Comparison baseline included" : "No comparison baseline selected";
  const dataDate = readModel.dataDate ? civilDate(readModel.dataDate) : "No data date recorded";
  const activities = readModel.rows.filter((row) => !row.summary);
  const completedActivities = activities.filter((row) => row.progressStatus === "completed");
  const forecastFinish = activities.length ? civilDate(readModel.scheduleFinish) : "No forecast yet";
  const rows = readModel.rows.map((row) => reportRow(row, names, workdayDurationMinutes)).join("");
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>${escapeHtml(jobName)} schedule</title>
<style>body{font:14px/1.4 system-ui,sans-serif;color:#171717;margin:24px}h1{margin:0}dl{display:grid;grid-template-columns:max-content 1fr;gap:4px 16px;margin:20px 0}table{border-collapse:collapse;width:100%;font-size:12px;table-layout:fixed}th,td{border:1px solid #bbb;padding:6px;text-align:left;vertical-align:top;overflow-wrap:anywhere;word-break:break-word}th{background:#eee}td.num{text-align:right;font-variant-numeric:tabular-nums}.attention{font-weight:700}@page{size:landscape;margin:12mm}@media print{body{margin:0}thead{display:table-header-group}tr{break-inside:avoid}}</style>
</head><body><h1>${escapeHtml(jobName)}</h1><dl><dt>Forecast finish</dt><dd>${escapeHtml(forecastFinish)}</dd><dt>Progress</dt><dd>${completedActivities.length} of ${activities.length} activities completed</dd><dt>Data date</dt><dd>${escapeHtml(dataDate)}</dd><dt>Baseline</dt><dd>${escapeHtml(baselineContext)}</dd><dt>Schedule rows</dt><dd>${readModel.rows.length}</dd></dl>
<table><thead><tr><th>WBS / task</th><th>Kind</th><th>Start</th><th>Finish</th><th>Duration (working days)</th><th>Progress</th><th>Critical</th><th>Attention</th><th>Float (working days)</th><th>Predecessors</th><th>Baseline / variance</th></tr></thead><tbody>${rows}</tbody></table></body></html>`;
}

export function safeScheduleFilename(jobName: string, extension: "csv" | "html"): string {
  const stem = jobName
    .normalize("NFKD")
    .replace(/[^\w.-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
  return `${stem || "schedule"}-schedule.${extension}`;
}

export function downloadSchedule(content: string, filename: string, mimeType: string): void {
  const url = URL.createObjectURL(new Blob([content], { type: `${mimeType};charset=utf-8` }));
  const link = document.createElement("a");
  let clicked = false;
  try {
    link.href = url;
    link.download = filename;
    link.style.display = "none";
    document.body.append(link);
    link.click();
    clicked = true;
    // WebKit can consume an object URL after click returns. Defer its release one
    // turn while still guaranteeing cleanup after a successful download.
    window.setTimeout(() => URL.revokeObjectURL(url), 1_000);
  } finally {
    link.remove();
    if (!clicked) URL.revokeObjectURL(url);
  }
}

/** Use Tauri's native save dialog on desktop; keep browser downloads for web/dev. */
export async function saveSchedule(
  content: string,
  filename: string,
  kind: "csv" | "html",
): Promise<boolean> {
  if (isTauri()) {
    return invoke<boolean>("save_schedule_export", {
      request: { kind, filename, content },
    });
  }
  downloadSchedule(content, filename, kind === "csv" ? "text/csv" : "text/html");
  return true;
}

function deadlineAttention(row: GanttRow): string {
  if (row.constraintViolated) {
    return !row.summary && row.progressStatus !== "completed"
      ? "Needs attention: deadline constraint violated"
      : "Recorded deadline violation";
  }
  if (!row.summary && row.progressStatus !== "completed" && row.totalFloatMinutes < 0) return "Needs attention: negative float";
  if (row.finishNoLaterThan) return "Deadline monitored";
  return "None";
}

function predecessorText(links: GanttPredecessorLink[], names: Map<string, string>, workdayDurationMinutes: number, exact = false): string {
  return links.map((link) => {
    const lag = link.lagMinutes === 0
      ? "no lag"
      : exact
        ? `${link.lagMinutes > 0 ? "+" : ""}${link.lagMinutes / workdayDurationMinutes} working days`
        : formatSignedDays(link.lagMinutes, workdayDurationMinutes);
    return `${names.get(link.taskId) ?? link.taskId} (${link.dependencyType}, ${lag})`;
  }).join("; ");
}

function reportRow(row: GanttRow, names: Map<string, string>, workdayDurationMinutes: number): string {
  const baseline = row.baseline
    ? `${civilDate(row.baseline.start)} → ${civilDate(row.baseline.finish)}; start ${formatSignedCalendarDays(row.baseline.startVarianceMinutes)}, finish ${formatSignedCalendarDays(row.baseline.finishVarianceMinutes)}, duration ${formatSignedDays(row.baseline.durationVarianceMinutes, workdayDurationMinutes)} variance`
    : "No baseline";
  return `<tr><td>${escapeHtml(`${row.wbs} ${row.name}`)}</td><td>${escapeHtml(row.kind)}</td><td>${escapeHtml(civilDate(row.start))}</td><td>${escapeHtml(civilDate(row.finish))}</td><td class="num">${formatDayQuantity(row.durationMinutes, workdayDurationMinutes)}</td><td>${escapeHtml(`${row.percentComplete}% (${row.progressStatus})`)}</td><td>${row.critical ? "Critical" : ""}</td><td class="attention">${escapeHtml(deadlineAttention(row))}</td><td class="num">${formatDayQuantity(row.totalFloatMinutes, workdayDurationMinutes)}</td><td>${escapeHtml(predecessorText(row.predecessors, names, workdayDurationMinutes))}</td><td>${escapeHtml(baseline)}</td></tr>`;
}

function civilDate(value: string): string {
  return value.slice(0, 10);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]!);
}
