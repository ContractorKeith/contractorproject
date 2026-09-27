//! Supported operator entry point for verifying a backup in a fresh app-data
//! directory. It never offers in-place database replacement.

use std::path::PathBuf;

use serde::Serialize;

use crate::application::{ApplicationService, VerifyRestoreRequest};
use uuid::Uuid;

pub const USAGE: &str = "\
ContractorProject fresh-directory recovery verification

Usage:
  contractorproject-recovery --backup <backup-file> \\
    --target-app-data-dir <new-directory>

The target directory must not exist. This verifies a backup by restoring it
into a separate app-data directory through ApplicationService. It never
replaces the active database.
";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    pub backup: PathBuf,
    pub target_app_data_dir: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub verified: bool,
    pub job_count: i64,
    pub task_count: i64,
    pub dependency_count: i64,
    pub command_log_count: i64,
    pub restored_database: PathBuf,
}

pub fn parse_arguments(arguments: &[String]) -> Result<Option<Options>, String> {
    let mut backup = None;
    let mut target = None;
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        if matches!(argument, "-h" | "--help") {
            return Ok(None);
        }
        let (name, value) = if let Some((name, value)) = argument.split_once('=') {
            (name, Some(value.to_owned()))
        } else {
            index += 1;
            (
                argument,
                Some(
                    arguments
                        .get(index)
                        .ok_or_else(|| format!("{argument} needs a value"))?
                        .clone(),
                ),
            )
        };
        match name {
            "--backup" => set_once(&mut backup, value.unwrap(), name)?,
            "--target-app-data-dir" => set_once(&mut target, value.unwrap(), name)?,
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }
    Ok(Some(Options {
        backup: PathBuf::from(backup.ok_or_else(|| "--backup <path> is required".to_owned())?),
        target_app_data_dir: PathBuf::from(
            target.ok_or_else(|| "--target-app-data-dir <path> is required".to_owned())?,
        ),
    }))
}

fn set_once(slot: &mut Option<String>, value: String, name: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{name} needs a non-empty path"));
    }
    if slot.replace(value).is_some() {
        return Err(format!("{name} may be specified only once"));
    }
    Ok(())
}

pub fn run(options: &Options) -> Result<Output, String> {
    let coordinator = CoordinatorDatabase::create()?;
    let service = ApplicationService::open(&coordinator.database)
        .map_err(|_| "could not create a temporary recovery coordinator".to_owned())?;
    let result = service
        .verify_restore_into_fresh_app_data(VerifyRestoreRequest {
            backup_path: options.backup.to_string_lossy().into_owned(),
            target_app_data_dir: options.target_app_data_dir.to_string_lossy().into_owned(),
        })
        .map_err(|error| error.to_string())?;
    drop(service);
    drop(coordinator);
    Ok(Output {
        verified: result.verified,
        job_count: result.job_count,
        task_count: result.task_count,
        dependency_count: result.dependency_count,
        command_log_count: result.command_log_count,
        restored_database: options
            .target_app_data_dir
            .join("contractorproject.sqlite3"),
    })
}

struct CoordinatorDatabase {
    directory: PathBuf,
    database: PathBuf,
}

impl CoordinatorDatabase {
    fn create() -> Result<Self, String> {
        let directory =
            std::env::temp_dir().join(format!("contractorproject-recovery-{}", Uuid::now_v7()));
        std::fs::create_dir(&directory)
            .map_err(|_| "could not create a temporary recovery coordinator".to_owned())?;
        Ok(Self {
            database: directory.join("coordinator.sqlite3"),
            directory,
        })
    }
}

impl Drop for CoordinatorDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_arguments;

    #[test]
    fn requires_backup_and_fresh_target_paths() {
        assert!(parse_arguments(&[]).unwrap_err().contains("--backup"));
        let parsed = parse_arguments(&[
            "--backup=backup.sqlite3".into(),
            "--target-app-data-dir".into(),
            "restored".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.backup, std::path::PathBuf::from("backup.sqlite3"));
        assert_eq!(
            parsed.target_app_data_dir,
            std::path::PathBuf::from("restored")
        );
    }

    #[test]
    fn coordinator_uses_a_new_temporary_directory() {
        let coordinator = super::CoordinatorDatabase::create().unwrap();
        assert!(coordinator.directory.starts_with(std::env::temp_dir()));
        assert!(!coordinator.database.exists());
        let directory = coordinator.directory.clone();
        drop(coordinator);
        assert!(!directory.exists());
    }
}
