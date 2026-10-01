import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { tauriJobClient, type JobClient } from "./api/jobs";
import { BrandMark } from "./components/BrandMark";
import { ScheduleExports } from "./components/ScheduleExports";
import { ExplanationPanel } from "./gantt/ExplanationPanel";
import { formatWorkingDays, parseSignedWorkingDayLag, parseWorkingDays } from "./workingDays";
import { formatSignedDays } from "./gantt/format";
import { GanttTreegrid } from "./gantt/GanttTreegrid";
import { ScheduleSummary } from "./gantt/ScheduleSummary";
import { SettingsDialog } from "./components/SettingsDialog";
import { ThemeToggle } from "./components/ThemeToggle";
import { loadThemePreference, watchTheme, type ThemePreference } from "./theme";
import type {
  Baseline,
  CalendarWeekday,
  DependencyType,
  Job,
  Task,
  TaskConstraintKind,
  TaskHierarchy,
  WorkingCalendar,
} from "./types/jobs";
import type { GanttReadModel } from "./types/gantt";

interface AppProps {
  client?: JobClient;
}

type TaskLoadState =
  | { status: "loading" }
  | { status: "loaded"; hierarchy: TaskHierarchy }
  | { status: "error"; message: string };

type ScheduleLoadState =
  | { status: "loading" }
  | { status: "loaded"; readModel: GanttReadModel }
  | { status: "setup"; message: string }
  | { status: "error"; message: string };

export function App({ client = tauriJobClient }: AppProps) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [archivedJobs, setArchivedJobs] = useState<Job[]>([]);
  const [name, setName] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [refreshingJobs, setRefreshingJobs] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const [archiveConflict, setArchiveConflict] = useState(false);
  const [pendingJobId, setPendingJobId] = useState<string | null>(null);
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [archivedLoading, setArchivedLoading] = useState(false);
  const [openJobId, setOpenJobId] = useState<string | null>(null);
  const [taskLoads, setTaskLoads] = useState<Record<string, TaskLoadState>>({});
  const [workspaceDirty, setWorkspaceDirty] = useState(false);
  const [workspacePending, setWorkspacePending] = useState(false);
  const [theme, setTheme] = useState<ThemePreference>(loadThemePreference);
  const [backupPending, setBackupPending] = useState(false);
  const [backupResult, setBackupResult] = useState<"cancelled" | { destination: string; createdAtUtc: string; byteSize: number } | null>(null);
  const [backupError, setBackupError] = useState<string | null>(null);
  const backupButtonRef = useRef<HTMLButtonElement>(null);
  const handleWorkspaceStateChange = useCallback(({ dirty, pending }: { dirty: boolean; pending: boolean }) => {
    setWorkspaceDirty(dirty);
    setWorkspacePending(pending);
  }, []);

  useEffect(
    () =>
      watchTheme(theme, (resolvedTheme) => {
        document.documentElement.dataset.theme = resolvedTheme;
      }),
    [theme],
  );

  useEffect(() => {
    // Wait for React to re-enable the button before restoring native dialog focus.
    if (!backupPending && (backupResult || backupError)) backupButtonRef.current?.focus();
  }, [backupPending, backupResult, backupError]);

  useEffect(() => {
    let active = true;
    client
      .listJobs()
      .then((loadedJobs) => {
        if (active) setJobs(loadedJobs);
      })
      .catch((reason: unknown) => {
        if (active) setError(errorMessage(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [client]);

  async function handleCreateJob(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || creating) return;

    if (workspacePending) {
      setCreateError("Finish saving the selected task before creating another job.");
      return;
    }
    if (workspaceDirty && !window.confirm("Discard the unsaved changes to this task?")) return;

    setCreating(true);
    setCreateError(null);
    try {
      const job = await client.createJob({
        name: name.trim(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
      });
      setJobs((current) => [job, ...current]);
      setName("");
      setSettingsOpen(false);
      // The just-created record is authoritative even before the list query has
      // observed it. Load its hierarchy directly so the first workspace opens.
      setOpenJobId(job.id);
      setTaskLoads((current) => ({ ...current, [job.id]: { status: "loading" } }));
      void client.listTasks(job.id)
        .then((hierarchy) => setTaskLoads((current) => ({ ...current, [job.id]: { status: "loaded", hierarchy } })))
        .catch((reason: unknown) => setTaskLoads((current) => ({ ...current, [job.id]: { status: "error", message: errorMessage(reason) } })));
    } catch (reason: unknown) {
      setCreateError(errorMessage(reason));
    } finally {
      setCreating(false);
    }
  }

  async function handleCreateVerifiedBackup() {
    if (!client.createVerifiedBackup || backupPending) return;
    setBackupPending(true);
    setBackupResult(null);
    setBackupError(null);
    try {
      const result = await client.createVerifiedBackup();
      if (result === null) {
        setBackupResult("cancelled");
      } else {
        setBackupResult({
          destination: result.destination,
          createdAtUtc: result.createdAtUtc,
          byteSize: result.byteSize,
        });
      }
    } catch (reason: unknown) {
      setBackupError(errorMessage(reason));
    } finally {
      setBackupPending(false);
    }
  }

  async function loadArchivedJobs() {
    setArchivedLoading(true);
    setArchiveError(null);
    try {
      setArchivedJobs(await client.listJobs("archived"));
    } catch (reason: unknown) {
      setArchiveError(errorMessage(reason));
    } finally {
      setArchivedLoading(false);
    }
  }

  function handleToggleArchivedJobs() {
    const nextOpen = !archivedOpen;
    setArchivedOpen(nextOpen);
    if (nextOpen) void loadArchivedJobs();
  }

  async function refreshJobs() {
    setArchiveConflict(false);
    setArchiveError(null);
    setRefreshingJobs(true);
    try {
      const refreshedJobs = await client.listJobs();
      setJobs(refreshedJobs);
      if (openJobId) {
        const snapshot = await loadJobSnapshot(client, openJobId);
        setJobs((current) => current.map((job) => job.id === snapshot.job.id ? snapshot.job : job));
        setTaskLoads((current) => ({
          ...current,
          [openJobId]: { status: "loaded", hierarchy: snapshot.hierarchy },
        }));
      }
      if (archivedOpen) await loadArchivedJobs();
    } catch (reason: unknown) {
      setArchiveError(errorMessage(reason));
    } finally {
      setRefreshingJobs(false);
    }
  }

  async function handleArchive(job: Job) {
    if (!client.archiveJob || pendingJobId) return;
    if (openJobId === job.id && !confirmWorkspaceLeave()) return;
    setPendingJobId(job.id);
    setArchiveError(null);
    setArchiveConflict(false);
    try {
      await client.archiveJob({ jobId: job.id, expectedJobVersion: job.version });
      setJobs((current) => current.filter((candidate) => candidate.id !== job.id));
      setTaskLoads((current) => {
        const next = { ...current };
        delete next[job.id];
        return next;
      });
      if (openJobId === job.id) setOpenJobId(null);
      if (archivedOpen) await loadArchivedJobs();
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) setArchiveConflict(true);
      else setArchiveError(errorMessage(reason));
    } finally {
      setPendingJobId(null);
    }
  }

  async function handleRestore(job: Job) {
    if (!client.restoreJob || pendingJobId) return;
    setPendingJobId(job.id);
    setArchiveError(null);
    setArchiveConflict(false);
    try {
      const restored = await client.restoreJob({ jobId: job.id, expectedJobVersion: job.version });
      setArchivedJobs((current) => current.filter((candidate) => candidate.id !== job.id));
      setJobs((current) => [restored, ...current]);
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) setArchiveConflict(true);
      else setArchiveError(errorMessage(reason));
    } finally {
      setPendingJobId(null);
    }
  }

  async function handleToggleTasks(job: Job, retry = false) {
    if (openJobId === job.id && !retry) {
      if (!confirmWorkspaceLeave()) return;
      setOpenJobId(null);
      return;
    }

    if (openJobId && openJobId !== job.id && !confirmWorkspaceLeave()) return;

    setOpenJobId(job.id);
    if (taskLoads[job.id]?.status === "loaded" || (!retry && taskLoads[job.id]?.status === "loading")) return;

    setTaskLoads((current) => ({
      ...current,
      [job.id]: { status: "loading" },
    }));
    try {
      const snapshot = await loadJobSnapshot(client, job.id);
      handleJobChange(snapshot.job);
      setTaskLoads((current) => ({
        ...current,
        [job.id]: { status: "loaded", hierarchy: snapshot.hierarchy },
      }));
    } catch (reason: unknown) {
      setTaskLoads((current) => ({
        ...current,
        [job.id]: { status: "error", message: errorMessage(reason) },
      }));
    }
  }

  function confirmWorkspaceLeave(): boolean {
    if (workspacePending) {
      setError("Finish saving the selected task before changing jobs.");
      return false;
    }
    if (workspaceDirty && !window.confirm("Discard the unsaved changes to this task?")) return false;
    setWorkspaceDirty(false);
    return true;
  }

  function handleHierarchyChange(jobId: string, hierarchy: TaskHierarchy) {
    setTaskLoads((current) => ({
      ...current,
      [jobId]: { status: "loaded", hierarchy },
    }));
  }

  function handleJobChange(updatedJob: Job) {
    setJobs((current) =>
      current.map((job) => (job.id === updatedJob.id ? updatedJob : job)),
    );
    setTaskLoads((current) => {
      const load = current[updatedJob.id];
      if (load?.status !== "loaded") return current;
      return {
        ...current,
        [updatedJob.id]: {
          status: "loaded",
          hierarchy: { ...load.hierarchy, jobVersion: updatedJob.version },
        },
      };
    });
  }

  return (
    <div className={`app-shell${openJobId ? " app-shell--job-open" : ""}`}>
      <header className="app-header">
        <a className="brand" href="#main" aria-label="ContractorProject home">
          <BrandMark />
          <span className="brand__name">
            Contractor<span>Project</span>
          </span>
        </a>
        <div className="header-controls">
          <ThemeToggle theme={theme} onChange={setTheme} />
          <button
            className="settings-trigger"
            type="button"
            aria-label="Open settings"
            title="Settings"
            aria-haspopup="dialog"
            onClick={() => { setCreateError(null); setSettingsOpen(true); }}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <circle cx="12" cy="12" r="3" />
              <path d="M9.5 2.5h5l.5 2.2 1.6.9 2.1-.7 2.5 4.3-1.7 1.5v2.6l1.7 1.5-2.5 4.3-2.1-.7-1.6.9-.5 2.2h-5L9 19.3l-1.6-.9-2.1.7-2.5-4.3 1.7-1.5v-2.6L2.8 9.2l2.5-4.3 2.1.7L9 4.7Z" />
            </svg>
          </button>
        </div>
      </header>

      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        jobName={name}
        onJobNameChange={setName}
        onCreateJob={handleCreateJob}
        creating={creating}
        createError={createError}
        canCreateBackup={Boolean(client.createVerifiedBackup)}
        backupPending={backupPending}
        backupResult={backupResult}
        backupError={backupError}
        onCreateBackup={() => void handleCreateVerifiedBackup()}
        backupButtonRef={backupButtonRef}
      />

      <main id="main" className="workspace">
        {error ? (
          <div className="inline-error" role="alert">
            <strong>Couldn&apos;t update local job data.</strong>
            <span>{error}</span>
          </div>
        ) : null}

        {archiveConflict ? (
          <div className="inline-error" role="alert">
            <strong>Job changed elsewhere.</strong>
            <span>Refresh before archiving or restoring so newer local work is not overwritten.</span>
            <button type="button" onClick={() => void refreshJobs()} disabled={refreshingJobs}>
              Refresh jobs
            </button>
          </div>
        ) : archiveError ? (
          <div className="inline-error" role="alert">
            <strong>Couldn&apos;t update job archive.</strong>
            <span>{archiveError}</span>
          </div>
        ) : null}

        <section className="job-section" aria-label="Saved jobs">
          <div className="section-rule">
            <h1 id="jobs-heading">Local jobs</h1>
            <span>{jobs.length}</span>
          </div>

          {loading ? (
            <p className="loading-state" aria-live="polite">
              Loading local jobs…
            </p>
          ) : jobs.length === 0 ? (
            <div className="empty-state">
              <span className="registration-mark" aria-hidden="true" />
              <p className="eyebrow">Ready when you are</p>
              <h2>No jobs yet</h2>
              <p>
                Create your first job in Settings. It will be stored in this app&apos;s
                local database.
              </p>
            </div>
          ) : (
            <div className="job-list">
              {jobs.map((job, index) => (
                <article className="job-card" key={job.id}>
                  <div className="job-card__number" aria-hidden="true">
                    {String(index + 1).padStart(2, "0")}
                  </div>
                  <div className="job-card__content">
                    <div className="job-card__meta"><span className="status-tag">{job.status}</span></div>
                    <h2>{job.name}</h2>
                    <button
                      className="task-disclosure"
                      type="button"
                      aria-label={openJobId === job.id ? `Back to jobs for ${job.name}` : `Open schedule for ${job.name}`}
                      aria-expanded={openJobId === job.id}
                      aria-controls={`task-panel-${job.id}`}
                      onClick={() => void handleToggleTasks(job)}
                    >
                      {openJobId === job.id ? "Back to jobs" : "Open schedule"}
                    </button>
                    <button
                      className="archive-action"
                      type="button"
                      onClick={() => void handleArchive(job)}
                      disabled={!client.archiveJob || pendingJobId !== null}
                    >
                      {pendingJobId === job.id ? "Archiving…" : "Archive job"}
                    </button>
                    {openJobId === job.id ? (
                      <TaskPanel
                        id={`task-panel-${job.id}`}
                        job={job}
                        state={taskLoads[job.id] ?? { status: "loading" }}
                        client={client}
                        onHierarchyChange={(hierarchy) =>
                          handleHierarchyChange(job.id, hierarchy)
                        }
                        onJobChange={handleJobChange}
                        onWorkspaceStateChange={handleWorkspaceStateChange}
                        onRetry={() => void handleToggleTasks(job, true)}
                      />
                    ) : null}
                  </div>
                  <span className="job-card__local">Local</span>
                </article>
              ))}
            </div>
          )}
        </section>

        <section className="archived-section" aria-labelledby="archived-jobs-heading">
          <button
            className="archived-disclosure"
            type="button"
            id="archived-jobs-heading"
            aria-expanded={archivedOpen}
            aria-controls="archived-jobs-panel"
            onClick={handleToggleArchivedJobs}
          >
            {archivedOpen ? "Hide archived jobs" : "View archived jobs"}
          </button>
          {archivedOpen ? (
            <div id="archived-jobs-panel" className="archived-panel" aria-live="polite">
              {archivedLoading ? <p>Loading archived jobs…</p> : archivedJobs.length === 0 ? <p>No archived jobs.</p> : (
                <>
                  <p className="archived-count">{archivedJobs.length} archived {archivedJobs.length === 1 ? "job" : "jobs"}</p>
                  <ul className="archived-list" aria-label="Archived jobs">
                    {archivedJobs.map((job) => (
                      <li key={job.id}>
                        <span>{job.name}</span>
                        <button type="button" onClick={() => void handleRestore(job)} disabled={!client.restoreJob || pendingJobId !== null}>
                          {pendingJobId === job.id ? "Restoring…" : `Restore ${job.name}`}
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          ) : null}
        </section>
      </main>
    </div>
  );
}

function TaskPanel({
  id,
  job,
  state,
  client,
  onHierarchyChange,
  onJobChange,
  onWorkspaceStateChange,
  onRetry,
}: {
  id: string;
  job: Job;
  state: TaskLoadState;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
  onWorkspaceStateChange: (state: { dirty: boolean; pending: boolean }) => void;
  onRetry: () => void;
}) {
  const [schedule, setSchedule] = useState<ScheduleLoadState>({ status: "loading" });
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedTaskDirty, setSelectedTaskDirty] = useState(false);
  const [selectedTaskPending, setSelectedTaskPending] = useState(false);
  const [pendingSelectionTaskId, setPendingSelectionTaskId] = useState<string | null>(null);
  const [removedDraftNotice, setRemovedDraftNotice] = useState<string | null>(null);
  const [scheduleReload, setScheduleReload] = useState(0);
  const lastSelectedHierarchyRef = useRef<TaskHierarchy | null>(null);

  useEffect(() => {
    if (state.status === "loaded" && selectedTaskId === null && state.hierarchy.tasks[0]) {
      setSelectedTaskId(state.hierarchy.tasks[0].id);
    }
  }, [selectedTaskId, state]);

  useEffect(() => {
    if (state.status !== "loaded") return;
    const selectedRemoved = selectedTaskId !== null && !state.hierarchy.tasks.some((task) => task.id === selectedTaskId);
    if (selectedTaskDirty && selectedRemoved) {
      const name = lastSelectedHierarchyRef.current?.tasks.find((task) => task.id === selectedTaskId)?.name ?? "selected task";
      setRemovedDraftNotice(`“${name}” was removed elsewhere. Your unsaved draft is still open until you discard it.`);
    } else {
      setRemovedDraftNotice(null);
    }
  }, [selectedTaskDirty, selectedTaskId, state]);

  useEffect(() => {
    onWorkspaceStateChange({ dirty: selectedTaskDirty, pending: selectedTaskPending });
    return () => onWorkspaceStateChange({ dirty: false, pending: false });
  }, [onWorkspaceStateChange, selectedTaskDirty, selectedTaskPending]);

  useEffect(() => {
    if (state.status !== "loaded") return;
    if (!client.getSchedule) return;
    let active = true;
    // Keep the current projection mounted while a hierarchy/version refresh asks
    // Rust for the next one, so its active row and timeline position do not jump.
    setSchedule((current) => current.status === "loaded" ? current : { status: "loading" });
    client.getSchedule(job.id)
      .then((readModel) => active && setSchedule({ status: "loaded", readModel }))
      .catch((reason: unknown) => active && setSchedule(scheduleLoadState(reason)));
    return () => { active = false; };
  }, [client, hierarchyVersion(state), job.id, job.version, scheduleReload, state.status]);
  if (state.status === "loading") {
    return (
      <div id={id} className="task-panel">
        <p aria-live="polite">Loading tasks…</p>
      </div>
    );
  }
  if (state.status === "error") {
    return (
      <div id={id} className="task-panel task-panel--error" role="alert">
        Couldn&apos;t load tasks. {state.message} <button type="button" onClick={onRetry}>Retry loading tasks</button>
      </div>
    );
  }
  const selectedRemoved = selectedTaskId !== null && !state.hierarchy.tasks.some((task) => task.id === selectedTaskId);
  if (!selectedRemoved) lastSelectedHierarchyRef.current = state.hierarchy;
  const hierarchy = selectedTaskDirty && selectedRemoved && lastSelectedHierarchyRef.current
    ? lastSelectedHierarchyRef.current
    : state.hierarchy;
  const selectedTask = selectedTaskId === null
    ? hierarchy.tasks[0]
    : hierarchy.tasks.find((task) => task.id === selectedTaskId)
    ?? null;
  const selectTask = (taskId: string) => {
    if (taskId === selectedTask?.id) return;
    if (selectedTaskPending) return;
    if (selectedTaskDirty && !window.confirm("Discard the unsaved changes to this task?")) {
      setPendingSelectionTaskId(taskId);
      return;
    }
    setSelectedTaskDirty(false);
    setPendingSelectionTaskId(null);
    setSelectedTaskId(taskId);
  };
  return (
    <div id={id} className="task-panel job-workspace">
      <div className="job-workspace__heading">
        <div>
          <p className="eyebrow">Schedule</p>
          <h1>{job.name}</h1>
        </div>
        <TaskEditor job={job} hierarchy={hierarchy} client={client} onHierarchyChange={onHierarchyChange} onJobChange={onJobChange} />
      </div>
      {!job.scheduleStart ? (
        <div className="schedule-guidance" role="status">
          <strong>Start your schedule</strong>
          <span>Choose the first working day below, then add the work you need to track.</span>
        </div>
      ) : null}
      <details className="schedule-setup" open={!job.scheduleStart}>
        <summary>Schedule setup</summary>
        <ScheduleSettings job={job} hierarchy={hierarchy} client={client} onHierarchyChange={onHierarchyChange} onJobChange={onJobChange} />
      </details>
      {client.getSchedule ? <ScheduleProjection schedule={schedule} jobName={job.name} workdayDurationMinutes={job.calendar?.workdayDurationMinutes ?? DEFAULT_CALENDAR.workdayDurationMinutes} jobVersion={state.hierarchy.jobVersion} initialActiveTaskId={selectedTaskId} onTaskSelection={selectTask} onRetry={() => setScheduleReload((current) => current + 1)} /> : null}
      <section className="task-workspace" aria-label={`Tasks for ${job.name}`}>
        {schedule.status !== "loaded" || schedule.readModel.rowCount === 0 ? (
          <div className="task-workspace__list">
            <h2>Work</h2>
            {hierarchy.tasks.length === 0 ? <><p>No tasks yet.</p><p>Add the first piece of work above.</p></> : (
              <TaskPicker tasks={hierarchy.tasks} label={`Tasks for ${job.name}`} selectedTaskId={selectedTask?.id ?? null} onSelect={selectTask} />
            )}
          </div>
        ) : null}
        {selectedTask ? (
          <div className="task-workspace__editor">
            <h2>Edit {selectedTask.name}</h2>
            {removedDraftNotice ? <div role="alert">{removedDraftNotice} <button type="button" onClick={() => { setSelectedTaskDirty(false); setSelectedTaskId(null); setRemovedDraftNotice(null); }}>Discard removed task changes</button></div> : null}
            <fieldset disabled={Boolean(removedDraftNotice)}>
              <TaskEditor key={selectedTask.id} job={job} task={selectedTask} hierarchy={hierarchy} client={client} onHierarchyChange={onHierarchyChange} onJobChange={onJobChange} onDirtyChange={setSelectedTaskDirty} onPendingChange={setSelectedTaskPending} />
            </fieldset>
            {pendingSelectionTaskId ? <button type="button" onClick={() => selectTask(pendingSelectionTaskId)}>Edit selected task</button> : null}
          </div>
        ) : null}
      </section>
      <details className="advanced-schedule">
        <summary>Advanced schedule settings</summary>
        <CalendarExceptions job={job} hierarchy={hierarchy} client={client} onHierarchyChange={onHierarchyChange} onJobChange={onJobChange} />
        <BaselineSettings job={job} hierarchy={hierarchy} client={client} onHierarchyChange={onHierarchyChange} onJobChange={onJobChange} />
        <DependencyControls job={job} hierarchy={hierarchy} client={client} onHierarchyChange={onHierarchyChange} onJobChange={onJobChange} />
      </details>
    </div>
  );
}

function hierarchyVersion(state: TaskLoadState): number {
  return state.status === "loaded" ? state.hierarchy.jobVersion : 0;
}

function ScheduleProjection({ schedule, jobName, workdayDurationMinutes, jobVersion, initialActiveTaskId, onTaskSelection, onRetry }: { schedule: ScheduleLoadState; jobName: string; workdayDurationMinutes: number; jobVersion: number; initialActiveTaskId: string | null; onTaskSelection: (taskId: string) => void; onRetry: () => void }) {
  if (schedule.status === "loading") {
    return <p className="gantt-state" aria-live="polite">Loading schedule…</p>;
  }
  if (schedule.status === "setup") {
    return <p className="gantt-state" role="status">{schedule.message}</p>;
  }
  if (schedule.status === "error") {
    return <div className="gantt-state gantt-state--error" role="alert">Couldn&apos;t build schedule for {jobName}. {schedule.message} <button type="button" onClick={onRetry}>Retry schedule</button></div>;
  }
  if (schedule.readModel.rowCount === 0) {
    return <p className="gantt-state">No scheduled tasks yet.</p>;
  }
  return <LoadedSchedule readModel={schedule.readModel} jobName={jobName} workdayDurationMinutes={workdayDurationMinutes} current={schedule.readModel.jobVersion === jobVersion} initialActiveTaskId={initialActiveTaskId} onTaskSelection={onTaskSelection} />;
}

// Holds the focused-task state so the explanation panel follows the treegrid's
// roving cell. React renders the Rust-provided explanation facts and derives none.
function LoadedSchedule({ readModel, jobName, workdayDurationMinutes, current, initialActiveTaskId, onTaskSelection }: { readModel: GanttReadModel; jobName: string; workdayDurationMinutes: number; current: boolean; initialActiveTaskId: string | null; onTaskSelection: (taskId: string) => void }) {
  const [focusedTaskId, setFocusedTaskId] = useState<string | null>(null);
  const [todayDate] = useState(() => {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, "0");
    const day = String(now.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  });
  const focusedRow = useMemo(
    () => readModel.rows.find((row) => row.taskId === focusedTaskId) ?? null,
    [focusedTaskId, readModel.rows],
  );
  const taskNames = useMemo(() => new Map(readModel.rows.map((row) => [row.taskId, row.name])), [readModel.rows]);
  return (
    <div className="schedule-reading" aria-busy={!current}>
      <ScheduleSummary readModel={readModel} />
      <ScheduleExports readModel={readModel} jobName={jobName} workdayDurationMinutes={workdayDurationMinutes} disabled={!current} />
      {!current ? <p role="status">Updating schedule…</p> : null}
      <GanttTreegrid
        readModel={readModel}
        workdayDurationMinutes={workdayDurationMinutes}
        ariaLabel={`Schedule for ${jobName}`}
        todayDate={todayDate}
        initialDetailsVisible={false}
        initialActiveTaskId={initialActiveTaskId}
        viewportHeight={360}
        onActiveTaskChange={(taskId) => {
          setFocusedTaskId(taskId);
          if (taskId) onTaskSelection(taskId);
        }}
      />
      <ExplanationPanel row={focusedRow} taskNames={taskNames} workdayDurationMinutes={workdayDurationMinutes} />
    </div>
  );
}

function TaskPicker({ tasks, label, selectedTaskId, onSelect }: { tasks: Task[]; label: string; selectedTaskId: string | null; onSelect: (taskId: string) => void }) {
  return (
    <ol className="task-tree" aria-label={label}>
      {tasks.map((task) => (
        <li key={task.id}>
          <button type="button" className={task.id === selectedTaskId ? "task-picker__item task-picker__item--selected" : "task-picker__item"} onClick={() => onSelect(task.id)}>
            {task.name}
          </button>
        </li>
      ))}
    </ol>
  );
}

function durationInputMatchesPersisted(value: string, persisted: number | null | undefined, workdayMinutes: number): boolean {
  return parseWorkingDays(value, workdayMinutes) === (persisted ?? null);
}

function TaskEditor({
  job,
  task,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
  onDirtyChange,
  onPendingChange,
}: {
  job?: Job;
  task?: Task;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onPendingChange?: (pending: boolean) => void;
}) {
  const workdayMinutes = job?.calendar?.workdayDurationMinutes ?? DEFAULT_CALENDAR.workdayDurationMinutes;
  const [name, setName] = useState(task?.name ?? "");
  const [draftBaseVersion, setDraftBaseVersion] = useState<number | null>(null);
  const [newChildName, setNewChildName] = useState("");
  const [newParentTaskId, setNewParentTaskId] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [duration, setDuration] = useState(() => formatWorkingDays(task?.durationMinutes, workdayMinutes));
  const [durationBaseVersion, setDurationBaseVersion] = useState<number | null>(
    null,
  );
  const [startNoEarlierThan, setStartNoEarlierThan] = useState(
    task?.startNoEarlierThan ?? "",
  );
  const [finishNoLaterThan, setFinishNoLaterThan] = useState(
    task?.finishNoLaterThan ?? "",
  );
  const [constraintBaseVersion, setConstraintBaseVersion] = useState<number | null>(
    null,
  );
  const [percentComplete, setPercentComplete] = useState(
    task?.percentComplete == null ? "" : String(task.percentComplete),
  );
  const [actualStart, setActualStart] = useState(task?.actualStart ?? "");
  const [actualFinish, setActualFinish] = useState(task?.actualFinish ?? "");
  const [progressBaseVersion, setProgressBaseVersion] = useState<number | null>(
    null,
  );
  const isRootCreator = !task;
  const label = isRootCreator ? "New root task" : `Task name for ${task!.name}`;
  const siblings = hierarchy.tasks.filter(
    (candidate) => candidate.parentTaskId === task?.parentTaskId,
  );
  const taskIndex = task
    ? siblings.findIndex((candidate) => candidate.id === task.id)
    : -1;
  const parents = task ? eligibleParents(task, hierarchy.tasks) : [];

  useEffect(() => {
    if (!task) return;
    if (draftBaseVersion === null) setName(task.name);
    else if (task.version !== draftBaseVersion) setConflict(true);
    if (durationBaseVersion === null) {
      setDuration(formatWorkingDays(task.durationMinutes, workdayMinutes));
    } else if (task.version !== durationBaseVersion) {
      setConflict(true);
    }
    if (constraintBaseVersion === null) {
      setStartNoEarlierThan(task.startNoEarlierThan ?? "");
      setFinishNoLaterThan(task.finishNoLaterThan ?? "");
    } else if (task.version !== constraintBaseVersion) {
      setConflict(true);
    }
    if (progressBaseVersion === null) {
      setPercentComplete(
        task.percentComplete == null ? "" : String(task.percentComplete),
      );
      setActualStart(task.actualStart ?? "");
      setActualFinish(task.actualFinish ?? "");
    } else if (task.version !== progressBaseVersion) {
      setConflict(true);
    }
  }, [constraintBaseVersion, draftBaseVersion, durationBaseVersion, progressBaseVersion, task, workdayMinutes]);

  useEffect(() => {
    onDirtyChange?.(draftBaseVersion !== null || durationBaseVersion !== null || constraintBaseVersion !== null || progressBaseVersion !== null || newChildName !== "" || newParentTaskId !== null || pending);
  }, [constraintBaseVersion, draftBaseVersion, durationBaseVersion, newChildName, newParentTaskId, onDirtyChange, pending, progressBaseVersion]);

  useEffect(() => {
    onPendingChange?.(pending);
    return () => onPendingChange?.(false);
  }, [onPendingChange, pending]);

  async function run(
    action: () => Promise<TaskHierarchy>,
    onSuccess?: () => void,
  ) {
    setPending(true);
    setError(null);
    try {
      onHierarchyChange(await action());
      onSuccess?.();
      setConflict(false);
      if (isRootCreator) setName("");
      setNewChildName("");
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) setConflict(true);
      else setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }

  async function refresh() {
    setPending(true);
    try {
      const snapshot = await loadJobSnapshot(client, hierarchy.jobId);
      const refreshed = snapshot.hierarchy;
      onJobChange(snapshot.job);
      if (task && draftBaseVersion !== null) {
        const refreshedTask = refreshed.tasks.find(
          (candidate) => candidate.id === task.id,
        );
        if (refreshedTask) setDraftBaseVersion(refreshedTask.version);
      }
      if (task && durationBaseVersion !== null) {
        const refreshedTask = refreshed.tasks.find(
          (candidate) => candidate.id === task.id,
        );
        if (refreshedTask) setDurationBaseVersion(refreshedTask.version);
      }
      if (task && constraintBaseVersion !== null) {
        const refreshedTask = refreshed.tasks.find(
          (candidate) => candidate.id === task.id,
        );
        if (refreshedTask) setConstraintBaseVersion(refreshedTask.version);
      }
      if (task && progressBaseVersion !== null) {
        const refreshedTask = refreshed.tasks.find(
          (candidate) => candidate.id === task.id,
        );
        if (refreshedTask) setProgressBaseVersion(refreshedTask.version);
      }
      onHierarchyChange(refreshed);
      setConflict(false);
    } catch (reason: unknown) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }

  const isSummary = task
    ? hierarchy.tasks.some((candidate) => candidate.parentTaskId === task.id)
    : false;

  async function saveConstraint(kind: TaskConstraintKind, value: string | null) {
    if (!task || !client.updateTaskConstraint || pending) return;
    setPending(true);
    setError(null);
    try {
      await client.updateTaskConstraint({
        taskId: task.id,
        kind,
        value,
        expectedVersion: constraintBaseVersion ?? task.version,
        expectedJobVersion: hierarchy.jobVersion,
      });
      const snapshot = await loadJobSnapshot(client, hierarchy.jobId);
      onJobChange(snapshot.job);
      onHierarchyChange(snapshot.hierarchy);
      setConstraintBaseVersion(null);
      setConflict(false);
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) setConflict(true);
      else setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }

  async function saveDuration() {
    if (!task || !client.updateTaskDuration || pending) return;
    const parsed = parseWorkingDays(duration, workdayMinutes);
    if (parsed === undefined) {
      setError("Enter a valid duration in working days.");
      return;
    }
    await run(
      () => client.updateTaskDuration!({
        taskId: task.id,
        durationMinutes: parsed,
        expectedVersion: durationBaseVersion ?? task.version,
        expectedJobVersion: hierarchy.jobVersion,
      }),
      () => setDurationBaseVersion(null),
    );
  }

  async function saveProgress(clear: boolean) {
    if (!task || !client.updateTaskProgress || pending) return;
    // Reject non-integer percent client-side so a decimal never surfaces an
    // opaque serialization error from the Rust command boundary.
    if (!clear && percentComplete !== "") {
      const parsed = Number(percentComplete);
      if (!Number.isInteger(parsed) || parsed < 0 || parsed > 100) {
        setError("Percent complete must be a whole number between 0 and 100.");
        return;
      }
    }
    setPending(true);
    setError(null);
    try {
      await client.updateTaskProgress({
        taskId: task.id,
        clear,
        percentComplete: clear || percentComplete === "" ? null : Number(percentComplete),
        actualStart: clear ? null : actualStart || null,
        actualFinish: clear ? null : actualFinish || null,
        expectedVersion: progressBaseVersion ?? task.version,
        expectedJobVersion: hierarchy.jobVersion,
      });
      const snapshot = await loadJobSnapshot(client, hierarchy.jobId);
      onJobChange(snapshot.job);
      onHierarchyChange(snapshot.hierarchy);
      setProgressBaseVersion(null);
      setConflict(false);
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) setConflict(true);
      else setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }

  function submitName(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || pending) return;
    if (!isRootCreator && name.trim() === task!.name) return;
    if (isRootCreator) {
      void run(() =>
        client.createTask({
          jobId: hierarchy.jobId,
          parentTaskId: null,
          name: name.trim(),
          expectedJobVersion: hierarchy.jobVersion,
        }),
      );
    } else {
      void run(
        () =>
          client.updateTask({
            taskId: task!.id,
            name: name.trim(),
            expectedVersion: draftBaseVersion ?? task!.version,
          }),
        () => setDraftBaseVersion(null),
      );
    }
  }

  return (
    <div className="task-editor">
      <form className="task-editor__form" onSubmit={submitName}>
        <label>
          <span className="visually-hidden">{label}</span>
          <input
            aria-label={label}
            value={name}
            onChange={(event) => {
              const nextName = event.target.value;
              setName(nextName);
              if (task) {
                if (nextName === task.name) {
                  setDraftBaseVersion(null);
                  setConflict(false);
                } else {
                  setDraftBaseVersion((current) => current ?? task.version);
                }
              }
            }}
            maxLength={200}
            autoComplete="off"
            placeholder={isRootCreator ? "e.g. Site work" : undefined}
          />
        </label>
        <button
          type="submit"
          disabled={
            pending ||
            !name.trim() ||
            (!isRootCreator && name.trim() === task!.name)
          }
        >
          {isRootCreator ? "Add task" : "Save"}
        </button>
      </form>
      {task ? (
        <div
          className="task-editor__actions"
          aria-label={`Actions for ${task.name}`}
        >
          {isSummary ? <p>Dates and duration come from subtasks.</p> : <>
            <label className="task-editor__duration">
              <span>Duration</span>
              <input
                aria-label={`Duration for ${task.name}`}
                type="number"
                min="0"
                step="any"
                value={duration}
                onChange={(event) => {
                  const nextDuration = event.target.value;
                  setDuration(nextDuration);
                  setDurationBaseVersion(
                    durationInputMatchesPersisted(nextDuration, task.durationMinutes, workdayMinutes)
                      ? null
                      : (current) => current ?? task.version,
                  );
                }}
              />
              <span>working days</span>
            </label>
            <button
              type="button"
              disabled={
                pending ||
                !client.updateTaskDuration ||
                durationInputMatchesPersisted(duration, task.durationMinutes, workdayMinutes)
              }
              onClick={() => void saveDuration()}
            >
              Save duration
            </button>
          </>}
          {!isSummary && client.updateTaskProgress ? (
            <fieldset className="task-editor__progress">
              <legend>Progress</legend>
              <label>
                Percent complete
                <input aria-label={`Percent complete for ${task.name}`} type="number" min="0" max="100" step="1" value={percentComplete} disabled={pending} onChange={(event) => { setPercentComplete(event.target.value); setProgressBaseVersion((current) => current ?? task.version); }} />
              </label>
              <label>
                Actual start
                <input aria-label={`Actual start for ${task.name}`} type="date" value={actualStart} disabled={pending} onChange={(event) => { setActualStart(event.target.value); setProgressBaseVersion((current) => current ?? task.version); }} />
              </label>
              <label>
                Actual finish
                <input aria-label={`Actual finish for ${task.name}`} type="date" value={actualFinish} disabled={pending} onChange={(event) => { setActualFinish(event.target.value); setProgressBaseVersion((current) => current ?? task.version); }} />
              </label>
              <button type="button" disabled={pending || percentComplete === ""} onClick={() => void saveProgress(false)}>Save progress</button>
              <button type="button" disabled={pending || task.percentComplete == null} onClick={() => void saveProgress(true)}>Clear progress</button>
            </fieldset>
          ) : null}
          <details className="task-editor__advanced">
            <summary>Advanced task details</summary>
          {!isSummary ? (
            <fieldset className="task-editor__constraints">
              <legend>Schedule constraints</legend>
              <label>
                Start no earlier than
                <input
                  aria-label={`Start no earlier than for ${task.name}`}
                  type="date"
                  value={startNoEarlierThan}
                  disabled={pending || !client.updateTaskConstraint}
                  onChange={(event) => {
                    setStartNoEarlierThan(event.target.value);
                    setConstraintBaseVersion((current) => current ?? task.version);
                  }}
                />
              </label>
              <button
                type="button"
                disabled={
                  pending ||
                  !client.updateTaskConstraint ||
                  startNoEarlierThan === (task.startNoEarlierThan ?? "")
                }
                onClick={() => void saveConstraint("start_no_earlier_than", startNoEarlierThan || null)}
              >
                Save start constraint
              </button>
              <button
                type="button"
                disabled={pending || !client.updateTaskConstraint || !task.startNoEarlierThan}
                onClick={() => void saveConstraint("start_no_earlier_than", null)}
              >
                Clear start constraint
              </button>
              <label>
                Finish no later than
                <input
                  aria-label={`Finish no later than for ${task.name}`}
                  type="date"
                  value={finishNoLaterThan}
                  disabled={pending || !client.updateTaskConstraint}
                  onChange={(event) => {
                    setFinishNoLaterThan(event.target.value);
                    setConstraintBaseVersion((current) => current ?? task.version);
                  }}
                />
              </label>
              <button
                type="button"
                disabled={
                  pending ||
                  !client.updateTaskConstraint ||
                  finishNoLaterThan === (task.finishNoLaterThan ?? "")
                }
                onClick={() => void saveConstraint("finish_no_later_than", finishNoLaterThan || null)}
              >
                Save finish constraint
              </button>
              <button
                type="button"
                disabled={pending || !client.updateTaskConstraint || !task.finishNoLaterThan}
                onClick={() => void saveConstraint("finish_no_later_than", null)}
              >
                Clear finish constraint
              </button>
            </fieldset>
          ) : null}
          <button
            type="button"
            disabled={pending || taskIndex <= 0}
            onClick={() =>
              void run(() =>
                client.reorderTask({
                  taskId: task.id,
                  newParentTaskId: task.parentTaskId,
                  newSiblingIndex: taskIndex - 1,
                  expectedVersion: task.version,
                  expectedJobVersion: hierarchy.jobVersion,
                }),
              )
            }
          >
            Move up
          </button>
          <button
            type="button"
            disabled={pending || taskIndex === siblings.length - 1}
            onClick={() =>
              void run(() =>
                client.reorderTask({
                  taskId: task.id,
                  newParentTaskId: task.parentTaskId,
                  newSiblingIndex: taskIndex + 1,
                  expectedVersion: task.version,
                  expectedJobVersion: hierarchy.jobVersion,
                }),
              )
            }
          >
            Move down
          </button>
          <label>
            <span className="visually-hidden">New parent for {task.name}</span>
            <select
              aria-label={`New parent for ${task.name}`}
              value={newParentTaskId ?? ""}
              onChange={(event) =>
                setNewParentTaskId(event.target.value || null)
              }
              disabled={pending}
            >
              <option value="">Top level</option>
              {parents.map((parent) => (
                <option key={parent.id} value={parent.id}>
                  Under {parent.name}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            disabled={pending || newParentTaskId === task.parentTaskId}
            onClick={() =>
              void run(() =>
                client.reorderTask({
                  taskId: task.id,
                  newParentTaskId,
                  newSiblingIndex: hierarchy.tasks.filter(
                    (candidate) =>
                      candidate.parentTaskId === newParentTaskId &&
                      candidate.id !== task.id,
                  ).length,
                  expectedVersion: task.version,
                  expectedJobVersion: hierarchy.jobVersion,
                }),
              )
            }
          >
            Move to parent
          </button>
          <form
            className="task-editor__child-form"
            onSubmit={(event) => {
              event.preventDefault();
              if (!newChildName.trim() || pending) return;
              void run(() =>
                client.createTask({
                  jobId: hierarchy.jobId,
                  parentTaskId: task.id,
                  name: newChildName.trim(),
                  expectedJobVersion: hierarchy.jobVersion,
                }),
              );
            }}
          >
            <label>
              <span className="visually-hidden">
                New child task for {task.name}
              </span>
              <input
                aria-label={`New child task for ${task.name}`}
                value={newChildName}
                onChange={(event) => setNewChildName(event.target.value)}
                maxLength={200}
                placeholder="Add subtask"
              />
            </label>
            <button type="submit" disabled={pending || !newChildName.trim()}>
              Add subtask
            </button>
          </form>
          </details>
        </div>
      ) : null}
      {conflict ? (
        <div className="task-editor__conflict" role="alert">
          <span>
            Tasks changed elsewhere. Your pending change is still here.
          </span>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={pending}
          >
            Refresh tasks
          </button>
        </div>
      ) : null}
      {error ? (
        <p className="task-editor__error" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

const WEEKDAYS: { value: CalendarWeekday; label: string }[] = [
  { value: "monday", label: "Mon" },
  { value: "tuesday", label: "Tue" },
  { value: "wednesday", label: "Wed" },
  { value: "thursday", label: "Thu" },
  { value: "friday", label: "Fri" },
  { value: "saturday", label: "Sat" },
  { value: "sunday", label: "Sun" },
];

const DEFAULT_CALENDAR: WorkingCalendar = {
  workingWeekdays: ["monday", "tuesday", "wednesday", "thursday", "friday"],
  workdayStartMinute: 480,
  workdayDurationMinutes: 480,
};

function ScheduleSettings({
  job,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  job: Job;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const [start, setStart] = useState(job.scheduleStart ?? "");
  const [calendar, setCalendar] = useState<WorkingCalendar>(
    job.calendar ?? DEFAULT_CALENDAR,
  );
  const [baseVersion, setBaseVersion] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [dataDate, setDataDate] = useState(job.dataDate ?? "");
  const [dataDateBaseVersion, setDataDateBaseVersion] = useState<number | null>(
    null,
  );
  // The persisted values a dirty draft was based on. When a sibling command (a
  // baseline create/switch) bumps the job version without touching schedule
  // inputs, we silently rebase the draft instead of raising a phantom conflict;
  // a genuinely changed persisted input still raises it.
  const scheduleBaseRef = useRef<{ start: string; calendar: WorkingCalendar } | null>(null);
  const dataDateBaseRef = useRef<{ value: string } | null>(null);
  useEffect(() => {
    if (baseVersion === null) {
      setStart(job.scheduleStart ?? "");
      setCalendar(job.calendar ?? DEFAULT_CALENDAR);
    } else if (hierarchy.jobVersion !== baseVersion) {
      const base = scheduleBaseRef.current;
      if (
        base !== null &&
        base.start === (job.scheduleStart ?? "") &&
        calendarsEqual(base.calendar, job.calendar ?? DEFAULT_CALENDAR)
      ) {
        setBaseVersion(hierarchy.jobVersion);
      } else {
        setConflict(true);
      }
    }
    if (dataDateBaseVersion === null) {
      setDataDate(job.dataDate ?? "");
    } else if (hierarchy.jobVersion !== dataDateBaseVersion) {
      const base = dataDateBaseRef.current;
      if (base !== null && base.value === (job.dataDate ?? "")) {
        setDataDateBaseVersion(hierarchy.jobVersion);
      } else {
        setConflict(true);
      }
    }
  }, [
    baseVersion,
    dataDateBaseVersion,
    hierarchy.jobVersion,
    job.calendar,
    job.dataDate,
    job.scheduleStart,
  ]);
  if (!client.updateSchedule) return null;
  async function saveDataDate(value: string | null) {
    if (!client.updateJobDataDate || saving) return;
    setSaving(true);
    setMessage(null);
    try {
      await client.updateJobDataDate({
        jobId: job.id,
        dataDate: value,
        expectedJobVersion: dataDateBaseVersion ?? hierarchy.jobVersion,
      });
      const snapshot = await loadJobSnapshot(client, job.id);
      onJobChange(snapshot.job);
      onHierarchyChange(snapshot.hierarchy);
      setDataDateBaseVersion(null);
      // The save advanced the job version; re-baseline a dirty schedule draft so
      // the effect does not re-raise the conflict banner on the fresh version.
      setBaseVersion((current) => (current === null ? null : snapshot.hierarchy.jobVersion));
      setConflict(false);
      setMessage("Data date saved.");
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) setConflict(true);
      setMessage(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }
  const markDirty = () =>
    setBaseVersion((current) => {
      if (current === null) {
        scheduleBaseRef.current = {
          start: job.scheduleStart ?? "",
          calendar: job.calendar ?? DEFAULT_CALENDAR,
        };
      }
      return current ?? hierarchy.jobVersion;
    });
  const refresh = async () => {
    setSaving(true);
    setMessage(null);
    try {
      const snapshot = await loadJobSnapshot(client, job.id);
      const refreshedHierarchy = snapshot.hierarchy;
      onJobChange(snapshot.job);
      onHierarchyChange(refreshedHierarchy);
      setBaseVersion(refreshedHierarchy.jobVersion);
      // Re-baseline the persisted refs too, or the next sibling version bump
      // rebases against stale values and re-raises a phantom conflict.
      scheduleBaseRef.current = {
        start: snapshot.job.scheduleStart ?? "",
        calendar: snapshot.job.calendar ?? DEFAULT_CALENDAR,
      };
      // Re-baseline the data-date draft too, or a dirty field plus a job-version
      // bump wedges the conflict banner until the component remounts.
      if (dataDateBaseVersion !== null) {
        setDataDateBaseVersion(refreshedHierarchy.jobVersion);
        dataDateBaseRef.current = { value: snapshot.job.dataDate ?? "" };
      }
      setConflict(false);
    } catch (reason: unknown) {
      setMessage(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  };
  return (
    <form
      className="schedule-settings"
      onSubmit={(event) => {
        event.preventDefault();
        setSaving(true);
        setMessage(null);
        void client.updateSchedule!({
          jobId: job.id,
          scheduleStart: start || null,
          // Send only the weekly-calendar fields. Dated exceptions are owned by
          // the dedicated add/remove commands, so we omit them here rather than
          // relying on the server to strip them from this payload.
          calendar: {
            workingWeekdays: [...calendar.workingWeekdays],
            workdayStartMinute: calendar.workdayStartMinute,
            workdayDurationMinutes: calendar.workdayDurationMinutes,
          },
          expectedJobVersion: baseVersion ?? hierarchy.jobVersion,
        })
          .then((updatedJob) => {
            onJobChange(updatedJob);
            setBaseVersion(null);
            setConflict(false);
            setMessage("Schedule settings saved.");
          })
          .catch((reason: unknown) => {
            if (isVersionConflict(reason)) setConflict(true);
            setMessage(errorMessage(reason));
          })
          .finally(() => setSaving(false));
      }}
    >
      <label>
        Schedule start{" "}
        <input
          aria-label={`Schedule start for ${job.name}`}
          type="date"
          value={start}
          onChange={(event) => {
            setStart(event.target.value);
            markDirty();
          }}
        />
      </label>
      {client.updateJobDataDate ? (
        <div className="schedule-settings__data-date">
          <label>
            Data date{" "}
            <input
              aria-label={`Data date for ${job.name}`}
              type="date"
              value={dataDate}
              disabled={saving}
              onChange={(event) => {
                setDataDate(event.target.value);
                setDataDateBaseVersion((current) => {
                  if (current === null) {
                    dataDateBaseRef.current = { value: job.dataDate ?? "" };
                  }
                  return current ?? hierarchy.jobVersion;
                });
              }}
            />
          </label>
          <button
            type="button"
            disabled={saving || dataDate === (job.dataDate ?? "")}
            onClick={() => void saveDataDate(dataDate || null)}
          >
            Save data date
          </button>
          <button
            type="button"
            disabled={saving || !job.dataDate}
            onClick={() => void saveDataDate(null)}
          >
            Clear data date
          </button>
        </div>
      ) : null}
      <fieldset>
        <legend>Working weekdays</legend>
        {WEEKDAYS.map(({ value, label }) => (
          <label key={value}>
            <input
              type="checkbox"
              checked={calendar.workingWeekdays.includes(value)}
              onChange={(event) => {
                markDirty();
                setCalendar((current) => ({
                  ...current,
                  workingWeekdays: event.target.checked
                    ? [...current.workingWeekdays, value]
                    : current.workingWeekdays.filter((day) => day !== value),
                }));
              }}
            />
            {label}
          </label>
        ))}
      </fieldset>
      <button type="submit" disabled={saving}>
        {saving ? "Saving…" : "Save schedule settings"}
      </button>
      {conflict ? (
        <span role="alert">
          Schedule inputs changed elsewhere. Your pending values are still here.{" "}
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={saving}
          >
            Refresh schedule
          </button>
        </span>
      ) : message ? (
        <span role="status">{message}</span>
      ) : null}
    </form>
  );
}

// Dated calendar-exception management: list the job's non-working closures, add
// one, or remove one, using the audited #53 commands. Each add/remove is a real
// schedule-input change, so a success reloads the job snapshot; the version bump
// re-drives the Gantt getSchedule effect (reshading the timeline) and lets the
// sibling ScheduleSettings drafts rebase, since the persisted schedule start,
// weekly calendar, and data date are untouched. Typed rejections (out-of-range,
// duplicate) surface like other validation errors; a version conflict refreshes.
function CalendarExceptions({
  job,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  job: Job;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const add = client.addCalendarException;
  const remove = client.removeCalendarException;
  const [date, setDate] = useState("");
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  if (!add || !remove) return null;
  const exceptions = [...(job.calendarExceptions ?? [])].sort();

  async function mutate(kind: "add" | "remove", value: string) {
    if (saving || !value) return;
    setSaving(true);
    setMessage(null);
    try {
      const command = kind === "add" ? add! : remove!;
      await command({
        jobId: job.id,
        date: value,
        expectedJobVersion: hierarchy.jobVersion,
      });
      const snapshot = await loadJobSnapshot(client, job.id);
      onJobChange(snapshot.job);
      onHierarchyChange(snapshot.hierarchy);
      if (kind === "add") setDate("");
      setMessage(kind === "add" ? `Added closure ${value}.` : `Removed closure ${value}.`);
    } catch (reason: unknown) {
      if (isVersionConflict(reason)) {
        // Recover the stale version so an immediate retry can succeed.
        try {
          const snapshot = await loadJobSnapshot(client, job.id);
          onJobChange(snapshot.job);
          onHierarchyChange(snapshot.hierarchy);
        } catch {
          // Ignore a secondary refresh failure; the message still explains it.
        }
        setMessage("The job changed elsewhere. Refreshed the list — try again.");
      } else {
        setMessage(errorMessage(reason));
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <section
      className="schedule-settings__exceptions"
      aria-label={`Calendar exceptions for ${job.name}`}
    >
      <h4>Office closures and holidays</h4>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void mutate("add", date);
        }}
      >
        <label>
          Closure date{" "}
          <input
            aria-label={`Calendar exception date for ${job.name}`}
            type="date"
            value={date}
            disabled={saving}
            onChange={(event) => setDate(event.target.value)}
          />
        </label>
        <button type="submit" disabled={saving || !date}>
          Add closure
        </button>
      </form>
      {exceptions.length ? (
        <ul className="schedule-settings__exception-list">
          {exceptions.map((value) => (
            <li key={value}>
              <span>{value}</span>
              <button
                type="button"
                disabled={saving}
                aria-label={`Remove calendar exception ${value}`}
                onClick={() => void mutate("remove", value)}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p>No closures yet.</p>
      )}
      {message ? <span role="status">{message}</span> : null}
    </section>
  );
}

// Baseline management: list existing baselines, create a named baseline, and
// switch the comparison default. Mirrors the data-date flow (typed rejections,
// version-conflict recovery). A successful command reloads the job snapshot so
// the version bump re-drives the Gantt getSchedule effect onto the new baseline.
function BaselineSettings({
  job,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  job: Job;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const [baselines, setBaselines] = useState<Baseline[]>([]);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);

  const listBaselines = client.listBaselines;
  useEffect(() => {
    if (!listBaselines) return;
    let active = true;
    listBaselines(job.id)
      .then((list) => {
        if (active) setBaselines(list);
      })
      .catch((reason: unknown) => {
        if (active) setMessage(errorMessage(reason));
      });
    return () => {
      active = false;
    };
    // Re-list whenever the job version advances (create/switch bumps it).
  }, [listBaselines, job.id, job.version]);

  if (
    !client.listBaselines ||
    !client.createBaseline ||
    !client.setBaselineComparisonDefault
  )
    return null;

  // A successful command advanced the job version: reload the canonical snapshot
  // so the Gantt and the list rebase onto it, clearing any phantom conflict.
  const applied = async (status: string) => {
    const snapshot = await loadJobSnapshot(client, job.id);
    onJobChange(snapshot.job);
    onHierarchyChange(snapshot.hierarchy);
    setConflict(false);
    setMessage(status);
  };
  const failed = (reason: unknown) => {
    setConflict(isVersionConflict(reason));
    setMessage(errorMessage(reason));
  };
  const refresh = async () => {
    setSaving(true);
    try {
      const snapshot = await loadJobSnapshot(client, job.id);
      onJobChange(snapshot.job);
      onHierarchyChange(snapshot.hierarchy);
      setConflict(false);
      setMessage(null);
    } catch (reason: unknown) {
      setMessage(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  };

  async function createBaseline() {
    if (!client.createBaseline || saving) return;
    const trimmed = name.trim();
    if (trimmed === "") {
      setMessage("Enter a baseline name.");
      return;
    }
    setSaving(true);
    setMessage(null);
    try {
      await client.createBaseline({
        jobId: job.id,
        name: trimmed,
        expectedJobVersion: hierarchy.jobVersion,
      });
    } catch (reason: unknown) {
      failed(reason);
      setSaving(false);
      return;
    }
    // The baseline is committed. A post-command reload failure must never report
    // the create as failed; clear the name only once the reload succeeds.
    try {
      await applied(`Baseline “${trimmed}” created.`);
      setName("");
    } catch {
      setConflict(true);
      setMessage(
        `Baseline “${trimmed}” was created, but refreshing the view failed. Refresh to see it.`,
      );
    } finally {
      setSaving(false);
    }
  }

  async function setDefault(baselineId: string) {
    if (!client.setBaselineComparisonDefault || saving) return;
    setSaving(true);
    setMessage(null);
    try {
      await client.setBaselineComparisonDefault({
        jobId: job.id,
        baselineId,
        expectedJobVersion: hierarchy.jobVersion,
      });
    } catch (reason: unknown) {
      failed(reason);
      setSaving(false);
      return;
    }
    // The switch is committed. A post-command reload failure must never report
    // the switch as failed.
    try {
      await applied("Comparison baseline updated.");
    } catch {
      setConflict(true);
      setMessage("Comparison baseline updated, but refreshing the view failed. Refresh to see it.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="baseline-settings" aria-labelledby="baseline-settings-heading">
      <h3 id="baseline-settings-heading">Baselines</h3>
      <div className="baseline-settings__create">
        <label>
          New baseline name{" "}
          <input
            aria-label={`New baseline name for ${job.name}`}
            type="text"
            value={name}
            disabled={saving}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <button
          type="button"
          disabled={saving || name.trim() === ""}
          onClick={() => void createBaseline()}
        >
          Create baseline
        </button>
      </div>
      {baselines.length > 0 ? (
        <ul className="baseline-settings__list">
          {baselines.map((baseline) => (
            <li key={baseline.id} className="baseline-settings__item">
              <span className="baseline-settings__name">{baseline.name}</span>
              <span className="baseline-settings__created">
                Created {formatLocalDate(baseline.createdAt)}
              </span>
              {baseline.isComparisonDefault ? (
                <span className="baseline-settings__default">Comparison default</span>
              ) : (
                <button
                  type="button"
                  disabled={saving}
                  onClick={() => void setDefault(baseline.id)}
                >
                  {`Set "${baseline.name}" as comparison default`}
                </button>
              )}
            </li>
          ))}
        </ul>
      ) : (
        <p>No baselines yet.</p>
      )}
      {conflict ? (
        <span role="alert">
          Baselines changed elsewhere. Your entry is still here.{" "}
          <button type="button" onClick={() => void refresh()} disabled={saving}>
            Refresh baselines
          </button>
        </span>
      ) : message ? (
        <span role="status">{message}</span>
      ) : null}
    </section>
  );
}

function DependencyControls({
  job,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  job: Job;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const workdayMinutes = job.calendar?.workdayDurationMinutes ?? DEFAULT_CALENDAR.workdayDurationMinutes;
  const [predecessorTaskId, setPredecessor] = useState("");
  const [successorTaskId, setSuccessor] = useState("");
  const [dependencyType, setDependencyType] = useState<DependencyType>("FS");
  const [lagDays, setLagDays] = useState("0");
  const [message, setMessage] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  if (
    !client.addDependency ||
    !client.removeDependency ||
    hierarchy.tasks.length < 2
  )
    return null;
  const taskName = (id: string) =>
    hierarchy.tasks.find((task) => task.id === id)?.name ?? id;
  const failed = (reason: unknown) => {
    setMessage(errorMessage(reason));
    setConflict(isVersionConflict(reason));
  };
  const succeeded = (next: TaskHierarchy) => {
    setConflict(false);
    setMessage(null);
    onHierarchyChange(next);
  };
  const refresh = () =>
    void loadJobSnapshot(client, hierarchy.jobId)
      .then((snapshot) => {
        onJobChange(snapshot.job);
        succeeded(snapshot.hierarchy);
      })
      .catch(failed);
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setMessage(null);
    // Reject a non-integer lag client-side so a decimal — or a bare "-" that
    // would coerce to NaN — never reaches the store. Lag is signed working
    // minutes; the store bounds the magnitude.
    const lag = parseSignedWorkingDayLag(lagDays, workdayMinutes);
    if (lag === undefined) {
      setMessage("Enter a valid lag in working days within the supported range.");
      return;
    }
    void client.addDependency!({
      jobId: hierarchy.jobId,
      predecessorTaskId,
      successorTaskId,
      dependencyType,
      lagMinutes: lag,
      expectedJobVersion: hierarchy.jobVersion,
    })
      .then(succeeded)
      .catch(failed);
  };
  return (
    <section
      className="dependency-controls"
      aria-label="Task dependencies"
    >
      <form onSubmit={submit}>
        <label>
          Predecessor{" "}
          <select
            aria-label="Dependency predecessor"
            value={predecessorTaskId}
            onChange={(event) => setPredecessor(event.target.value)}
          >
            <option value="">Select task</option>
            {hierarchy.tasks.map((task) => (
              <option value={task.id} key={task.id}>
                {task.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Successor{" "}
          <select
            aria-label="Dependency successor"
            value={successorTaskId}
            onChange={(event) => setSuccessor(event.target.value)}
          >
            <option value="">Select task</option>
            {hierarchy.tasks.map((task) => (
              <option value={task.id} key={task.id}>
                {task.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Type{" "}
          <select
            aria-label="Dependency type"
            value={dependencyType}
            onChange={(event) =>
              setDependencyType(event.target.value as DependencyType)
            }
          >
            <option value="FS">Finish-to-start (FS)</option>
            <option value="SS">Start-to-start (SS)</option>
            <option value="FF">Finish-to-finish (FF)</option>
            <option value="SF">Start-to-finish (SF)</option>
          </select>
        </label>
        <label>
          Lag{" "}
          <input
            aria-label="Dependency lag in working days"
            type="number"
            step="any"
            value={lagDays}
            onChange={(event) => setLagDays(event.target.value)}
          />
          <span>working days</span>
        </label>
        <button type="submit" disabled={!predecessorTaskId || !successorTaskId}>
          Add dependency
        </button>
      </form>
      {(hierarchy.dependencies ?? []).map((dependency) => (
        <div
          key={`${dependency.predecessorTaskId}-${dependency.successorTaskId}-${dependency.dependencyType}`}
        >
          <span>
            {taskName(dependency.predecessorTaskId)} {dependency.dependencyType}{" "}
            {taskName(dependency.successorTaskId)}
            {dependency.lagMinutes !== 0
              ? ` ${formatSignedDays(dependency.lagMinutes, workdayMinutes)}`
              : ""}
          </span>
          <button
            type="button"
            aria-label={`Remove ${dependency.dependencyType} dependency from ${taskName(dependency.predecessorTaskId)} to ${taskName(dependency.successorTaskId)}`}
            onClick={() =>
              void client.removeDependency!({
                jobId: hierarchy.jobId,
                predecessorTaskId: dependency.predecessorTaskId,
                successorTaskId: dependency.successorTaskId,
                dependencyType: dependency.dependencyType,
                expectedJobVersion: hierarchy.jobVersion,
              })
                .then(succeeded)
                .catch(failed)
            }
          >
            Remove dependency
          </button>
        </div>
      ))}
      {message ? (
        <p role="alert">
          {message}
          {conflict ? (
            <button type="button" onClick={refresh}>
              Refresh dependencies
            </button>
          ) : null}
        </p>
      ) : null}
    </section>
  );
}

function eligibleParents(task: Task, tasks: Task[]): Task[] {
  const descendants = new Set<string>([task.id]);
  for (let found = true; found; ) {
    found = false;
    for (const candidate of tasks) {
      if (
        candidate.parentTaskId &&
        descendants.has(candidate.parentTaskId) &&
        !descendants.has(candidate.id)
      ) {
        descendants.add(candidate.id);
        found = true;
      }
    }
  }
  return tasks.filter((candidate) => !descendants.has(candidate.id));
}

function groupTasksByParent(tasks: Task[]): Map<string | null, Task[]> {
  const children = new Map<string | null, Task[]>();
  for (const task of tasks) {
    const siblings = children.get(task.parentTaskId) ?? [];
    siblings.push(task);
    children.set(task.parentTaskId, siblings);
  }
  return children;
}

async function loadJobSnapshot(
  client: JobClient,
  jobId: string,
): Promise<{ job: Job; hierarchy: TaskHierarchy }> {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const jobs = await client.listJobs();
    const job = jobs.find((candidate) => candidate.id === jobId);
    if (!job) throw new Error("The job is no longer available.");
    const hierarchy = await client.listTasks(jobId);
    if (job.version === hierarchy.jobVersion) return { job, hierarchy };
  }
  throw new Error(
    "The job changed while refreshing. Refresh again before saving.",
  );
}

// Content equality for working calendars (weekday set plus workday window),
// used to tell an unchanged persisted schedule from a genuine external edit.
function calendarsEqual(left: WorkingCalendar, right: WorkingCalendar): boolean {
  if (
    left.workdayStartMinute !== right.workdayStartMinute ||
    left.workdayDurationMinutes !== right.workdayDurationMinutes ||
    left.workingWeekdays.length !== right.workingWeekdays.length
  ) {
    return false;
  }
  const leftDays = [...left.workingWeekdays].sort();
  const rightDays = [...right.workingWeekdays].sort();
  return leftDays.every((day, index) => day === rightDays[index]);
}

// Renders a stored UTC timestamp as a local ISO calendar date (YYYY-MM-DD),
// matching every other date shown in the app.
function formatLocalDate(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  const year = parsed.getFullYear();
  const month = `${parsed.getMonth() + 1}`.padStart(2, "0");
  const day = `${parsed.getDate()}`.padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function errorMessage(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object" && "message" in reason) {
    const message = reason.message;
    if (typeof message === "string") return message;
  }
  return "Please try again.";
}

/** Keeps normal first-time schedule setup instructional, while real failures remain actionable. */
function scheduleLoadState(reason: unknown): ScheduleLoadState {
  const code = scheduleValidationCode(reason);
  if (code === "schedule_start_required") {
    return { status: "setup", message: "Choose a schedule start to calculate task dates." };
  }
  if (code === "summary_without_children") {
    return { status: "setup", message: "Add a duration to every leaf task to calculate the schedule." };
  }
  return { status: "error", message: errorMessage(reason) };
}

function scheduleValidationCode(reason: unknown): string | null {
  if (reason && typeof reason === "object" && "code" in reason && typeof reason.code === "string") {
    return reason.code;
  }
  return null;
}

function isVersionConflict(reason: unknown): boolean {
  if (reason && typeof reason === "object" && "kind" in reason) {
    return reason.kind === "version_conflict";
  }
  if (typeof reason === "string") {
    return reason.includes("version_conflict");
  }
  return false;
}
