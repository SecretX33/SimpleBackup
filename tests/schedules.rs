use chrono::Local;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
        let root = std::env::temp_dir().join(format!(
            "simplebackup-schedule-test-{}-{nonce}",
            std::process::id()
        ));
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

    fn run(&self, options: Value) -> String {
        let config = self.root.join("config.json");
        let mut value = json!({
            "output_folder": self.output,
            "sources": [{ "path": self.source }],
            "include_timestamps": false
        });
        value
            .as_object_mut()
            .unwrap()
            .extend(options.as_object().unwrap().clone());
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_simplebackup"))
            .arg(config)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }

    fn backups(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.output)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "7z"))
            .collect()
    }

    fn state_path(&self) -> PathBuf {
        self.output.join("backup_schedule.json")
    }

    fn seed_old_backup(&self) -> PathBuf {
        self.run(json!({}));
        let old = self.output.join("backup_2000-01-01_12-00-00.7z");
        fs::rename(self.backups().remove(0), &old).unwrap();
        old
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn successful_backup_satisfies_the_schedule_across_invocations() {
    for schedule in [
        json!({ "period": "daily" }),
        json!({ "min_interval": "24h" }),
    ] {
        let fixture = Fixture::new();
        let options = json!({ "schedule": schedule });
        assert!(
            fixture
                .run(options.clone())
                .contains("Backup completed successfully")
        );
        let state = fs::read(fixture.state_path()).unwrap();
        assert!(fixture.run(options).contains("Backup is not due yet"));
        assert_eq!(fixture.backups().len(), 1);
        assert_eq!(fs::read(fixture.state_path()).unwrap(), state);
    }
}

#[test]
fn recognized_archive_is_used_when_completion_state_is_absent() {
    let fixture = Fixture::new();
    fixture.run(json!({}));
    assert!(!fixture.state_path().exists());
    assert!(
        fixture
            .run(json!({ "schedule": { "period": "daily" } }))
            .contains("Backup is not due yet")
    );
    assert_eq!(fixture.backups().len(), 1);
}

#[test]
fn subhour_periods_create_backups_when_due() {
    for period in ["secondly", "minutely"] {
        let fixture = Fixture::new();
        fixture.seed_old_backup();
        assert!(
            fixture
                .run(json!({ "schedule": { "period": period } }))
                .contains("Backup completed successfully")
        );
        assert_eq!(fixture.backups().len(), 2);
        assert!(fixture.state_path().is_file());
    }
}

#[test]
fn missed_periods_produce_one_current_backup() {
    let fixture = Fixture::new();
    fixture.seed_old_backup();
    assert!(
        fixture
            .run(json!({ "schedule": { "period": "daily" } }))
            .contains("Backup completed successfully")
    );
    assert_eq!(fixture.backups().len(), 2);
}

#[test]
fn skipped_duplicate_satisfies_calendar_and_interval_schedules() {
    for schedule in [
        json!({ "period": "daily" }),
        json!({ "min_interval": "24h" }),
    ] {
        let fixture = Fixture::new();
        let old = fixture.seed_old_backup();
        let options = json!({ "schedule": schedule, "duplicate_backup_action": "Skip" });
        assert!(
            fixture
                .run(options.clone())
                .contains("Duplicate backup handled with Skip")
        );
        assert_eq!(fixture.backups(), vec![old.clone()]);
        let state: Value =
            serde_json::from_slice(&fs::read(fixture.state_path()).unwrap()).unwrap();
        assert_eq!(
            state["archive_name"],
            old.file_name().unwrap().to_str().unwrap()
        );
        assert!(fixture.run(options).contains("Backup is not due yet"));
    }
}

#[test]
fn failed_backup_does_not_satisfy_the_period() {
    let fixture = Fixture::new();
    let options = json!({ "schedule": { "period": "daily" } });
    fs::remove_file(fixture.source.join("file.txt")).unwrap();
    fs::remove_dir(&fixture.source).unwrap();
    assert!(
        fixture
            .run(options.clone())
            .contains("Error running backup for source")
    );
    assert!(fixture.backups().is_empty());
    assert!(!fixture.state_path().exists());
    fs::create_dir(&fixture.source).unwrap();
    fs::write(fixture.source.join("file.txt"), b"recovered").unwrap();
    assert!(
        fixture
            .run(options)
            .contains("Backup completed successfully")
    );
    assert_eq!(fixture.backups().len(), 1);
}

#[test]
fn missing_archives_do_not_allow_state_to_suppress_a_backup() {
    let fixture = Fixture::new();
    let options = json!({ "schedule": { "period": "daily" } });
    fixture.run(options.clone());
    fs::remove_file(fixture.backups().remove(0)).unwrap();
    assert!(
        fixture
            .run(options)
            .contains("Backup completed successfully")
    );
    assert_eq!(fixture.backups().len(), 1);
}

#[test]
fn invalid_or_stale_state_falls_back_to_the_latest_archive() {
    for state in [
        "invalid JSON".to_string(),
        json!({ "archive_name": "backup_2000-01-01_12-00-00.7z", "completed_at": "invalid time" })
            .to_string(),
        json!({ "archive_name": "missing.7z", "completed_at": Local::now().to_rfc3339() })
            .to_string(),
    ] {
        let fixture = Fixture::new();
        fixture.seed_old_backup();
        fs::write(fixture.state_path(), state).unwrap();
        assert!(
            fixture
                .run(json!({ "schedule": { "period": "daily" } }))
                .contains("Backup completed successfully")
        );
        assert_eq!(fixture.backups().len(), 2);
    }
}

#[test]
fn schedule_state_is_scoped_to_the_archive_prefix() {
    let fixture = Fixture::new();
    let options = json!({ "schedule": { "period": "daily" } });
    fixture.run(options.clone());
    assert!(
        fixture
            .run(json!({ "schedule": { "period": "daily" }, "archive_name_prefix": "other_" }))
            .contains("Backup completed successfully")
    );
    assert!(fixture.state_path().is_file());
    assert!(fixture.output.join("other_schedule.json").is_file());
    assert_eq!(fixture.backups().len(), 2);
    assert!(fixture.run(options).contains("Backup is not due yet"));
}

#[test]
fn omitting_schedule_runs_even_with_a_recent_completion_record() {
    let fixture = Fixture::new();
    fixture.seed_old_backup();
    fixture.run(json!({ "schedule": { "period": "daily" }, "duplicate_backup_action": "Skip" }));
    assert!(
        fixture
            .run(json!({}))
            .contains("Backup completed successfully")
    );
    assert_eq!(fixture.backups().len(), 2);
}
