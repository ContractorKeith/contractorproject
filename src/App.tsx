import { FormEvent, useEffect, useState } from "react";

import { tauriJobClient, type JobClient } from "./api/jobs";
import { BrandMark } from "./components/BrandMark";
import { loadThemePreference, watchTheme, type ThemePreference } from "./theme";
import type { Job } from "./types/jobs";

interface AppProps {
  client?: JobClient;
}

export function App({ client = tauriJobClient }: AppProps) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
                    <p>Schedule setup is ready for the next slice.</p>
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

function errorMessage(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object" && "message" in reason) {
    const message = reason.message;
    if (typeof message === "string") return message;
  }
  return "Please try again.";
}
