/** Convert between the pilot's working-day inputs and the scheduler's minute API. */
export function formatWorkingDays(minutes: number | null | undefined, workdayMinutes: number): string {
  return minutes == null ? "" : String(minutes / workdayMinutes);
}

/** Undefined is invalid; null keeps the deliberate unset-duration state. */
export function parseWorkingDays(value: string, workdayMinutes: number): number | null | undefined {
  if (value.trim() === "") return null;
  const days = Number(value);
  const minutes = days * workdayMinutes;
  const nearestMinute = Math.round(minutes);
  // Allow only floating-point residue when an existing integer minute value is
  // displayed as days and parsed again. Real fractional minutes cannot be stored.
  const tolerance = Number.EPSILON * Math.max(1, Math.abs(minutes)) * 8;
  if (!Number.isFinite(minutes) || minutes < 0 || !Number.isSafeInteger(nearestMinute) || Math.abs(minutes - nearestMinute) > tolerance) {
    return undefined;
  }
  return nearestMinute;
}

export function parseSignedWorkingDayLag(value: string, workdayMinutes: number): number | undefined {
  if (value.trim() === "") return 0;
  const days = Number(value);
  const minutes = days * workdayMinutes;
  const nearestMinute = Math.round(minutes);
  const tolerance = Number.EPSILON * Math.max(1, Math.abs(minutes)) * 8;
  if (!Number.isFinite(minutes) || !Number.isSafeInteger(nearestMinute) || Math.abs(minutes - nearestMinute) > tolerance || Math.abs(nearestMinute) > 10_000_000) {
    return undefined;
  }
  return nearestMinute;
}
