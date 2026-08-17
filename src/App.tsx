import { FormEvent, useEffect, useRef, useState } from "react";

import { tauriJobClient, type JobClient } from "./api/jobs";
import { BrandMark } from "./components/BrandMark";
import { GanttTreegrid } from "./gantt/GanttTreegrid";
import { loadThemePreference, watchTheme, type ThemePreference } from "./theme";
import type {
  CalendarWeekday,
  Job,
  Task,
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
  | { status: "error"; message: string };

export function App({ client = tauriJobClient }: AppProps) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [archivedJobs, setArchivedJobs] = useState<Job[]>([]);
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const [archiveConflict, setArchiveConflict] = useState(false);
  const [pendingJobId, setPendingJobId] = useState<string | null>(null);
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [archivedLoading, setArchivedLoading] = useState(false);
  const [openJobId, setOpenJobId] = useState<string | null>(null);
  const [taskLoads, setTaskLoads] = useState<Record<string, TaskLoadState>>({});
  const [theme, setTheme] = useState<ThemePreference>(loadThemePreference);
  const [backupPending, setBackupPending] = useState(false);
  const [backupResult, setBackupResult] = useState<"cancelled" | { destination: string; createdAtUtc: string; byteSize: number } | null>(null);
  const [backupError, setBackupError] = useState<string | null>(null);
  const backupButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(
    () =>
      watchTheme(theme, (resolvedTheme) => {
        document.documentElement.dataset.theme = resolvedTheme;
      }),
    [theme],
  );

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

    setCreating(true);
    setError(null);
    try {
      const job = await client.createJob({
        name: name.trim(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
      });
      setJobs((current) => [job, ...current]);
      setName("");
    } catch (reason: unknown) {
      setError(errorMessage(reason));
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
      backupButtonRef.current?.focus();
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
    setLoading(true);
    try {
      setJobs(await client.listJobs());
      if (archivedOpen) await loadArchivedJobs();
    } catch (reason: unknown) {
      setArchiveError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function handleArchive(job: Job) {
    if (!client.archiveJob || pendingJobId) return;
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

  async function handleToggleTasks(job: Job) {
    if (openJobId === job.id) {
      setOpenJobId(null);
      return;
    }

    setOpenJobId(job.id);
    if (taskLoads[job.id]) return;

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
    <div className="app-shell">
      <header className="app-header">
        <a className="brand" href="#main" aria-label="ContractorProject home">
          <BrandMark />
          <span className="brand__name">
            Contractor<span>Project</span>
          </span>
        </a>
        <div className="header-controls">
          <label className="theme-control">
            <span>Theme</span>
            <select
              aria-label="Theme"
              value={theme}
              onChange={(event) =>
                setTheme(event.target.value as ThemePreference)
              }
            >
              <option value="system">System</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </label>
          <div className="storage-actions">
            <div className="storage-state" aria-label="Local storage status">
              <span className="storage-state__dot" />
              Local SQLite · on this device
            </div>
            {client.createVerifiedBackup ? (
              <button
                ref={backupButtonRef}
                className="backup-action"
                type="button"
                onClick={() => void handleCreateVerifiedBackup()}
                disabled={backupPending}
                aria-describedby="backup-status"
              >
                {backupPending ? "Creating backup…" : "Create verified backup"}
              </button>
            ) : null}
          </div>
        </div>
      </header>

      <main id="main" className="workspace">
        <div id="backup-status" className="backup-status" aria-live="polite">
          {backupResult === "cancelled" ? "Backup cancelled. Your local data was not changed." : null}
          {backupResult && backupResult !== "cancelled" ? (
            <p>
              Verified backup created: {backupResult.destination} · {backupResult.byteSize.toLocaleString()} bytes · {backupResult.createdAtUtc}
            </p>
          ) : null}
        </div>
        {backupError ? (
          <div className="inline-error" role="alert">
            <strong>Couldn&apos;t create verified backup.</strong>
            <span>{backupError}</span>
          </div>
        ) : null}
        <section className="workspace-heading" aria-labelledby="jobs-heading">
          <div>
            <p className="eyebrow">Jobs</p>
            <h1 id="jobs-heading">Your work, on your machine.</h1>
            <p className="lede">
              Start with one job. Schedules, crews, costs, and files will stay
              local unless you choose to export them.
            </p>
          </div>

          <form className="new-job" onSubmit={handleCreateJob}>
            <label htmlFor="job-name">Job name</label>
            <div className="new-job__controls">
              <input
                id="job-name"
                name="jobName"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. Ridgeline Fence — Phase 2"
                maxLength={120}
                autoComplete="off"
              />
              <button type="submit" disabled={creating || !name.trim()}>
                {creating ? "Creating…" : "Create job"}
              </button>
            </div>
          </form>
        </section>

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
            <button type="button" onClick={() => void refreshJobs()} disabled={loading}>
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
            <h2>Local jobs</h2>
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
                Create the first job above. It will be stored in this app&apos;s
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
                    <div className="job-card__meta">
                      <span className="status-tag">{job.status}</span>
                      <span>{job.timezone}</span>
                    </div>
                    <h3>{job.name}</h3>
                    <button
                      className="archive-action"
                      type="button"
                      onClick={() => void handleArchive(job)}
                      disabled={!client.archiveJob || pendingJobId !== null}
                    >
                      {pendingJobId === job.id ? "Archiving…" : "Archive job"}
                    </button>
                    <button
                      className="task-disclosure"
                      type="button"
                      aria-label={`${openJobId === job.id ? "Hide" : "View"} tasks for ${job.name}`}
                      aria-expanded={openJobId === job.id}
                      aria-controls={`task-panel-${job.id}`}
                      onClick={() => void handleToggleTasks(job)}
                    >
                      {openJobId === job.id ? "Hide tasks" : "View tasks"}
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
}: {
  id: string;
  job: Job;
  state: TaskLoadState;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const [schedule, setSchedule] = useState<ScheduleLoadState>({ status: "loading" });

  useEffect(() => {
    if (!client.getSchedule) return;
    let active = true;
    setSchedule({ status: "loading" });
    client.getSchedule(job.id)
      .then((readModel) => active && setSchedule({ status: "loaded", readModel }))
      .catch((reason: unknown) => active && setSchedule({ status: "error", message: errorMessage(reason) }));
    return () => { active = false; };
  }, [client, hierarchyVersion(state), job.id, job.version]);
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
        Couldn&apos;t load tasks. {state.message}
      </div>
    );
  }
  const children = groupTasksByParent(state.hierarchy.tasks);
  return (
    <div id={id} className="task-panel">
      <TaskEditor
        job={job}
        hierarchy={state.hierarchy}
        client={client}
        onHierarchyChange={onHierarchyChange}
        onJobChange={onJobChange}
      />
      <ScheduleSettings
        job={job}
        hierarchy={state.hierarchy}
        client={client}
        onHierarchyChange={onHierarchyChange}
        onJobChange={onJobChange}
      />
      <DependencyControls
        hierarchy={state.hierarchy}
        client={client}
        onHierarchyChange={onHierarchyChange}
        onJobChange={onJobChange}
      />
      {client.getSchedule ? <ScheduleProjection schedule={schedule} jobName={job.name} /> : null}
      {state.hierarchy.tasks.length === 0 ? <p>No tasks yet.</p> : null}
      <TaskList
        children={children}
        parentTaskId={null}
        label={`Tasks for ${job.name}`}
        hierarchy={state.hierarchy}
        client={client}
        onHierarchyChange={onHierarchyChange}
        onJobChange={onJobChange}
      />
    </div>
  );
}

function hierarchyVersion(state: TaskLoadState): number {
  return state.status === "loaded" ? state.hierarchy.jobVersion : 0;
}

function ScheduleProjection({ schedule, jobName }: { schedule: ScheduleLoadState; jobName: string }) {
  if (schedule.status === "loading") {
    return <p className="gantt-state" aria-live="polite">Loading schedule…</p>;
  }
  if (schedule.status === "error") {
    return <div className="gantt-state gantt-state--error" role="alert">Couldn&apos;t build schedule for {jobName}. {schedule.message}</div>;
  }
  if (schedule.readModel.rowCount === 0) {
    return <p className="gantt-state">No scheduled tasks yet.</p>;
  }
  return <GanttTreegrid readModel={schedule.readModel} ariaLabel={`Schedule for ${jobName}`} />;
}

function TaskList({
  children,
  parentTaskId,
  label,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  children: Map<string | null, Task[]>;
  parentTaskId: string | null;
  label?: string;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const tasks = children.get(parentTaskId) ?? [];
  return (
    <ol className="task-tree" aria-label={label}>
      {tasks.map((task) => (
        <li key={task.id}>
          <TaskEditor
            task={task}
            hierarchy={hierarchy}
            client={client}
            onHierarchyChange={onHierarchyChange}
            onJobChange={onJobChange}
          />
          {children.has(task.id) ? (
            <TaskList
              children={children}
              parentTaskId={task.id}
              hierarchy={hierarchy}
              client={client}
              onHierarchyChange={onHierarchyChange}
              onJobChange={onJobChange}
            />
          ) : null}
        </li>
      ))}
    </ol>
  );
}

function TaskEditor({
  job,
  task,
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  job?: Job;
  task?: Task;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const [name, setName] = useState(task?.name ?? "");
  const [draftBaseVersion, setDraftBaseVersion] = useState<number | null>(null);
  const [newChildName, setNewChildName] = useState("");
  const [newParentTaskId, setNewParentTaskId] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [duration, setDuration] = useState(
    task?.durationMinutes == null ? "" : String(task.durationMinutes),
  );
  const [durationBaseVersion, setDurationBaseVersion] = useState<number | null>(
    null,
  );
  const isRootCreator = Boolean(job);
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
      setDuration(
        task.durationMinutes == null ? "" : String(task.durationMinutes),
      );
    } else if (task.version !== durationBaseVersion) {
      setConflict(true);
    }
  }, [draftBaseVersion, durationBaseVersion, task]);

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
      onHierarchyChange(refreshed);
      setConflict(false);
    } catch (reason: unknown) {
      setError(errorMessage(reason));
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
          <label className="task-editor__duration">
            <span>Duration (min)</span>
            <input
              aria-label={`Duration for ${task.name}`}
              type="number"
              min="0"
              value={duration}
              onChange={(event) => {
                const nextDuration = event.target.value;
                setDuration(nextDuration);
                const storedDuration =
                  task.durationMinutes == null
                    ? ""
                    : String(task.durationMinutes);
                setDurationBaseVersion(
                  nextDuration === storedDuration
                    ? null
                    : (current) => current ?? task.version,
                );
              }}
            />
          </label>
          <button
            type="button"
            disabled={
              pending ||
              !client.updateTaskDuration ||
              duration ===
                (task.durationMinutes == null
                  ? ""
                  : String(task.durationMinutes))
            }
            onClick={() =>
              void run(
                () =>
                  client.updateTaskDuration!({
                    taskId: task.id,
                    durationMinutes: duration === "" ? null : Number(duration),
                    expectedVersion: durationBaseVersion ?? task.version,
                    expectedJobVersion: hierarchy.jobVersion,
                  }),
                () => setDurationBaseVersion(null),
              )
            }
          >
            Save duration
          </button>
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
  useEffect(() => {
    if (baseVersion === null) {
      setStart(job.scheduleStart ?? "");
      setCalendar(job.calendar ?? DEFAULT_CALENDAR);
    } else if (hierarchy.jobVersion !== baseVersion) {
      setConflict(true);
    }
  }, [baseVersion, hierarchy.jobVersion, job.calendar, job.scheduleStart]);
  if (!client.updateSchedule) return null;
  const markDirty = () =>
    setBaseVersion((current) => current ?? hierarchy.jobVersion);
  const refresh = async () => {
    setSaving(true);
    setMessage(null);
    try {
      const snapshot = await loadJobSnapshot(client, job.id);
      const refreshedHierarchy = snapshot.hierarchy;
      onJobChange(snapshot.job);
      onHierarchyChange(refreshedHierarchy);
      setBaseVersion(refreshedHierarchy.jobVersion);
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
          calendar: {
            ...calendar,
            workingWeekdays: [...calendar.workingWeekdays],
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
      <label>
        Workday start (minutes after midnight){" "}
        <input
          aria-label="Workday start minute"
          type="number"
          min="0"
          max="1439"
          value={calendar.workdayStartMinute}
          onChange={(event) => {
            markDirty();
            setCalendar((current) => ({
              ...current,
              workdayStartMinute: Number(event.target.value),
            }));
          }}
        />
      </label>
      <label>
        Working minutes per day{" "}
        <input
          aria-label="Workday duration minutes"
          type="number"
          min="1"
          max="1440"
          value={calendar.workdayDurationMinutes}
          onChange={(event) => {
            markDirty();
            setCalendar((current) => ({
              ...current,
              workdayDurationMinutes: Number(event.target.value),
            }));
          }}
        />
      </label>
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

function DependencyControls({
  hierarchy,
  client,
  onHierarchyChange,
  onJobChange,
}: {
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
  onJobChange: (job: Job) => void;
}) {
  const [predecessorTaskId, setPredecessor] = useState("");
  const [successorTaskId, setSuccessor] = useState("");
  const [lagMinutes, setLag] = useState("0");
  const [message, setMessage] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  if (
    !client.addDependency ||
    !client.removeDependency ||
    hierarchy.tasks.length < 2
  )
    return null;
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
    void client.addDependency!({
      jobId: hierarchy.jobId,
      predecessorTaskId,
      successorTaskId,
      lagMinutes: Number(lagMinutes),
      expectedJobVersion: hierarchy.jobVersion,
    })
      .then(succeeded)
      .catch(failed);
  };
  return (
    <section
      className="dependency-controls"
      aria-label="Finish-to-start dependencies"
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
          Lag{" "}
          <input
            aria-label="Dependency lag minutes"
            type="number"
            min="0"
            value={lagMinutes}
            onChange={(event) => setLag(event.target.value)}
          />
        </label>
        <button type="submit" disabled={!predecessorTaskId || !successorTaskId}>
          Add dependency
        </button>
      </form>
      {(hierarchy.dependencies ?? []).map((dependency) => (
        <div
          key={`${dependency.predecessorTaskId}-${dependency.successorTaskId}`}
        >
          <span>FS dependency</span>
          <button
            type="button"
            onClick={() =>
              void client.removeDependency!({
                jobId: hierarchy.jobId,
                predecessorTaskId: dependency.predecessorTaskId,
                successorTaskId: dependency.successorTaskId,
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

function errorMessage(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object" && "message" in reason) {
    const message = reason.message;
    if (typeof message === "string") return message;
  }
  return "Please try again.";
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
