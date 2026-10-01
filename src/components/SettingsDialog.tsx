import { FormEvent, useEffect, useRef } from "react";

import "./SettingsDialog.css";

export interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
  jobName: string;
  onJobNameChange: (name: string) => void;
  onCreateJob: (event: FormEvent<HTMLFormElement>) => void;
  creating: boolean;
  createError: string | null;
  canCreateBackup: boolean;
  backupPending: boolean;
  backupResult: "cancelled" | { destination: string; createdAtUtc: string; byteSize: number } | null;
  backupError: string | null;
  onCreateBackup: () => void;
  backupButtonRef: React.RefObject<HTMLButtonElement | null>;
}

export function SettingsDialog({
  open,
  onClose,
  jobName,
  onJobNameChange,
  onCreateJob,
  creating,
  createError,
  canCreateBackup,
  backupPending,
  backupResult,
  backupError,
  onCreateBackup,
  backupButtonRef,
}: SettingsDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) closeDialog();
  }, [open]);

  function closeDialog() {
    const dialog = dialogRef.current;
    if (!dialog) return;
    dialog.close();
  }

  return (
    <dialog
      ref={dialogRef}
      className="settings-dialog"
      aria-labelledby="settings-title"
      onClose={onClose}
    >
      <div className="settings-dialog__header">
        <h2 id="settings-title">Settings</h2>
        <button className="settings-dialog__close" type="button" aria-label="Close settings" onClick={closeDialog}>
          <span aria-hidden="true">×</span>
        </button>
      </div>

      <section className="settings-dialog__section" aria-labelledby="settings-create-title">
        <h3 id="settings-create-title">Create a job</h3>
        <form className="new-job" onSubmit={onCreateJob}>
          <label htmlFor="job-name">Job name</label>
          <div className="new-job__controls">
            <input
              id="job-name"
              name="jobName"
              value={jobName}
              onChange={(event) => onJobNameChange(event.target.value)}
              placeholder="e.g. Ridgeline Fence — Phase 2"
              maxLength={120}
              autoComplete="off"
              autoFocus
            />
            <button type="submit" disabled={creating || !jobName.trim()}>
              {creating ? "Creating…" : "Create job"}
            </button>
          </div>
        </form>
        {createError ? <div className="inline-error" role="alert">{createError}</div> : null}
      </section>

      <section className="settings-dialog__section" aria-labelledby="settings-storage-title">
        <h3 id="settings-storage-title">Backups</h3>
        {canCreateBackup ? (
          <>
            <button
              ref={backupButtonRef}
              className="backup-action"
              type="button"
              onClick={onCreateBackup}
              disabled={backupPending}
              aria-describedby="backup-status"
            >
              {backupPending ? "Creating backup…" : "Create verified backup"}
            </button>
            <div id="backup-status" className="backup-status" aria-live="polite">
              {backupResult === "cancelled" ? "Backup cancelled. Your local data was not changed." : null}
              {backupResult && backupResult !== "cancelled" ? (
                <p>Verified backup created: {backupResult.destination} · {backupResult.byteSize.toLocaleString()} bytes · {backupResult.createdAtUtc}</p>
              ) : null}
            </div>
            {backupError ? (
              <div className="inline-error" role="alert">
                <strong>Couldn&apos;t create verified backup.</strong>
                <span>{backupError}</span>
              </div>
            ) : null}
          </>
        ) : <p>Verified backups are unavailable in this environment.</p>}
      </section>
    </dialog>
  );
}
