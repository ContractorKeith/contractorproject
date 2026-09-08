import type { GanttReadModel } from "../types/gantt";
import { formatInstant } from "./format";
import { scheduleSummaryFacts } from "./scheduleFacts";

export interface ScheduleSummaryProps {
  readModel: GanttReadModel;
}

export { scheduleSummaryFacts } from "./scheduleFacts";

/** Compact, read-only answer strip for the job schedule. */
export function ScheduleSummary({ readModel }: ScheduleSummaryProps) {
  const facts = scheduleSummaryFacts(readModel);
  const progressContext = readModel.dataDate
    ? `Progress recorded through ${formatInstant(readModel.dataDate)}`
    : "No progress recorded yet";

  return (
    <section className="schedule-summary" aria-label="Schedule summary">
      <dl className="schedule-summary__facts">
        <div>
          <dt>Forecast finish</dt>
          <dd>{facts.leafCount === 0 ? "No scheduled work" : formatInstant(readModel.scheduleFinish)}</dd>
        </div>
        <div>
          <dt>Progress</dt>
          <dd>{facts.leafCount === 0 ? "No task progress" : `${facts.completedLeafCount} of ${facts.leafCount} complete`}</dd>
        </div>
        <div>
          <dt>Next work</dt>
          <dd>
            {facts.nextUnfinished
              ? <><span className="schedule-summary__next-name" title={facts.nextUnfinished.name}>{facts.nextUnfinished.name}</span><span className="schedule-summary__next-date">{formatInstant(facts.nextUnfinished.start)}</span></>
              : facts.leafCount === 0
                ? "Add scheduled work"
                : "All work complete"}
          </dd>
        </div>
        <div>
          <dt>Needs attention</dt>
          <dd>{facts.attentionCount === 0 ? "None" : `${facts.attentionCount} task${facts.attentionCount === 1 ? "" : "s"}`}</dd>
        </div>
        <div>
          <dt>Critical unfinished</dt>
          <dd>{facts.criticalUnfinishedCount === 0 ? "None" : `${facts.criticalUnfinishedCount} task${facts.criticalUnfinishedCount === 1 ? "" : "s"}`}</dd>
        </div>
      </dl>
      <p className="schedule-summary__context">
        {progressContext}. Critical work may affect the finish; it is not a late-work count.
      </p>
    </section>
  );
}
