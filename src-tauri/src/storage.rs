use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::domain::{Job, JobStatus};
use crate::error::ApplicationError;

pub(crate) struct SqliteJobStore {
    database_path: PathBuf,
}

impl SqliteJobStore {
    pub(crate) fn open(database_path: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        let database_path = database_path.as_ref().to_path_buf();
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { database_path };
        store.migrate()?;
        Ok(store)
    }

    pub(crate) fn insert_job(&self, job: &Job) -> Result<(), ApplicationError> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO jobs (
                id, name, status, timezone, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                job.id,
                job.name,
                job.status.as_database_value(),
                job.timezone,
                job.created_at,
                job.updated_at,
                job.version,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn list_jobs(&self) -> Result<Vec<Job>, ApplicationError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, status, timezone, created_at, updated_at, version
             FROM jobs
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(
                |(id, name, status, timezone, created_at, updated_at, version)| {
                    let status = JobStatus::from_database_value(&status).ok_or_else(|| {
                        ApplicationError::InvalidStoredData(format!(
                            "job {id} has unsupported status {status}"
                        ))
                    })?;
                    Ok(Job {
                        id,
                        name,
                        status,
                        timezone,
                        created_at,
                        updated_at,
                        version,
                    })
                },
            )
            .collect()
    }

    fn connection(&self) -> Result<Connection, ApplicationError> {
        let connection = Connection::open(&self.database_path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn migrate(&self) -> Result<(), ApplicationError> {
        let connection = self.connection()?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timezone TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL CHECK (version > 0)
             );
             INSERT OR IGNORE INTO schema_migrations (version, applied_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        Ok(())
    }
}
