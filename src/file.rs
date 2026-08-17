use crate::config::{AppConfig, TargetConfig};
use crate::{debug_log, log};
use color_eyre::Result;
use color_eyre::eyre::bail;
use std::path;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn run_backup(app_config: &AppConfig) {
    for target_config in &app_config.targets {
        if let Err(err) = run_backup_for_target(target_config) {
            log!("Error running backup for target '{}': {}", target_config.path.display(), err);
            break;
        }
    }
}

fn run_backup_for_target(target_config: &TargetConfig) -> Result<()> {
    let base_path = target_config.path.as_path();
    if !base_path.exists() {
        bail!("Target path does not exist");
    }
    if !base_path.is_dir() {
        bail!("Error: target path is not a directory");
    }

    let mut walker = walk_folder(target_config).into_iter();

    loop {
        let entry = match walker.next() {
            None => break,
            Some(Err(err)) => {
                log!("Failed to read entry, skipping: {:?}", err);
                continue;
            }
            Some(Ok(entry)) => entry,
        };

        let entry_full_path = entry.path();
        let entry_relative_path = match entry_full_path.strip_prefix(base_path) {
            Ok(path) => path,
            Err(err) => {
                log!(
                    "Failed to make '{}' relative to '{}', skipping: {}",
                    entry_full_path.display(),
                    base_path.display(),
                    err
                );
                continue;
            }
        };
        let is_folder = entry.file_type().is_dir();

        if is_excluded(target_config, &entry_relative_path.to_string_lossy()) {
            if is_folder {
                debug_log!(
                    "Skipping entire directory: {}",
                    entry_relative_path.display()
                );
                walker.skip_current_dir();
            } else {
                debug_log!(
                    "Skipping excluded entry: {} {:?}",
                    entry_relative_path.display(),
                    entry_relative_path.parent()
                );
            }
            continue;
        }
        if is_folder {
            continue;
        }

        println!(
            "TODO: Add file '{}' to compressed file",
            entry_relative_path.display()
        );
    }

    Ok(())
}

fn walk_folder(config: &TargetConfig) -> WalkDir {
    let folder = config.path.as_path();
    let mut walk = WalkDir::new(folder).follow_links(config.follow_symlinks);
    if let Some(min_depth) = config.min_depth {
        walk = walk.min_depth(min_depth);
    }
    if let Some(max_depth) = config.max_depth {
        walk = walk.max_depth(max_depth);
    }
    walk
}

fn is_excluded(config: &TargetConfig, relative_path: &str) -> bool {
    if config
        .exclude
        .as_ref()
        .is_some_and(|set| set.is_match(relative_path))
    {
        return true;
    }
    config
        .include
        .as_ref()
        .is_some_and(|set| !set.accepts_prefix(relative_path))
}
