use crate::config::{AppConfig, CompressionMethod, CompressionOptions, TargetConfig};
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
    let start_time = Instant::now();
    let archive_path = build_archive_path(app_config);
    let mut archive_writer = create_archive_writer(archive_path.as_path(), app_config)
        .expect("Could not create archive writer");
    log!("Starting backup to '{}'", archive_path.display());

    for target_config in &app_config.targets {
        if let Err(err) =
            run_backup_for_target(target_config, &app_config.compression, &mut archive_writer)
        {
            log!(
                "Error running backup for target '{}': {}",
                target_config.path.display(),
                err
            );
            // Best effort cleanup
            let _ = archive_writer.finish();
            let _ = std::fs::remove_file(archive_path);
            return;
        }
    }

    archive_writer
        .finish()
        .expect("Could not finish archive writer");
    log!(
        "Backup completed successfully in {}s",
        (start_time.elapsed().as_millis() as f64 / 100.0).floor() / 10.0
    );
}

fn run_backup_for_target(
    target_config: &TargetConfig,
    compression_options: &CompressionOptions,
    archive_writer: &mut ArchiveWriter<File>,
) -> Result<()> {
    let base_path = target_config.path.as_path();
    if !base_path.exists() {
        bail!("Target path does not exist");
    }
    if !base_path.is_dir() {
        bail!("Error: target path is not a directory");
    }
    log!(
        "target_config.archive_path = {}",
        target_config.archive_path.display()
    );

    let compression_methods = create_compression_methods(compression_options);
    let copy_methods = create_copy_methods();
    let mut copy_mode = false;
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
            handle_excluded_entry(&mut walker, entry_relative_path, is_folder);
            continue;
        }
        if is_folder {
            continue;
        }

        let file_name = target_config
            .archive_path
            .join(entry_relative_path)
            .to_str()
            .expect("Could not convert path to string")
            .to_owned();

        copy_mode = handle_smart_copy_compression_switch(
            archive_writer,
            copy_mode,
            entry_full_path,
            &compression_methods,
            &copy_methods,
            target_config,
        );
        log!("Adding file '{}' to compressed file", file_name);

        archive_writer.push_archive_entry(
            ArchiveEntry::from_path(entry_full_path, file_name),
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
    let option = match &compression_options.method {
        CompressionMethod::Deflate => {
            encoder_options::DeflateOptions::from_level(compression_options.level as u32).into()
        }
        CompressionMethod::LZMA2 => {
            encoder_options::Lzma2Options::from_level(compression_options.level as u32).into()
        }
        CompressionMethod::PPMd => {
            encoder_options::PpmdOptions::from_level(compression_options.level as u32).into()
        }
    };
    vec![option]
}

fn create_copy_methods() -> Vec<EncoderConfiguration> {
    vec![EncoderMethod::COPY.into()]
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

fn handle_smart_copy_compression_switch(
    archive_writer: &mut ArchiveWriter<File>,
    current_copy_mode: bool,
    entry_full_path: &Path,
    compression_methods: &Vec<EncoderConfiguration>,
    copy_methods: &Vec<EncoderConfiguration>,
    target_config: &TargetConfig,
) -> bool {
    let copy_without_compression = should_copy_without_compression(entry_full_path, target_config);

    if current_copy_mode != copy_without_compression {
        let methods = if copy_without_compression {
            copy_methods.clone()
        } else {
            compression_methods.clone()
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

fn should_copy_without_compression(path: &Path, target_config: &TargetConfig) -> bool {
    if !target_config.smart_copy_compressed_files {
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
    let now = chrono::Utc::now();
    let filename = format!(
        "{}{}.{}",
        app_config.compressed_file_name_prefix,
        now.format("%Y-%m-%d_%H-%M-%S"),
        app_config.compression.method.extension()
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
    File::create(destination)?;

    let mut writer = ArchiveWriter::create(destination)?;
    writer.set_content_methods(create_compression_methods(&app_config.compression));
    Ok(writer)
}
