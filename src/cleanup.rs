use crate::config::{AppConfig, CompressionAlgorithm};
use crate::duplicate::{promote_symlink, resolve_real_backup};
use crate::{debug_log, log};
use chrono::NaiveDateTime;
use regex_lite::Regex;
use std::collections::HashSet;
use std::fs;
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
        .for_each(|file| debug_log!("Backup file date: {}", file.date));

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

    let targets_to_delete: Vec<_> = files_to_delete.iter().cloned().collect();
    for target in targets_to_delete {
        if !fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_file()) {
            continue;
        }
        let real_target = match resolve_real_backup(&target, &app_config.output_folder) {
            Ok(target) => target,
            Err(error) => {
                log!(
                    "Could not inspect backup '{}', retaining it: {error}",
                    target.display()
                );
                files_to_delete.remove(&target);
                continue;
            }
        };
        let mut dependent_links: Vec<_> = backup_files
            .iter()
            .filter(|backup| !files_to_delete.contains(&backup.entry.path()))
            .filter(|backup| {
                fs::symlink_metadata(backup.entry.path())
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
            })
            .filter(|backup| {
                resolve_real_backup(&backup.entry.path(), &app_config.output_folder)
                    .is_ok_and(|real| real == real_target)
            })
            .collect();
        if dependent_links.is_empty() {
            continue;
        }
        dependent_links.sort_by_key(|backup| backup.date);
        let promoted = dependent_links.remove(0).entry.path();
        let other_links: Vec<_> = dependent_links
            .iter()
            .map(|backup| backup.entry.path())
            .collect();
        if let Err(error) = promote_symlink(&target, &promoted, &other_links) {
            log!(
                "Could not promote backup link, retaining '{}': {error}",
                target.display()
            );
            files_to_delete.remove(&target);
        }
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
    latest_backup(app_config).map(|(_, date)| date)
}

pub fn latest_backup(app_config: &AppConfig) -> Option<(PathBuf, chrono::DateTime<chrono::Local>)> {
    list_backup_files(app_config)
        .into_iter()
        .rev()
        .find(|file| file.entry.path().is_file())
        .map(|file| (file.entry.path(), file.date))
}

fn parse_iso_datetime(date_str: &str) -> Option<chrono::DateTime<chrono::Local>> {
    NaiveDateTime::parse_from_str(date_str, ISO_DATETIME_FORMAT)
        .ok()?
        .and_local_timezone(chrono::Local)
        .single()
}
