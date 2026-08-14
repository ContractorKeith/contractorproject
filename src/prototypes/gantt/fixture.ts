export type TaskKind = "summary" | "task" | "milestone";

export interface SpikeTaskRow {
  id: string;
  parentId: string | null;
  wbs: string;
  depth: 0 | 1 | 2;
  kind: TaskKind;
  name: string;
  startDay: number;
  durationDays: number;
  baselineStartDay: number;
  baselineDurationDays: number;
  percentComplete: number;
  totalFloatDays: number;
  critical: boolean;
  predecessorIds: string[];
  assignee: string;
  logicalIndex: number;
  positionInSet: number;
  setSize: number;
}

export const SPIKE_EPOCH = "2026-01-05";
export const SPIKE_FIXTURE_ID = "gantt-1000-v1";
export const SPIKE_ROW_COUNT = 1_000;

const trades = [
  "Site work",
  "Concrete",
  "Framing",
  "Envelope",
  "Openings",
  "Mechanical",
  "Electrical",
  "Finishes",
  "Commissioning",
  "Closeout",
] as const;

const crews = ["Crew A", "Crew B", "Crew C", "Trade partner", "Unassigned"] as const;

export function createGanttFixture(): SpikeTaskRow[] {
  const rows: SpikeTaskRow[] = [];
  let priorCriticalLeaf: string | null = null;

  function append(row: Omit<SpikeTaskRow, "logicalIndex">) {
    rows.push({ ...row, logicalIndex: rows.length });
  }

  for (let phaseIndex = 0; phaseIndex < 10; phaseIndex += 1) {
    const phaseNumber = phaseIndex + 1;
    const phaseId = `phase-${phaseNumber}`;
    const phaseStart = phaseIndex * 18;
    append({
      id: phaseId,
      parentId: null,
      wbs: String(phaseNumber),
      depth: 0,
      kind: "summary",
      name: trades[phaseIndex]!,
      startDay: phaseStart,
      durationDays: 46,
      baselineStartDay: phaseStart - ((phaseIndex % 3) - 1),
      baselineDurationDays: 44,
      percentComplete: seededPercent(phaseIndex),
      totalFloatDays: phaseIndex % 4,
      critical: true,
      predecessorIds: priorCriticalLeaf ? [priorCriticalLeaf] : [],
      assignee: "Multiple crews",
      positionInSet: phaseNumber,
      setSize: 10,
    });

    for (let packageIndex = 0; packageIndex < 9; packageIndex += 1) {
      const packageNumber = packageIndex + 1;
      const packageId = `${phaseId}-package-${packageNumber}`;
      const packageStart = phaseStart + packageIndex * 3;
      append({
        id: packageId,
        parentId: phaseId,
        wbs: `${phaseNumber}.${packageNumber}`,
        depth: 1,
        kind: "summary",
        name: `${trades[phaseIndex]!} package ${packageNumber}`,
        startDay: packageStart,
        durationDays: 22,
        baselineStartDay: packageStart + ((packageIndex % 3) - 1),
        baselineDurationDays: 21,
        percentComplete: seededPercent(phaseIndex * 9 + packageIndex),
        totalFloatDays: packageIndex % 5,
        critical: packageIndex === 0,
        predecessorIds: [],
        assignee: crews[(phaseIndex + packageIndex) % crews.length]!,
        positionInSet: packageNumber,
        setSize: 9,
      });

      let previousLeafId: string | null = null;
      let twoBackLeafId: string | null = null;

      for (let leafIndex = 0; leafIndex < 10; leafIndex += 1) {
        const leafNumber = leafIndex + 1;
        const leafId = `${packageId}-task-${leafNumber}`;
        const isMilestone = packageIndex === 8 && leafIndex === 9;
        const isCritical = packageIndex === 0;
        const predecessorIds = [previousLeafId, leafIndex > 2 ? twoBackLeafId : null].filter(
          (value): value is string => value !== null,
        );
        if (isCritical && leafIndex === 0 && priorCriticalLeaf) predecessorIds.push(priorCriticalLeaf);

        const startDay = packageStart + leafIndex * 2;
        const durationDays = isMilestone ? 0 : 2 + ((phaseIndex + packageIndex + leafIndex) % 5);
        append({
          id: leafId,
          parentId: packageId,
          wbs: `${phaseNumber}.${packageNumber}.${leafNumber}`,
          depth: 2,
          kind: isMilestone ? "milestone" : "task",
          name: isMilestone ? `${trades[phaseIndex]!} package complete` : `Task ${leafNumber}`,
          startDay,
          durationDays,
          baselineStartDay: startDay + (((phaseIndex + packageIndex + leafIndex) % 7) - 3),
          baselineDurationDays: isMilestone ? 0 : Math.max(1, durationDays - (leafIndex % 2)),
          percentComplete: seededPercent(phaseIndex * 90 + packageIndex * 10 + leafIndex),
          totalFloatDays: isCritical ? 0 : (leafIndex % 9) - 2,
          critical: isCritical,
          predecessorIds,
          assignee: crews[(packageIndex + leafIndex) % crews.length]!,
          positionInSet: leafNumber,
          setSize: 10,
        });

        twoBackLeafId = previousLeafId;
        previousLeafId = leafId;
        if (isCritical) priorCriticalLeaf = leafId;
      }
    }
  }

  if (rows.length !== SPIKE_ROW_COUNT) {
    throw new Error(`Expected ${SPIKE_ROW_COUNT} rows, received ${rows.length}`);
  }
  return rows;
}

export function dayLabel(dayOffset: number): string {
  const date = new Date(`${SPIKE_EPOCH}T12:00:00.000Z`);
  date.setUTCDate(date.getUTCDate() + dayOffset);
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(date);
}

export function finishDay(row: SpikeTaskRow): number {
  return row.startDay + Math.max(row.durationDays - 1, 0);
}

function seededPercent(seed: number): number {
  return (seed * 37 + 19) % 101;
}
