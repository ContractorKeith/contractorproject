import { FormEvent, useEffect, useState } from "react";

import { tauriJobClient, type JobClient } from "./api/jobs";
import { BrandMark } from "./components/BrandMark";
import { loadThemePreference, watchTheme, type ThemePreference } from "./theme";
import type { Job, Task, TaskHierarchy } from "./types/jobs";

interface AppProps {
  client?: JobClient;
}

type TaskLoadState =
  | { status: "loading" }
  | { status: "loaded"; hierarchy: TaskHierarchy }
  | { status: "error"; message: string };

export function App({ client = tauriJobClient }: AppProps) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [openJobId, setOpenJobId] = useState<string | null>(null);
  const [taskLoads, setTaskLoads] = useState<Record<string, TaskLoadState>>({});
  const [theme, setTheme] = useState<ThemePreference>(loadThemePreference);

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

  async function handleToggleTasks(job: Job) {
    if (openJobId === job.id) {
      setOpenJobId(null);
      return;
    }

    setOpenJobId(job.id);
    if (taskLoads[job.id]) return;

    setTaskLoads((current) => ({ ...current, [job.id]: { status: "loading" } }));
    try {
      const hierarchy = await client.listTasks(job.id);
      setTaskLoads((current) => ({
        ...current,
        [job.id]: { status: "loaded", hierarchy },
      }));
    } catch (reason: unknown) {
      setTaskLoads((current) => ({
        ...current,
        [job.id]: { status: "error", message: errorMessage(reason) },
      }));
    }
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
              onChange={(event) => setTheme(event.target.value as ThemePreference)}
            >
              <option value="system">System</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </label>
          <div className="storage-state" aria-label="Local storage status">
            <span className="storage-state__dot" />
            Local SQLite · on this device
          </div>
        </div>
      </header>

      <main id="main" className="workspace">
        <section className="workspace-heading" aria-labelledby="jobs-heading">
          <div>
            <p className="eyebrow">Jobs</p>
            <h1 id="jobs-heading">Your work, on your machine.</h1>
            <p className="lede">
              Start with one job. Schedules, crews, costs, and files will stay local unless you
              choose to export them.
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
              <p>Create the first job above. It will be stored in this app&apos;s local database.</p>
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
                          setTaskLoads((current) => ({
                            ...current,
                            [job.id]: { status: "loaded", hierarchy },
                          }))
                        }
                      />
                    ) : null}
                  </div>
                  <span className="job-card__local">Local</span>
                </article>
              ))}
            </div>
          )}
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
}: {
  id: string;
  job: Job;
  state: TaskLoadState;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
}) {
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
      />
      {state.hierarchy.tasks.length === 0 ? <p>No tasks yet.</p> : null}
      <TaskList
        children={children}
        parentTaskId={null}
        label={`Tasks for ${job.name}`}
        hierarchy={state.hierarchy}
        client={client}
        onHierarchyChange={onHierarchyChange}
      />
    </div>
  );
}

function TaskList({
  children,
  parentTaskId,
  label,
  hierarchy,
  client,
  onHierarchyChange,
}: {
  children: Map<string | null, Task[]>;
  parentTaskId: string | null;
  label?: string;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
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
          />
          {children.has(task.id) ? (
            <TaskList
              children={children}
              parentTaskId={task.id}
              hierarchy={hierarchy}
              client={client}
              onHierarchyChange={onHierarchyChange}
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
}: {
  job?: Job;
  task?: Task;
  hierarchy: TaskHierarchy;
  client: JobClient;
  onHierarchyChange: (hierarchy: TaskHierarchy) => void;
}) {
  const [name, setName] = useState(task?.name ?? "");
  const [draftBaseVersion, setDraftBaseVersion] = useState<number | null>(null);
  const [newChildName, setNewChildName] = useState("");
  const [newParentTaskId, setNewParentTaskId] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const isRootCreator = Boolean(job);
  const label = isRootCreator ? "New root task" : `Task name for ${task!.name}`;
  const siblings = hierarchy.tasks.filter((candidate) => candidate.parentTaskId === task?.parentTaskId);
  const taskIndex = task ? siblings.findIndex((candidate) => candidate.id === task.id) : -1;
  const parents = task ? eligibleParents(task, hierarchy.tasks) : [];

  useEffect(() => {
    if (!task) return;
    if (draftBaseVersion === null) setName(task.name);
    else if (task.version !== draftBaseVersion) setConflict(true);
  }, [draftBaseVersion, task]);

  async function run(action: () => Promise<TaskHierarchy>, onSuccess?: () => void) {
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
      const refreshed = await client.listTasks(hierarchy.jobId);
      if (task && draftBaseVersion !== null) {
        const refreshedTask = refreshed.tasks.find((candidate) => candidate.id === task.id);
        if (refreshedTask) setDraftBaseVersion(refreshedTask.version);
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
        <button type="submit" disabled={pending || !name.trim() || (!isRootCreator && name.trim() === task!.name)}>
          {isRootCreator ? "Add task" : "Save"}
        </button>
      </form>
      {task ? (
        <div className="task-editor__actions" aria-label={`Actions for ${task.name}`}>
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
              onChange={(event) => setNewParentTaskId(event.target.value || null)}
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
                    (candidate) => candidate.parentTaskId === newParentTaskId && candidate.id !== task.id,
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
              <span className="visually-hidden">New child task for {task.name}</span>
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
          <span>Tasks changed elsewhere. Your pending change is still here.</span>
          <button type="button" onClick={() => void refresh()} disabled={pending}>
            Refresh tasks
          </button>
        </div>
      ) : null}
      {error ? <p className="task-editor__error" role="alert">{error}</p> : null}
    </div>
  );
}

function eligibleParents(task: Task, tasks: Task[]): Task[] {
  const descendants = new Set<string>([task.id]);
  for (let found = true; found; ) {
    found = false;
    for (const candidate of tasks) {
      if (candidate.parentTaskId && descendants.has(candidate.parentTaskId) && !descendants.has(candidate.id)) {
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
