use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ScheduleExportKind {
    Csv,
    Html,
}

impl ScheduleExportKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Csv => "Schedule CSV",
            Self::Html => "Printable schedule",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Html => "html",
        }
    }

    fn content_is_valid(self, bytes: &[u8]) -> bool {
        match self {
            Self::Csv => bytes.starts_with("\u{feff}Job name,WBS,Task,Kind,".as_bytes()),
            Self::Html => bytes.starts_with(b"<!doctype html>\n"),
        }
    }
}

/// Write a fully staged export at a new path, never replacing an existing file.
/// The staging file is in the destination directory so hard-link publication is
/// atomic and cannot expose a partial export.
pub(crate) fn write_new_export(
    destination: &Path,
    active_database: &Path,
    kind: ScheduleExportKind,
    filename: &str,
    content: &[u8],
) -> Result<(), String> {
    validate_export(kind, filename, content)?;
    reject_active_storage_path(destination, active_database)?;

    let parent = destination
        .parent()
        .ok_or_else(|| "Choose a destination inside an existing folder.".to_owned())?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Could not access the destination folder: {error}"))?;
    let destination_name = destination
        .file_name()
        .ok_or_else(|| "Choose a valid filename.".to_owned())?;
    let destination = parent.join(destination_name);
    let actual_filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Choose a valid filename.".to_owned())?;
    validate_export(kind, actual_filename, content)?;
    reject_active_storage_path(&destination, active_database)?;

    let staging = unique_staging_path(&parent);
    let result = write_and_publish(&staging, &destination, content);
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

pub(crate) fn validate_export(
    kind: ScheduleExportKind,
    filename: &str,
    content: &[u8],
) -> Result<(), String> {
    if filename.is_empty()
        || filename.len() > 180
        || filename == "."
        || filename == ".."
        || filename
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0'))
        || Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(filename)
    {
        return Err("The export filename is invalid.".to_owned());
    }
    if !filename
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case(kind.extension()))
    {
        return Err(format!("The filename must end in .{}.", kind.extension()));
    }
    if content.is_empty() || content.len() > MAX_EXPORT_BYTES {
        return Err(format!(
            "The export must be between 1 byte and {MAX_EXPORT_BYTES} bytes."
        ));
    }
    if !kind.content_is_valid(content) {
        return Err("The schedule export contents are invalid.".to_owned());
    }
    Ok(())
}

fn reject_active_storage_path(destination: &Path, active_database: &Path) -> Result<(), String> {
    let active_parent = active_database
        .parent()
        .ok_or_else(|| "Could not locate the active database folder.".to_owned())?;
    let active_parent = fs::canonicalize(active_parent)
        .map_err(|error| format!("Could not locate the active database folder: {error}"))?;
    let active_name = active_database
        .file_name()
        .ok_or_else(|| "Could not locate the active database filename.".to_owned())?
        .to_string_lossy()
        .to_ascii_lowercase();

    let parent = destination
        .parent()
        .ok_or_else(|| "Choose a destination inside an existing folder.".to_owned())?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Could not access the destination folder: {error}"))?;
    if parent != active_parent {
        return Ok(());
    }
    let name = destination
        .file_name()
        .ok_or_else(|| "Choose a valid filename.".to_owned())?
        .to_string_lossy()
        .to_ascii_lowercase();
    let protected = [
        active_name.clone(),
        format!("{active_name}-wal"),
        format!("{active_name}-shm"),
        format!("{active_name}-journal"),
    ];
    if protected.iter().any(|candidate| candidate == &name) {
        return Err(
            "The active database and its sidecars cannot be export destinations.".to_owned(),
        );
    }
    Ok(())
}

fn unique_staging_path(parent: &Path) -> PathBuf {
    loop {
        let candidate = parent.join(format!(
            ".contractorproject-export-{}.tmp",
            uuid::Uuid::now_v7()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
}

fn write_and_publish(staging: &Path, destination: &Path, content: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .map_err(|error| format!("Could not stage the export: {error}"))?;
    if let Err(error) = file.write_all(content).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(staging);
        return Err(format!("Could not write the export: {error}"));
    }
    drop(file);
    fs::hard_link(staging, destination)
        .map_err(|error| format!("Could not create the export file: {error}"))?;
    fs::remove_file(staging).map_err(|error| {
        format!("The export was created, but temporary cleanup failed: {error}")
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{write_new_export, ScheduleExportKind};
    use std::fs;

    const CSV: &[u8] = b"\xef\xbb\xbfJob name,WBS,Task,Kind,\r\n";
    const HTML: &[u8] = b"<!doctype html>\n<html></html>";

    #[test]
    fn writes_csv_and_html_only_to_new_paths() {
        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("app/contractorproject.sqlite3");
        fs::create_dir_all(active.parent().unwrap()).unwrap();
        fs::write(&active, b"database").unwrap();
        let csv = temp.path().join("schedule.csv");
        write_new_export(&csv, &active, ScheduleExportKind::Csv, "schedule.csv", CSV).unwrap();
        assert_eq!(fs::read(csv).unwrap(), CSV);
        let html = temp.path().join("schedule.html");
        write_new_export(
            &html,
            &active,
            ScheduleExportKind::Html,
            "schedule.html",
            HTML,
        )
        .unwrap();
        assert_eq!(fs::read(html).unwrap(), HTML);
    }

    #[test]
    fn existing_destination_is_never_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("app/contractorproject.sqlite3");
        fs::create_dir_all(active.parent().unwrap()).unwrap();
        fs::write(&active, b"database").unwrap();
        let destination = temp.path().join("existing.csv");
        fs::write(&destination, b"original").unwrap();
        let error = write_new_export(
            &destination,
            &active,
            ScheduleExportKind::Csv,
            "existing.csv",
            CSV,
        )
        .unwrap_err();
        assert!(error.contains("Could not create the export file"));
        assert_eq!(fs::read(destination).unwrap(), b"original");
    }

    #[test]
    fn refuses_active_database_and_sidecars_without_mutating_them() {
        let temp = tempfile::tempdir().unwrap();
        let app_data = temp.path().join("app");
        fs::create_dir_all(&app_data).unwrap();
        let active = app_data.join("contractorproject.sqlite3");
        fs::write(&active, b"database").unwrap();
        let wal = app_data.join("contractorproject.sqlite3-wal");
        fs::write(&wal, b"wal").unwrap();
        for destination in [&active, &wal] {
            let error = write_new_export(
                destination,
                &active,
                ScheduleExportKind::Csv,
                "export.csv",
                CSV,
            )
            .unwrap_err();
            assert!(error.contains("cannot be export destinations"));
        }
        assert_eq!(fs::read(active).unwrap(), b"database");
        assert_eq!(fs::read(wal).unwrap(), b"wal");
    }

    #[test]
    fn rejects_invalid_filename_and_payload_before_creating_anything() {
        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("app/contractorproject.sqlite3");
        fs::create_dir_all(active.parent().unwrap()).unwrap();
        fs::write(&active, b"database").unwrap();
        let destination = temp.path().join("../escape.csv");
        assert!(write_new_export(
            &destination,
            &active,
            ScheduleExportKind::Csv,
            "../escape.csv",
            CSV,
        )
        .is_err());
        let destination = temp.path().join("invalid.csv");
        assert!(write_new_export(
            &destination,
            &active,
            ScheduleExportKind::Csv,
            "invalid.csv",
            b"not a csv",
        )
        .is_err());
        assert!(!destination.exists());
    }
}
