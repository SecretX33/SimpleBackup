use crate::config::{AppConfig, DuplicateBackupAction};
use crate::log;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub fn handle_duplicate(
    config: &AppConfig,
    new_backup: &Path,
    previous_backup: &(PathBuf, chrono::DateTime<chrono::Local>),
) {
    let Some(action) = config.duplicate_backup_action else {
        return;
    };

    let matches = match archives_match(new_backup, &previous_backup.0) {
        Ok(matches) => matches,
        Err(error) => {
            log!("Could not compare backups, keeping the new archive: {error}");
            return;
        }
    };
    if !matches {
        return;
    }

    if matches!(action, DuplicateBackupAction::Skip)
        && config
            .retention
            .as_ref()
            .and_then(|retention| retention.max_age)
            .is_some_and(|max_age| previous_backup.1 < chrono::Local::now() - max_age)
    {
        log!("Previous backup has reached max_age, keeping the new archive");
        return;
    }

    let result = match action {
        DuplicateBackupAction::Skip => trash::delete(new_backup).map_err(io::Error::other),
        DuplicateBackupAction::HardLink | DuplicateBackupAction::SymbolicLink => {
            resolve_real_backup(&previous_backup.0, &config.output_folder)
                .and_then(|target| replace_with_link(new_backup, &target, action))
        }
    };
    match result {
        Ok(()) => log!("Duplicate backup handled with {:?}", action),
        Err(error) => log!("Could not handle duplicate backup, keeping the full archive: {error}"),
    }
}

fn archives_match(first: &Path, second: &Path) -> io::Result<bool> {
    if fs::metadata(first)?.len() != fs::metadata(second)?.len() {
        return Ok(false);
    }
    Ok(hash_archive(first)? == hash_archive(second)?)
}

fn hash_archive(path: &Path) -> io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(hasher.finalize())
}

pub fn resolve_real_backup(path: &Path, output_folder: &Path) -> io::Result<PathBuf> {
    let resolved = fs::canonicalize(path)?;
    let output_folder = fs::canonicalize(output_folder)?;
    if resolved.parent() != Some(output_folder.as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Backup link points outside the output folder",
        ));
    }
    if !fs::metadata(&resolved)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Backup is not a file",
        ));
    }
    Ok(resolved)
}

fn replace_with_link(
    new_backup: &Path,
    target: &Path,
    action: DuplicateBackupAction,
) -> io::Result<()> {
    let staged_link = staging_path(new_backup, "new-link")?;
    let staged_backup = staging_path(new_backup, "full")?;
    let result = match action {
        DuplicateBackupAction::HardLink => fs::hard_link(target, &staged_link),
        DuplicateBackupAction::SymbolicLink => {
            create_symlink(target.file_name().unwrap().as_ref(), &staged_link)
        }
        _ => unreachable!(),
    };
    result?;
    if let Err(error) = fs::rename(new_backup, &staged_backup) {
        let _ = fs::remove_file(staged_link);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staged_link, new_backup) {
        let _ = fs::rename(&staged_backup, new_backup);
        let _ = fs::remove_file(staged_link);
        return Err(error);
    }
    if let Err(error) = fs::remove_file(&staged_backup) {
        log!(
            "Could not remove redundant staged archive '{}': {error}",
            staged_backup.display()
        );
    }
    Ok(())
}

pub fn promote_symlink(target: &Path, promoted: &Path, other_links: &[PathBuf]) -> io::Result<()> {
    let staged_real = staging_path(promoted, "real")?;
    if fs::hard_link(target, &staged_real).is_err()
        && let Err(error) = fs::copy(target, &staged_real)
    {
        let _ = fs::remove_file(&staged_real);
        return Err(error);
    }

    let staged_link = staging_path(promoted, "link")?;
    if let Err(error) = fs::rename(promoted, &staged_link) {
        let _ = fs::remove_file(&staged_real);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staged_real, promoted) {
        let _ = fs::rename(&staged_link, promoted);
        let _ = fs::remove_file(&staged_real);
        return Err(error);
    }
    fs::remove_file(staged_link)?;

    for link in other_links {
        repoint_symlink(link, promoted)?;
    }
    Ok(())
}

fn repoint_symlink(link: &Path, new_target: &Path) -> io::Result<()> {
    let staged_new = staging_path(link, "new-link")?;
    create_symlink(new_target.file_name().unwrap().as_ref(), &staged_new)?;
    let staged_old = staging_path(link, "old-link")?;
    if let Err(error) = fs::rename(link, &staged_old) {
        let _ = fs::remove_file(staged_new);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staged_new, link) {
        let _ = fs::rename(&staged_old, link);
        let _ = fs::remove_file(staged_new);
        return Err(error);
    }
    fs::remove_file(staged_old)
}

fn staging_path(path: &Path, purpose: &str) -> io::Result<PathBuf> {
    let name = format!(
        "{}.{}-{}",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id(),
        purpose
    );
    let staged = path.with_file_name(name);
    match fs::symlink_metadata(&staged) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Backup staging path already exists",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(staged),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
