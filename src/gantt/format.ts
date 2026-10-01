// Shared Gantt date and working-day formatters for the treegrid and explanation panel.

/** Contractor-facing civil date; schedule clock times stay internal. */
export function formatInstant(value: string): string {
  return value.slice(0, 10);
}

/** Working-day quantity with enough precision to keep minute-sized facts visible. */
export function formatDays(minutes: number, workdayDurationMinutes = 480): string {
  const days = minutes / workdayDurationMinutes;
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 3 }).format(days);
}

export function formatDayQuantity(minutes: number, workdayDurationMinutes = 480): string {
  const days = minutes / workdayDurationMinutes;
  return `${formatDays(minutes, workdayDurationMinutes)} ${Math.abs(days) === 1 ? "day" : "days"}`;
}

/** Signed working-day fact, e.g. `+0.25 days`, `-1 day`, or `0 days`. */
export function formatSignedDays(minutes: number, workdayDurationMinutes = 480): string {
  const days = minutes / workdayDurationMinutes;
  const value = formatDays(Math.abs(minutes), workdayDurationMinutes);
  return `${days > 0 ? "+" : days < 0 ? "-" : ""}${value} ${Math.abs(days) === 1 ? "day" : "days"}`;
}

/** Baseline start/finish deltas are elapsed calendar time, not working time. */
export function formatSignedCalendarDays(minutes: number): string {
  const days = minutes / 1_440;
  const value = new Intl.NumberFormat("en-US", { maximumFractionDigits: 3 }).format(Math.abs(days));
  return `${days > 0 ? "+" : days < 0 ? "-" : ""}${value} ${Math.abs(days) === 1 ? "calendar day" : "calendar days"}`;
}
