# SimpleBackup

[![CI](https://github.com/SecretX33/SimpleBackup/actions/workflows/build-and-release.yml/badge.svg)](https://github.com/SecretX33/SimpleBackup/actions/workflows/build-and-release.yml)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/SecretX33/SimpleBackup)](https://github.com/SecretX33/SimpleBackup/releases/latest)
[![GitHub License](https://img.shields.io/github/license/SecretX33/SimpleBackup)](https://github.com/SecretX33/SimpleBackup/blob/master/LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)

SimpleBackup creates a compressed archive from one or more source directories using a JSON configuration file.

## Download

Available for Windows, Linux, and macOS. Download the [latest release](https://github.com/SecretX33/SimpleBackup/releases/latest) or browse [all releases](https://github.com/SecretX33/SimpleBackup/releases).

## Usage

```bash
simplebackup <path/to/config.json>
```

## Configuration options

### Minimum configuration

```json
{
  "output_folder": "path/to/backups",
  "sources": [
    {
      "path": "path/to/documents"
    }
  ]
}
```

Note: relative paths are resolved from the directory where the app is run, not from the directory containing the configuration file.

### Full configuration

```json
{
  "output_folder": "path/to/backups",
  "sources": [
    {
      "path": "path/to/documents",
      "path_in_archive": "some/folder/mydocuments",
      "include": ["*.txt", "**/*.txt", "*.pdf", "**/*.pdf"],
      "exclude": ["temporary/**"],
      "min_depth": 1,
      "max_depth": 10,
      "follow_symlinks": true,
      "skip_recompression_for_known_formats": false
    }
  ],
  "schedule": {
    "period": "daily"
  },
  "archive_name_prefix": "documents_",
  "duplicate_backup_action": "HardLink",
  "include_timestamps": {
    "creation": true,
    "modification": false,
    "access": false
  },
  "follow_symlinks": false,
  "skip_recompression_for_known_formats": true,
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

### Options

Top-level options:

| Option | Required | Default | Description |
| --- | --- | --- | --- |
| `output_folder` | Yes |                 | Directory where archives are created. |
| `sources` | Yes |                 | Non-empty list of source directory configurations. |
| `schedule` | No | Every invocation | Calendar period or minimum elapsed interval between successful backups. |
| `follow_symlinks` | No | `false`         | Global default for following symbolic links while walking sources. |
| `skip_recompression_for_known_formats` | No | `false`         | Stores commonly compressed file formats without recompressing them. |
| `retention` | No |                 | Rules for removing older recognized archives. |
| `archive_name_prefix` | No | `backup_`       | Prefix added before the archive timestamp. It must not be empty. |
| `compression` | No | Deflate, level 5 | Compression algorithm and level. |
| `duplicate_backup_action` | No | Disabled | Case-insensitive action for an archive identical to the latest backup: `Skip`, `SymbolicLink`, or `HardLink`. |
| `include_timestamps` | No | Depends on duplicate handling | Boolean for all source file timestamps, or an object controlling each timestamp. |

Each item in `sources` accepts:

| Option | Required | Default | Description |
| --- | --- | --- | --- |
| `path` | Yes | | Source directory to scan. |
| `path_in_archive` | No | Derived from the source paths | Base path used for this source inside the archive. |
| `include` | No | Include everything | List of relative path globs to include. |
| `exclude` | No | Exclude nothing | List of relative path globs to exclude. Exclusions take precedence over inclusions. |
| `min_depth` | No | No minimum | Minimum walk depth, where the source directory is depth 0. |
| `max_depth` | No | No maximum | Maximum walk depth. It must be greater than or equal to `min_depth`. |
| `follow_symlinks` | No | Global value | Overrides the global symbolic link setting for this source. |
| `skip_recompression_for_known_formats` | No | Global value | Overrides the global recompression setting for this source. |

Glob patterns are matched against paths relative to their source: `?` matches one non-separator character, `*` matches within one path segment, and `**` matches across path separators. Matching is case-insensitive.

### Backup schedule

Choose either `period` or `min_interval`. Omit `schedule` to back up on every invocation.

`period` backs up once per period using the computer's local calendar:

```json
"schedule": {
  "period": "daily"
}
```

| Period | A new period begins |
| --- | --- |
| `hourly` | At the start of each local clock hour. |
| `daily` | At local midnight. |
| `weekly` | On Monday at local midnight. |
| `monthly` | On the first day of each month at local midnight. |
| `yearly` | On January 1 at local midnight. |

`min_interval` sets the minimum time between successful backups, using durations such as `30m`, `12h`, or `7days`:

```json
"schedule": {
  "min_interval": "23h"
}
```

Run SimpleBackup through Windows Task Scheduler, cron, or another scheduler; it checks once and exits. Failed backups remain eligible for retry. An unchanged backup handled with `Skip` still counts as successful.

Use different archive prefixes for jobs sharing an output folder. Scheduling history is saved in `<archive_name_prefix>schedule.json`.

### Retention

The `retention` object accepts either or both options:

| Option | Values and purpose |
| --- | --- |
| `keep_last` | Non-negative integer; keep the newest N archives. |
| `max_age` | Duration string, such as `"30days"`; remove older archives. |

Removed archives go to the operating system's trash; empty it to reclaim space. With both rules set, either can remove an archive. Only archives matching the configured prefix and filename format are managed.

### Compression

When setting `compression`, provide both fields:

| Option | Accepted values |
| --- | --- |
| `algorithm` | `"deflate"`, `"lzma2"`, or `"ppmd"` (case-insensitive) |
| `level` | Integer from `0` through `9` |

### Duplicate backups

`duplicate_backup_action` compares the new archive with the latest backup. Values are case-insensitive:

| Value | Action for an identical archive |
| --- | --- |
| `Skip` | Move the new archive to trash. |
| `SymbolicLink` | Replace the new archive with a symbolic link to existing data. |
| `HardLink` | Replace the new archive with a hard link to existing data. |

Archives must match byte for byte, including stored timestamps. If comparison or linking fails, the full archive is kept. `Skip` also keeps the new archive if the previous one has reached `max_age`.

### File timestamps

`include_timestamps` accepts `true` or `false` for all timestamps, or an object with optional `creation`, `modification`, and `access` Boolean fields. Explicit values override these defaults:

| Field | Without duplicate handling | With duplicate handling |
| --- | --- | --- |
| `creation` | `true` | `true` |
| `modification` | `true` | `false` |
| `access` | `true` | `false` |

Omitting timestamps improves duplicate matching, but restored files cannot retain omitted timestamps.

## Building from Source

- Install [Rust](https://www.rust-lang.org/tools/install).
- Build the binary by executing this command, the compiled file will be in the `target/[debug|release]` folder.

```shell
# For development build
cargo build

# For release (optimized) build
cargo build --release
```

## License

[MIT](LICENSE).
