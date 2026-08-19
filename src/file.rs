use crate::config::{AppConfig, CompressionAlgorithm};
use crate::{debug_log, log};
use chrono::NaiveDateTime;
use regex_lite::Regex;
use std::collections::HashSet;
use std::fs::DirEntry;
use std::path::PathBuf;

const ISO_DATETIME_REGEX_STR: &str = r"\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}";
pub const ISO_DATETIME_FORMAT: &str = "%Y-%m-%d_%H-%M-%S";

pub fn cleanup_old_backups(app_config: &AppConfig) {
    let Some(retention_config) = app_config.retention.as_ref() else {
        return;
    };

    let backup_files = list_backup_files(app_config);
    let now = chrono::Utc::now();
    let mut files_to_delete: HashSet<PathBuf> = HashSet::new();

    if let Some(keep_last) = retention_config.keep_last {
        files_to_delete.extend(
            backup_files
                .iter()
                .rev()
                .skip(keep_last)
                .map(|e| e.entry.path()),
        );
    }

    backup_files
        .iter()
        .for_each(|file| debug_log!("File date: {}", file.date));

    if let Some(max_age) = retention_config.max_age {
        let cutoff = now - max_age;
        debug_log!("Backup cleanup by age: now: {now}, cutoff date: {cutoff}");
        files_to_delete.extend(
            backup_files
                .iter()
                .filter(|file| file.date < cutoff)
                .map(|e| e.entry.path()),
        );
    }

    if !files_to_delete.is_empty() {
        log!("Deleting {} old backup files", files_to_delete.len());
        for file in files_to_delete {
            if let Err(e) = trash::delete(file) {
                log!("Failed to delete old backup file: {e}");
            }
        }
    }
}

struct BackupFile {
    entry: DirEntry,
    date: chrono::DateTime<chrono::Local>,
}

fn list_backup_files(app_config: &AppConfig) -> Vec<BackupFile> {
    let filename_regex = Regex::new(&format!(
        "^{}({})\\.(?:{})$",
        regex_lite::escape(&app_config.archive_name_prefix),
        ISO_DATETIME_REGEX_STR,
        CompressionAlgorithm::ALL_EXTENSIONS.join("|")
    ))
    .expect("Failed to compile regex for filename pattern");

    let Ok(a) = std::fs::read_dir(&app_config.output_folder)
        .inspect_err(|e| log!("Failed to read output folder: {e}"))
    else {
        return Vec::new();
    };

    let mut backup_files: Vec<_> = a
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let filename = entry.file_name().to_str().unwrap().to_owned();
            let caps = filename_regex.captures(&filename)?;
            let date = parse_iso_datetime(&caps[1])?;
            Some(BackupFile { entry, date })
        })
        .collect();

    backup_files.sort_by_key(|file| file.date);
    debug_log!("Found {} backup files", backup_files.len());

    backup_files
}

pub fn last_backup_time(app_config: &AppConfig) -> Option<chrono::DateTime<chrono::Local>> {
    list_backup_files(app_config).last().map(|file| file.date)
}

fn parse_iso_datetime(date_str: &str) -> Option<chrono::DateTime<chrono::Local>> {
    NaiveDateTime::parse_from_str(date_str, ISO_DATETIME_FORMAT)
        .ok()?
        .and_local_timezone(chrono::Local)
        .single()
}
