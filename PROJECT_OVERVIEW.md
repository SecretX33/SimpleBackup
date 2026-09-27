# SimpleBackup Project Overview

## What this project is

SimpleBackup is a cross-platform command-line backup utility written in Rust. It reads a JSON configuration file, collects files from one or more source directories, and stores them in a timestamped 7z archive.

The project is designed for repeatable, configuration-driven backups. It can be run manually or invoked by an external scheduler such as Windows Task Scheduler, cron, or a systemd timer. SimpleBackup itself performs one backup check per execution and then exits.

## What you can do with it

From a user's perspective, SimpleBackup lets you describe a backup job once in a JSON file and run that job whenever needed. You can:

- Back up personal documents, photos, project folders, application data, or any other local directory.
- Combine several folders into one archive while choosing where each folder appears inside it.
- Back up only the file types you care about, such as PDF documents or source code.
- Leave out temporary files, caches, build output, downloads, or other unwanted content.
- Control how deeply SimpleBackup searches through a directory tree.
- Choose between faster or smaller archives by changing the compression algorithm and level.
- Save time and processing power by storing files such as JPEG images, videos, and ZIP archives without attempting to compress them again.
- Prevent overly frequent backups by defining a minimum time between successful archive timestamps.
- Keep storage usage under control by retaining only a chosen number of recent backups or removing backups after a chosen age.
- Optionally detect identical 7z archives and link to existing data or skip the new backup.
- Recover files with any compatible 7z extraction application.

### Example uses

- Run a nightly backup of your documents through Windows Task Scheduler or cron.
- Keep the latest 14 backups and automatically move older archives to the system trash.
- Archive multiple development projects while excluding directories such as `target`, `node_modules`, or generated build output.
- Collect selected file types from a large folder by using include patterns such as `**/*.pdf` or `**/*.rs`.
- Put backups on another local drive, a mounted network share, or a synchronized cloud-storage folder by setting that location as `output_folder`.

SimpleBackup reports its progress in the terminal, including skipped runs, files being added, source errors, completion time, and retention cleanup problems. If a source cannot be backed up, it makes a best-effort attempt to remove the incomplete archive so that it is not mistaken for a finished backup.

## Main capabilities

- Backs up one or more directories into a single archive.
- Places each source at a configurable path inside the archive.
- Selects files with case-insensitive include and exclude glob patterns.
- Limits directory traversal by minimum and maximum depth.
- Configures symbolic link traversal globally or per source.
- Supports Deflate, LZMA2, and PPMd compression at levels 0 through 9.
- Avoids recompressing formats that are usually already compressed, such as images, videos, office documents, and existing archives.
- Skips a run when the configured minimum interval has not elapsed since the latest recognized backup.
- Removes old backups according to a retained count, a maximum age, or both.
- Sends removed archives to the operating system trash instead of permanently deleting them.
- Configures creation, modification, and access timestamps independently, with defaults that improve byte-for-byte duplicate matching when duplicate handling is enabled.

## How it works

The executable expects the configuration file path as its first argument:

```shell
simplebackup path/to/config.json
```

During a run, SimpleBackup performs the following work:

1. Resolves and validates the JSON configuration.
2. Finds existing archives whose names match the configured prefix and timestamp format.
3. Skips archive creation if `min_backup_interval` is set and a recent backup already exists.
4. Walks each source directory while applying depth, symbolic link, include, and exclude rules.
5. Writes matching files to a new 7z archive using the selected compression algorithm and timestamp settings.
6. If duplicate handling is enabled, hashes the finished archive and compares it with the latest recognized backup.
7. Removes an incomplete archive if processing a source fails.
8. Applies retention rules to recognized archives after the backup attempt, promoting symbolic links when their original archive is removed.

Archives use the following naming convention:

```text
<archive_name_prefix>YYYY-MM-DD_HH-MM-SS.7z
```

With the default prefix, an example filename is `backup_2026-08-30_14-25-10.7z`.

## Configuration example

```json
{
  "output_folder": "path/to/backups",
  "sources": [
    {
      "path": "path/to/documents",
      "path_in_archive": "documents",
      "include": ["**/*.txt", "**/*.pdf"],
      "exclude": ["temporary/**"],
      "follow_symlinks": false,
      "skip_recompression_for_known_formats": true
    }
  ],
  "min_backup_interval": "12h",
  "archive_name_prefix": "documents_",
  "duplicate_backup_action": "HardLink",
  "compression": {
    "algorithm": "lzma2",
    "level": 7
  },
  "retention": {
    "keep_last": 25,
    "max_age": "90days"
  }
}
```

Relative paths are resolved from the process working directory, not from the directory containing the configuration file. The output directory cannot be inside a source directory because that could cause the backup to include itself recursively.

Exclude patterns take precedence over include patterns. Patterns are relative to their source directory, `?` matches one non-separator character, `*` matches within one path segment, and `**` can cross path separators.

## Retention behavior

SimpleBackup only manages files in the output directory that match its configured archive prefix, timestamp format, and supported extension. `keep_last` retains the newest matching archives, while `max_age` removes matching archives older than the specified duration. When both options are present, a file selected by either rule is moved to trash.

The optional `duplicate_backup_action` accepts `Skip`, `SymbolicLink`, or `HardLink` without regard to case. When omitted, every due backup is saved without hashing. When set, it compares complete archive hashes with the latest backup. The optional `include_timestamps` setting accepts a Boolean for all three timestamps or an object with `creation`, `modification`, and `access` fields. All three default to `true` without duplicate handling. With duplicate handling, creation defaults to `true` and modification and access default to `false`. Explicit fields override those defaults independently, and restored files cannot retain timestamps omitted from the archive.

## Project structure

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Application entry point and top-level backup workflow. |
| `src/config.rs` | JSON parsing, defaults, validation, and resolved configuration types. |
| `src/backup.rs` | Directory traversal, file selection, archive creation, and compression behavior. |
| `src/cleanup.rs` | Existing backup discovery, interval checks, and retention cleanup. |
| `src/duplicate.rs` | Archive hash comparison, duplicate actions, and symbolic-link promotion. |
| `src/path_glob.rs` | Cross-platform, case-insensitive path glob parsing and matching. |
| `src/util.rs` | Path normalization and shared path utilities. |
| `src/log_macros.rs` | Lightweight application logging macros. |
| `Cargo.toml` | Rust package metadata, dependencies, and build profiles. |

## Technology

SimpleBackup targets the Rust 2024 edition. Its main dependencies provide JSON deserialization, human-readable durations, directory walking, regular-expression matching, error reporting, 7z archive writing, BLAKE3 hashing, local timestamps, and operating system trash integration.

The release profile enables optimization, link-time optimization, symbol stripping, and abort-on-panic behavior to produce a compact standalone executable.

## Building from source

Install the stable Rust toolchain, clone the repository, and run:

```shell
cargo build --release
```

The optimized executable is produced under `target/release`.

## Intended use

SimpleBackup is best suited to straightforward local or mounted-storage backup jobs where configuration should remain readable and versionable. It creates full archives with optional duplicate handling rather than incremental backups, and it does not include a built-in scheduler, remote transport, encryption configuration, or restore command. Restoration is performed with a compatible 7z extraction tool.

## License

The project is distributed under the MIT License. See `LICENSE` for the full terms.
