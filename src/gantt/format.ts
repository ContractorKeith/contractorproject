// Shared Gantt fact formatters. The treegrid cells and the explanation panel
// render schedule instants and signed minutes in lockstep; that identical format
// is a documented contract, so both surfaces import these helpers rather than
// keeping their own copies.

/** A schedule instant as `YYYY-MM-DD HH:mm` (drops seconds and the `T`). */
export function formatInstant(value: string): string {
  return value.slice(0, 16).replace("T", " ");
}

/** Signed working minutes, e.g. `+120 min`, `-480 min`, or `0 min`. */
export function formatSignedMinutes(value: number): string {
  if (value === 0) return "0 min";
  return `${value > 0 ? "+" : ""}${value.toLocaleString("en-US")} min`;
}
