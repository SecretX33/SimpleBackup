# SimpleBackup

[![CI](https://github.com/SecretX33/SimpleBackup/actions/workflows/build-and-release.yml/badge.svg)](https://github.com/SecretX33/SimpleBackup/actions/workflows/build-and-release.yml)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/SecretX33/SimpleBackup)](https://github.com/SecretX33/SimpleBackup/releases/latest)
[![GitHub License](https://img.shields.io/github/license/SecretX33/SimpleBackup)](https://github.com/SecretX33/SimpleBackup/blob/master/LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)

SimpleBackup creates a compressed archive from one or more source directories using a JSON configuration file.

## Download

SimpleBackup is available for Windows, Linux, and MacOS.

Get the latest version [here](https://github.com/SecretX33/SimpleBackup/releases/latest). Want an older version? Check all releases [here](https://github.com/SecretX33/SimpleBackup/releases).

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

### Extended example

```json
{
  "output_folder": "path/to/backups",
  "sources": [
    {
      "path": "path/to/documents",
      "path_in_archive": "some/folder/mydocuments",
      "include": [
        "*.txt",
        "**/*.txt",
        "*.pdf",
        "**/*.pdf"
      ],
      "exclude": [
        "temporary/**"
      ],
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

Top-level options:

| Option | Required | Default         | Description |
| --- | --- |-----------------| --- |
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

Glob patterns are matched against paths relative to their source. `?` matches one non-separator character, `*` matches within one path segment, and `**` can match across path separators. Matching is case-insensitive.

### Backup schedule

Omit `schedule` to create a backup on every invocation. Otherwise, provide exactly one of `period` or `min_interval`. Empty schedule objects, both options together, unknown fields, and invalid values are rejected. The former top-level `min_backup_interval` option is no longer accepted.

Use a calendar period to back up once per local calendar period:

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

A successful daily backup yesterday at 18:00 allows another today at 08:00. Further invocations today skip the backup. Calendar periods use the computer's local time zone and calendar boundaries, so a day is not necessarily 24 hours during daylight saving changes. A repeated local clock hour counts as the same hourly period.

Use an elapsed interval when the full duration must pass between successful backups:

```json
"schedule": {
  "min_interval": "23h"
}
```

`min_interval` accepts durations such as `30m`, `12h`, and `7days`. A backup becomes due when at least that duration has elapsed. SimpleBackup checks once per invocation; use Windows Task Scheduler, cron, or another external scheduler to invoke it. Missed periods produce one current backup on the next invocation. Failed backups do not satisfy the schedule and can be retried on the next invocation. A backup that spans a period boundary satisfies the period in which it completes.

Successful scheduled runs save their completion time and retained archive name in `<archive_name_prefix>schedule.json` in the output folder, such as `backup_schedule.json`. An unchanged backup handled with `duplicate_backup_action: "Skip"` also satisfies the schedule while its previous archive remains available. State is shared by configurations using the same output folder and archive prefix, so use different prefixes for separate jobs. If the state is missing, invalid, or refers to a different archive than the latest recognized backup, scheduling falls back to the latest archive's filename timestamp. Without any recognized backup, a backup is always due. State read or write failures are logged without discarding the backup. Retention applies to archives, not this state file.

### Retention and duplicate handling

Retention accepts `keep_last`, which keeps the newest specified number of recognized archives, and `max_age`, which moves archives older than a duration such as `30days` to the operating system's trash. If both are provided, an archive selected by either cleanup rule is moved to the trash.

Compression accepts `algorithm` and `level`. The algorithms are `deflate`, `lzma2`, and `ppmd`, matched case-insensitively. The level must be from 0 through 9. All three produce `.7z` archives.

Omit `duplicate_backup_action` to keep every due backup without hashing. When it is set, SimpleBackup hashes the finished archive and compares it with the latest recognized backup. A matching hash means the complete archive bytes match, including stored metadata. `Skip` sends the new archive to the operating system trash, while `SymbolicLink` and `HardLink` replace it with a link to the existing data. Storage used by a skipped archive is reclaimed when the trash is emptied. If `max_age` would remove the previous backup during the same run, `Skip` keeps the new full archive. With `Skip`, no new dated backup is saved, but a successful scheduled run still updates its completion record. Retention promotes a surviving symbolic link to a real archive before removing its data source. If hashing or the duplicate action fails, the new full archive is kept.

`include_timestamps` accepts `true` to store all three timestamps or `false` to omit all three. It also accepts an object with optional Boolean `creation`, `modification`, and `access` fields. Without `duplicate_backup_action`, all three default to `true`. With duplicate handling enabled, `creation` defaults to `true` while `modification` and `access` default to `false`, which improves duplicate matches. Each field explicitly set in the object overrides its default; omitted fields keep the default for the selected duplicate mode. For example, `"include_timestamps": { "access": true }` stores creation and access times, plus modification time only when duplicate handling is disabled. Restored files cannot retain timestamps omitted from the archive. Existing archives remain usable, though their stored timestamps may keep them from matching a new archive byte for byte.

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
