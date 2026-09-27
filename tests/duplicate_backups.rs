use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    output: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("simplebackup-test-{}-{nonce}", std::process::id()));
        let source = root.join("source");
        let output = root.join("backups");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(source.join("file.txt"), b"unchanged content").unwrap();
        Self {
            root,
            source,
            output,
        }
    }

    fn run(
        &self,
        action: Option<&str>,
        include_timestamps: Option<serde_json::Value>,
        retention: Option<serde_json::Value>,
    ) {
        let config = self.root.join("config.json");
        let mut value = serde_json::json!({ "output_folder": self.output, "sources": [{ "path": self.source }] });
        if let Some(action) = action {
            value["duplicate_backup_action"] = action.into();
        }
        if let Some(include_timestamps) = include_timestamps {
            value["include_timestamps"] = include_timestamps;
        }
        if let Some(retention) = retention {
            value["retention"] = retention;
        }
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_simplebackup"))
            .arg(&config)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&result.stdout).contains("Could not "),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
    }

    fn backups(&self) -> Vec<PathBuf> {
        let mut backups: Vec<_> = fs::read_dir(&self.output)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "7z"))
            .collect();
        backups.sort();
        backups
    }
}

fn all_file_times(value: bool) -> serde_json::Value {
    serde_json::json!({
        "creation": value,
        "modification": value,
        "access": value,
    })
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn access_time_changes_still_produce_a_hard_link_duplicate() {
    let fixture = Fixture::new();
    let timestamps = serde_json::json!({
        "creation": true,
        "modification": true,
        "access": false,
    });
    fixture.run(Some("hArDlInK"), Some(timestamps.clone()), None);
    let first = fixture.backups().remove(0);
    let old = fixture.output.join("backup_2020-01-01_00-00-00.7z");
    fs::rename(first, &old).unwrap();

    let source_file = fixture.source.join("file.txt");
    File::options()
        .write(true)
        .open(&source_file)
        .unwrap()
        .set_times(FileTimes::new().set_accessed(SystemTime::now() - Duration::from_secs(86_400)))
        .unwrap();
    fixture.run(Some("HardLink"), Some(timestamps), None);

    let backups = fixture.backups();
    assert_eq!(backups.len(), 2);
    let new = backups.last().unwrap();
    assert_eq!(fs::read(&old).unwrap(), fs::read(new).unwrap());
    fs::write(new, b"shared data").unwrap();
    assert_eq!(fs::read(old).unwrap(), b"shared data");
}

#[test]
fn archive_timestamp_fields_follow_configuration() {
    let fixture = Fixture::new();
    fixture.run(None, Some(all_file_times(false)), None);
    let archive = sevenz_rust2::Archive::open(fixture.backups().remove(0)).unwrap();
    let entry = &archive.files[0];
    assert!(!entry.has_access_date);
    assert!(!entry.has_creation_date);
    assert!(!entry.has_last_modified_date);
}

#[test]
fn file_time_defaults_follow_duplicate_handling() {
    let cases = [
        (None, None, true, true, true),
        (None, Some(serde_json::json!(false)), false, false, false),
        (None, Some(serde_json::json!(true)), true, true, true),
        (None, Some(serde_json::json!({})), true, true, true),
        (
            None,
            Some(serde_json::json!({ "modification": false })),
            true,
            false,
            true,
        ),
        (Some("Skip"), None, true, false, false),
        (
            Some("Skip"),
            Some(serde_json::json!(false)),
            false,
            false,
            false,
        ),
        (
            Some("Skip"),
            Some(serde_json::json!(true)),
            true,
            true,
            true,
        ),
        (
            Some("Skip"),
            Some(serde_json::json!({})),
            true,
            false,
            false,
        ),
        (
            Some("Skip"),
            Some(serde_json::json!({ "modification": true })),
            true,
            true,
            false,
        ),
        (
            Some("Skip"),
            Some(serde_json::json!({ "access": true })),
            true,
            false,
            true,
        ),
        (
            Some("Skip"),
            Some(serde_json::json!({ "creation": false })),
            false,
            false,
            false,
        ),
    ];

    for (action, timestamps, creation, modification, access) in cases {
        let fixture = Fixture::new();
        let metadata = fs::metadata(fixture.source.join("file.txt")).unwrap();
        fixture.run(action, timestamps, None);
        let archive = sevenz_rust2::Archive::open(fixture.backups().remove(0)).unwrap();
        let entry = &archive.files[0];
        assert_eq!(
            entry.has_creation_date,
            creation && metadata.created().is_ok()
        );
        assert_eq!(
            entry.has_last_modified_date,
            modification && metadata.modified().is_ok()
        );
        assert_eq!(entry.has_access_date, access && metadata.accessed().is_ok());
    }
}

#[test]
fn omitting_file_times_allows_modified_time_changes_to_match() {
    let fixture = Fixture::new();
    fixture.run(Some("HardLink"), Some(all_file_times(false)), None);
    let old = fixture.output.join("backup_2020-01-01_00-00-00.7z");
    fs::rename(fixture.backups().remove(0), &old).unwrap();
    File::options()
        .write(true)
        .open(fixture.source.join("file.txt"))
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(86_400)))
        .unwrap();
    fixture.run(Some("HardLink"), Some(all_file_times(false)), None);
    let latest = fixture
        .backups()
        .into_iter()
        .find(|path| path != &old)
        .unwrap();
    fs::write(latest, b"shared data").unwrap();
    assert_eq!(fs::read(old).unwrap(), b"shared data");
}

#[test]
fn changed_content_does_not_match_when_file_times_are_omitted() {
    let fixture = Fixture::new();
    fixture.run(Some("HardLink"), Some(all_file_times(false)), None);
    let old = fixture.output.join("backup_2020-01-01_00-00-00.7z");
    fs::rename(fixture.backups().remove(0), &old).unwrap();
    fs::write(fixture.source.join("file.txt"), b"different content").unwrap();
    fixture.run(Some("HardLink"), Some(all_file_times(false)), None);
    let latest = fixture
        .backups()
        .into_iter()
        .find(|path| path != &old)
        .unwrap();
    fs::write(latest, b"new archive data").unwrap();
    assert_ne!(fs::read(old).unwrap(), b"new archive data");
}

#[test]
fn skip_keeps_a_new_full_backup_when_the_previous_one_expires() {
    let fixture = Fixture::new();
    fixture.run(Some("Skip"), None, None);
    let first = fixture.backups().remove(0);
    let old = fixture.output.join("backup_2020-01-01_00-00-00.7z");
    fs::rename(first, &old).unwrap();
    fixture.run(
        Some("Skip"),
        None,
        Some(serde_json::json!({ "max_age": "1day" })),
    );
    let backups = fixture.backups();
    assert_eq!(backups.len(), 1);
    assert_ne!(backups[0], old);
    assert!(backups[0].is_file());
}

#[test]
fn skip_removes_the_duplicate_archive() {
    let skip = Fixture::new();
    skip.run(Some("Skip"), None, None);
    let old = skip.output.join("backup_2020-01-01_00-00-00.7z");
    fs::rename(skip.backups().remove(0), &old).unwrap();
    skip.run(Some("Skip"), None, None);
    assert_eq!(skip.backups(), vec![old]);
}

#[test]
fn symbolic_duplicates_remain_valid_after_retention_promotes_a_link() {
    let fixture = Fixture::new();
    let probe_target = fixture.output.join("symlink-probe-target");
    let probe_link = fixture.output.join("symlink-probe-link");
    fs::write(&probe_target, b"probe").unwrap();
    if let Err(error) = create_symlink(Path::new("symlink-probe-target"), &probe_link) {
        if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("Could not create test symlink: {error}");
    }
    fs::remove_file(probe_link).unwrap();
    fs::remove_file(probe_target).unwrap();
    fixture.run(Some("SymbolicLink"), None, None);
    let old = fixture.output.join("backup_2020-01-01_00-00-00.7z");
    fs::rename(fixture.backups().remove(0), &old).unwrap();
    fixture.run(Some("SymbolicLink"), None, None);
    let second = fixture
        .backups()
        .into_iter()
        .find(|path| path != &old)
        .unwrap();
    assert!(
        fs::symlink_metadata(&second)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let promoted = fixture.output.join("backup_2021-01-01_00-00-00.7z");
    fs::rename(second, &promoted).unwrap();
    fixture.run(
        Some("SymbolicLink"),
        None,
        Some(serde_json::json!({ "keep_last": 2 })),
    );

    assert!(!old.exists());
    assert!(
        fs::symlink_metadata(&promoted)
            .unwrap()
            .file_type()
            .is_file()
    );
    let latest = fixture
        .backups()
        .into_iter()
        .find(|path| path != &promoted)
        .unwrap();
    assert_eq!(
        fs::read_link(&latest).unwrap(),
        Path::new("backup_2021-01-01_00-00-00.7z")
    );
    assert_eq!(fs::read(latest).unwrap(), fs::read(promoted).unwrap());
}

#[test]
fn retention_promotes_oldest_surviving_symbolic_link() {
    let fixture = Fixture::new();
    let target = fixture.output.join("backup_2020-01-01_00-00-00.7z");
    let first_link = fixture.output.join("backup_2021-01-01_00-00-00.7z");
    let second_link = fixture.output.join("backup_2022-01-01_00-00-00.7z");
    fs::write(&target, b"archived data").unwrap();
    if let Err(error) = create_symlink(Path::new("backup_2020-01-01_00-00-00.7z"), &first_link) {
        if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("Could not create test symlink: {error}");
    }
    create_symlink(Path::new("backup_2020-01-01_00-00-00.7z"), &second_link).unwrap();
    fixture.run(None, None, Some(serde_json::json!({ "keep_last": 3 })));

    assert!(!target.exists());
    assert!(
        fs::symlink_metadata(&first_link)
            .unwrap()
            .file_type()
            .is_file()
    );
    assert_eq!(fs::read(&first_link).unwrap(), b"archived data");
    assert_eq!(
        fs::read_link(&second_link).unwrap(),
        Path::new("backup_2021-01-01_00-00-00.7z")
    );
    assert_eq!(fs::read(&second_link).unwrap(), b"archived data");
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
