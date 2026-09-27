use crate::cleanup::{ISO_DATETIME_FORMAT, last_backup_time, latest_backup};
use crate::config::{AppConfig, CompressionAlgorithm, CompressionOptions, SourceConfig};
use crate::duplicate::handle_duplicate;
use crate::{debug_log, log};
use color_eyre::eyre::bail;
use color_eyre::{Result, eyre};
use sevenz_rust2::{ArchiveEntry, encoder_options};
use sevenz_rust2::{ArchiveWriter, EncoderConfiguration, EncoderMethod};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::{IntoIter, WalkDir};

pub fn run_backup(app_config: &AppConfig) {
    if !backup_is_due(app_config) {
        log!("Backup is not due yet, skipping...");
        return;
    }

    let start_time = Instant::now();
    let previous_backup = if app_config.duplicate_backup_action.is_some() {
        latest_backup(app_config)
    } else {
        None
    };
    let archive_path = build_archive_path(app_config);
    let mut archive_writer = match create_archive_writer(archive_path.as_path(), app_config) {
        Ok(writer) => writer,
        Err(error) => {
            log!("Could not create archive writer: {error}");
            return;
        }
    };
    log!("Starting backup to '{}'", archive_path.display());

    for source_config in &app_config.sources {
        if let Err(err) = run_backup_for_source(
            source_config,
            &app_config.compression,
            app_config.store_creation_and_modification_times,
            &mut archive_writer,
        ) {
            log!(
                "Error running backup for source '{}': {}",
                source_config.path.display(),
                err
            );
            // Best effort cleanup
            let _ = archive_writer.finish();
            let _ = std::fs::remove_file(archive_path);
            return;
        }
    }

    if let Err(error) = archive_writer.finish() {
        log!("Could not finish archive writer: {error}");
        let _ = std::fs::remove_file(&archive_path);
        return;
    }
    if let Some(previous_backup) = previous_backup {
        handle_duplicate(app_config, &archive_path, &previous_backup);
    }
    log!(
        "Backup completed successfully in {}s",
        (start_time.elapsed().as_millis() as f64 / 100.0).floor() / 10.0
    );
}

fn backup_is_due(app_config: &AppConfig) -> bool {
    let Some(backup_interval) = app_config.min_backup_interval else {
        return true;
    };
    let Some(last_backup_date) = last_backup_time(app_config) else {
        return true;
    };

    let now = chrono::Local::now();
    let cutoff_date = now - backup_interval;
    last_backup_date < cutoff_date
}

fn run_backup_for_source(
    source_config: &SourceConfig,
    compression_options: &CompressionOptions,
    store_creation_and_modification_times: bool,
    archive_writer: &mut ArchiveWriter<File>,
) -> Result<()> {
    let base_path = source_config.path.as_path();
    if !base_path.exists() {
        bail!("Source path does not exist");
    }
    if !base_path.is_dir() {
        bail!("Error: source path is not a directory");
    }
    log!(
        "source_config.path_in_archive = {}",
        source_config.path_in_archive.display()
    );

    let compression_methods = create_compression_methods(compression_options);
    let copy_methods = create_copy_methods();
    let mut copy_mode = false;
    let mut walker = walk_folder(source_config).into_iter();

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

        if is_excluded(
            source_config,
            &entry_relative_path.to_string_lossy(),
            is_folder,
        ) {
            handle_excluded_entry(&mut walker, entry_relative_path, is_folder);
            continue;
        }
        if is_folder {
            continue;
        }

        let file_name = source_config
            .path_in_archive
            .join(entry_relative_path)
            .to_str()
            .expect("Could not convert path to string")
            .to_owned();

        copy_mode = handle_recompression_switch(
            archive_writer,
            copy_mode,
            entry_full_path,
            &compression_methods,
            &copy_methods,
            source_config,
        );
        log!("Adding file '{}' to compressed file", file_name);

        let mut archive_entry = ArchiveEntry::from_path(entry_full_path, file_name);
        archive_entry.has_access_date = false;
        if !store_creation_and_modification_times {
            archive_entry.has_creation_date = false;
            archive_entry.has_last_modified_date = false;
        }

        archive_writer.push_archive_entry(
            archive_entry,
            Some(File::open(entry_full_path).map_err(|e| {
                eyre::eyre!("Could not open file '{}': {}", entry_full_path.display(), e)
            })?),
        )?;
    }

    Ok(())
}

fn create_compression_methods(
    compression_options: &CompressionOptions,
) -> Vec<EncoderConfiguration> {
    let option = match &compression_options.algorithm {
        CompressionAlgorithm::Deflate => {
            encoder_options::DeflateOptions::from_level(compression_options.level as u32).into()
        }
        CompressionAlgorithm::LZMA2 => {
            encoder_options::Lzma2Options::from_level(compression_options.level as u32).into()
        }
        CompressionAlgorithm::PPMd => {
            encoder_options::PpmdOptions::from_level(compression_options.level as u32).into()
        }
    };
    vec![option]
}

fn create_copy_methods() -> Vec<EncoderConfiguration> {
    vec![EncoderMethod::COPY.into()]
}

fn walk_folder(config: &SourceConfig) -> WalkDir {
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

fn is_excluded(config: &SourceConfig, relative_path: &str, is_folder: bool) -> bool {
    if config
        .exclude
        .as_ref()
        .is_some_and(|set| set.is_match(relative_path))
    {
        return true;
    }
    config.include.as_ref().is_some_and(|set| {
        if is_folder {
            !set.accepts_prefix(relative_path)
        } else {
            !set.is_match(relative_path)
        }
    })
}

fn handle_excluded_entry(walker: &mut IntoIter, entry_relative_path: &Path, is_folder: bool) {
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
}

fn handle_recompression_switch(
    archive_writer: &mut ArchiveWriter<File>,
    current_copy_mode: bool,
    entry_full_path: &Path,
    compression_methods: &[EncoderConfiguration],
    copy_methods: &[EncoderConfiguration],
    source_config: &SourceConfig,
) -> bool {
    let copy_without_compression = should_copy_without_compression(entry_full_path, source_config);

    if current_copy_mode != copy_without_compression {
        let methods = if copy_without_compression {
            copy_methods.to_owned()
        } else {
            compression_methods.to_owned()
        };
        archive_writer.set_content_methods(methods);
    }
    copy_without_compression
}

const USUALLY_COMPRESSED_EXTENSIONS: &[&str] = &[
    "3gp", "7z", "aab", "aac", "ace", "ape", "apk", "appx", "avi", "avif", "br", "bz2", "cab",
    "deb", "docx", "dotx", "ear", "epub", "flac", "flv", "gif", "gz", "heic", "heif", "ipa", "j2k",
    "jar", "jp2", "jpeg", "jpg", "jxl", "key", "lha", "lz", "lz4", "lzh", "m2ts", "m4a", "m4v",
    "mka", "mkv", "mov", "mp3", "mp4", "mpeg", "mpg", "msix", "mts", "numbers", "nupkg", "odp",
    "ods", "odt", "oga", "ogg", "ogv", "opus", "pages", "png", "potx", "pptx", "rar", "rpm", "rz",
    "tbz", "tbz2", "tgz", "ts", "txz", "tzst", "war", "webm", "webp", "wma", "wmv", "woff",
    "woff2", "xlsx", "xltx", "xpi", "xz", "z", "zip", "zipx", "zst",
];

fn should_copy_without_compression(path: &Path, source_config: &SourceConfig) -> bool {
    if !source_config.skip_recompression_for_known_formats {
        return false;
    }
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    USUALLY_COMPRESSED_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn build_archive_path(app_config: &AppConfig) -> PathBuf {
    let now = chrono::Local::now();
    let filename = format!(
        "{}{}.{}",
        app_config.archive_name_prefix,
        now.format(ISO_DATETIME_FORMAT),
        app_config.compression.algorithm.extension()
    );
    app_config.output_folder.join(filename)
}

fn create_archive_writer(
    destination: &Path,
    app_config: &AppConfig,
) -> Result<ArchiveWriter<File>> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create_new(destination)?;
    let mut writer = match ArchiveWriter::new(file) {
        Ok(writer) => writer,
        Err(error) => {
            let _ = std::fs::remove_file(destination);
            return Err(error.into());
        }
    };
    writer.set_content_methods(create_compression_methods(&app_config.compression));
    Ok(writer)
}
