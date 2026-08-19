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
  "min_backup_interval": "12h",
  "archive_name_prefix": "documents_",
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
| `min_backup_interval` | No | No minimum      | Minimum time between recognized backups, such as `30m`, `12h`, or `7days`. |
| `follow_symlinks` | No | `false`         | Global default for following symbolic links while walking sources. |
| `skip_recompression_for_known_formats` | No | `false`         | Stores commonly compressed file formats without recompressing them. |
| `retention` | No |                 | Rules for removing older recognized archives. |
| `archive_name_prefix` | No | `backup_`       | Prefix added before the archive timestamp. It must not be empty. |
| `compression` | No | Deflate, level 5 | Compression algorithm and level. |

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

Retention accepts `keep_last`, which keeps the newest specified number of recognized archives, and `max_age`, which moves archives older than a duration such as `30days` to the operating system's trash. If both are provided, an archive selected by either cleanup rule is moved to the trash.

Compression accepts `algorithm` and `level`. The algorithms are `deflate`, `lzma2`, and `ppmd`, matched case-insensitively. The level must be from 0 through 9. Deflate currently produces a filename ending in `.zip`; LZMA2 and PPMd produce filenames ending in `.7z`.

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