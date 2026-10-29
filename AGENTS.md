# Project guidance

SimpleBackup is a small Rust CLI that creates compressed backups from JSON configuration files.

- Keep changes simple and consistent with the existing code.
- Never preserve compatibility with old configuration formats; this small project can require configuration updates, which the owner manages.
- Update `README.md` when behavior or configuration changes.
- For code changes, run `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings`.
