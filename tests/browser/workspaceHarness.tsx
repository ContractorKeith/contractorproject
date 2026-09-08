import "@fontsource/barlow/latin-400.css";
import "@fontsource/barlow/latin-500.css";
import "@fontsource/barlow-condensed/latin-600.css";
import { createRoot } from "react-dom/client";
import { App } from "../../src/App";
import type { JobClient } from "../../src/api/jobs";
import { createGanttVerificationReadModel } from "../../src/gantt/verificationFixture";
import type { Job, Task, TaskHierarchy } from "../../src/types/jobs";
import "../../src/styles.css";

// In-memory application-seam fixture. Dates are canned Rust read-model facts:
// this harness verifies UI interaction, not scheduling or SQLite persistence.
const fixture = createGanttVerificationReadModel();
fixture.rows = fixture.rows.slice(0, 26).map((row, index) => ({
  ...row,
  name: index === 0 ? "Renovation" : index === 4 ? "Rough plumbing" : row.name,
  setSize: index === 0 ? 1 : 25,
}));
fixture.rowCount = fixture.rows.length;
fixture.scheduleFinish = fixture.rows.at(-1)!.finish;
fixture.rows[0]!.finish = fixture.scheduleFinish;
fixture.rows[0]!.durationMinutes = 12_000;
fixture.rows[0]!.percentComplete = 14;
fixture.criticalTaskIds = fixture.rows.filter((row) => row.critical).map((row) => row.taskId);
fixture.criticalPath = fixture.criticalTaskIds;

const job: Job = {
  id: fixture.jobId, name: "Riverside renovation", status: "draft",
  timezone: "America/New_York", version: 1,
  createdAt: "2026-08-17T08:00:00Z", updatedAt: "2026-08-17T08:00:00Z",
  scheduleStart: "2026-08-17", dataDate: "2026-08-21",
  calendar: { workingWeekdays: ["monday", "tuesday", "wednesday", "thursday", "friday"], workdayStartMinute: 480, workdayDurationMinutes: 360 },
};
let jobs = new URLSearchParams(location.search).has("empty") ? [] : [job];
const tasks: Task[] = fixture.rows.map((row) => ({
  id: row.taskId, jobId: job.id, parentTaskId: row.parentTaskId,
  sortKey: row.sortKey, name: row.name, createdAt: job.createdAt,
  updatedAt: job.updatedAt, version: 1, durationMinutes: row.summary ? null : row.durationMinutes,
  percentComplete: row.percentComplete, actualStart: row.actualStart?.slice(0, 10) ?? null,
  actualFinish: row.actualFinish?.slice(0, 10) ?? null,
}));
const hierarchy = (jobId: string): TaskHierarchy => ({
  jobId, jobVersion: jobs.find((item) => item.id === jobId)!.version,
  tasks: structuredClone(tasks.filter((task) => task.jobId === jobId)),
  dependencies: [],
});
const findJob = (id: string) => jobs.find((item) => item.id === id)!;
const findTask = (id: string) => tasks.find((item) => item.id === id)!;
const bumpTask = (task: Task) => { task.version++; findJob(task.jobId).version++; };
const client: JobClient = {
  async listJobs(status = "draft") { return structuredClone(jobs.filter((item) => item.status === status)); },
  async createJob(request) {
    const created = { ...structuredClone(job), ...request, id: `new-job-${jobs.length}`, scheduleStart: null, dataDate: null };
    jobs = [...jobs, created];
    return structuredClone(created);
  },
  async listTasks(id) { return hierarchy(id); },
  async getSchedule(id) {
    const current = findJob(id);
    if (!current.scheduleStart) {
      throw { kind: "validation", code: "schedule_start_required", message: "set a schedule start before viewing the schedule" };
    }
    const currentTasks = tasks.filter((task) => task.jobId === id);
    const parentIds = new Set(currentTasks.map((task) => task.parentTaskId));
    if (currentTasks.some((task) => !parentIds.has(task.id) && task.durationMinutes == null)) {
      throw { kind: "validation", code: "summary_without_children", message: "summary task must have at least one child" };
    }
    const rows = currentTasks.map((task, index) => ({
      ...(fixture.rows.find((row) => row.taskId === task.id) ?? fixture.rows[1]!),
      taskId: task.id, name: task.name, logicalIndex: index,
      parentTaskId: task.parentTaskId,
      durationMinutes: task.durationMinutes ?? fixture.rows.find((row) => row.taskId === task.id)!.durationMinutes,
      percentComplete: task.percentComplete ?? 0,
      progressStatus: task.percentComplete === 100 ? "completed" as const : task.percentComplete ? "inProgress" as const : "notStarted" as const,
    }));
    return structuredClone({ ...fixture, jobId: id, jobVersion: current.version, rowCount: rows.length, rows });
  },
  async createTask(request) {
    const task: Task = {
      id: `new-task-${tasks.length}`, jobId: request.jobId, name: request.name,
      parentTaskId: request.parentTaskId, sortKey: tasks.length,
      createdAt: job.createdAt, updatedAt: job.updatedAt, version: 1,
    };
    tasks.push(task); findJob(request.jobId).version++;
    return hierarchy(request.jobId);
  },
  async updateTask(request) {
    const task = findTask(request.taskId); task.name = request.name; bumpTask(task);
    return hierarchy(task.jobId);
  },
  async reorderTask() { throw new Error("Reordering is covered by application tests."); },
  async updateTaskDuration(request) {
    const task = findTask(request.taskId); task.durationMinutes = request.durationMinutes; bumpTask(task);
    return hierarchy(task.jobId);
  },
  async updateSchedule(request) {
    const current = findJob(request.jobId);
    Object.assign(current, { scheduleStart: request.scheduleStart, calendar: request.calendar });
    current.version++;
    return structuredClone(current);
  },
  async updateJobDataDate(request) {
    const current = findJob(request.jobId); current.dataDate = request.dataDate; current.version++;
    return structuredClone(current);
  },
  async updateTaskProgress(request) {
    const current = findTask(request.taskId);
    Object.assign(current, {
      percentComplete: request.clear ? null : request.percentComplete,
      actualStart: request.clear ? null : request.actualStart,
      actualFinish: request.clear ? null : request.actualFinish,
    });
    bumpTask(current);
    return { task: structuredClone(current), jobVersion: findJob(current.jobId).version };
  },
};

createRoot(document.getElementById("root")!).render(<App client={client} />);
