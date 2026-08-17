use crate::config::{AppConfig, CompressionMethod, CompressionOptions, TargetConfig};
use crate::{debug_log, log};
use color_eyre::{eyre, Result};
use color_eyre::eyre::bail;
use sevenz_rust2::{ArchiveWriter, EncoderConfiguration};
use sevenz_rust2::{ArchiveEntry, encoder_options};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

pub fn run_backup(app_config: &AppConfig) {
    let start_time = Instant::now();
    let archive_path = build_archive_path(app_config);
    let mut archive_writer = create_archive_writer(archive_path.as_path(), app_config).expect("Could not create archive writer");
    log!("Starting backup to '{}'", archive_path.display());

    for target_config in &app_config.targets {
        if let Err(err) = run_backup_for_target(target_config, &mut archive_writer) {
            log!("Error running backup for target '{}': {}", target_config.path.display(), err);
            // Best effort cleanup
            let _ = archive_writer.finish();
            let _ = std::fs::remove_file(archive_path);
            return;
        }
    }

    archive_writer.finish().expect("Could not finish archive writer");
    log!("Backup completed successfully in {}s", (start_time.elapsed().as_millis() as f64 / 100.0).floor() / 10.0);
}

fn run_backup_for_target(
    target_config: &TargetConfig,
    archive_writer: &mut ArchiveWriter<File>
) -> Result<()> {
    let base_path = target_config.path.as_path();
    if !base_path.exists() {
        bail!("Target path does not exist");
    }
    if !base_path.is_dir() {
        bail!("Error: target path is not a directory");
    }
    log!("target_config.archive_path = {}", target_config.archive_path.display());

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

        let file_name = target_config.archive_path.join(entry_relative_path).to_str().expect("Could not convert path to string").to_owned();
        log!("Adding file '{}' to compressed file", file_name);
        archive_writer.push_archive_entry(
            ArchiveEntry::from_path(entry_full_path, file_name),
            Some(File::open(entry_full_path).map_err(|e| eyre::eyre!("Could not open file '{}': {}", entry_full_path.display(), e))?)
        )?;
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

fn build_archive_path(app_config: &AppConfig) -> PathBuf {
    let now = chrono::Utc::now();
    let filename = format!("{}{}.{}", app_config.compressed_file_name_prefix, now.format("%Y-%m-%d_%H-%M-%S"), app_config.compression.method.extension());
    app_config.output_folder.join(filename)
}

fn create_archive_writer(
    destination: &Path,
    app_config: &AppConfig,
) -> Result<ArchiveWriter<File>> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    File::create(destination)?;

    let mut writer = ArchiveWriter::create(destination)?;
    writer.set_content_methods(create_encoder_options(&app_config.compression));
    Ok(writer)
}

fn create_encoder_options(compression_options: &CompressionOptions) -> Vec<EncoderConfiguration> {
    let option = match &compression_options.method {
        CompressionMethod::Deflate => encoder_options::DeflateOptions::from_level(compression_options.level as u32).into(),
        CompressionMethod::LZMA2 => encoder_options::Lzma2Options::from_level(compression_options.level as u32).into(),
        CompressionMethod::PPMd => encoder_options::PpmdOptions::from_level(compression_options.level as u32).into(),
    };
    vec![option]
}