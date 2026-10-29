use crate::cleanup::latest_backup;
use crate::config::{AppConfig, BackupPeriod, ScheduleConfig};
use crate::log;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

impl ScheduleConfig {
    fn is_due_since<Tz: TimeZone>(self, last: DateTime<Tz>, now: DateTime<Tz>) -> bool {
        match self {
            Self::MinInterval(interval) => now
                .signed_duration_since(last)
                .to_std()
                .is_ok_and(|elapsed| elapsed >= interval),
            Self::Period(period) => match period {
                BackupPeriod::Secondly => {
                    (last.date_naive(), last.hour(), last.minute(), last.second())
                        < (now.date_naive(), now.hour(), now.minute(), now.second())
                }
                BackupPeriod::Minutely => {
                    (last.date_naive(), last.hour(), last.minute())
                        < (now.date_naive(), now.hour(), now.minute())
                }
                BackupPeriod::Hourly => {
                    (last.date_naive(), last.hour()) < (now.date_naive(), now.hour())
                }
                BackupPeriod::Daily => last.date_naive() < now.date_naive(),
                BackupPeriod::Weekly => {
                    let last_week = last.iso_week();
                    let current_week = now.iso_week();
                    (last_week.year(), last_week.week())
                        < (current_week.year(), current_week.week())
                }
                BackupPeriod::Monthly => (last.year(), last.month()) < (now.year(), now.month()),
                BackupPeriod::Yearly => last.year() < now.year(),
            },
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ScheduleState {
    completed_at: String,
    archive_name: String,
}

fn state_path(config: &AppConfig) -> PathBuf {
    config
        .output_folder
        .join(format!("{}schedule.json", config.archive_name_prefix))
}

pub fn backup_is_due(config: &AppConfig) -> bool {
    let Some(schedule) = config.schedule else {
        return true;
    };
    let Some((archive, archive_time)) = latest_backup(config) else {
        return true;
    };
    let completion = read_completion(config, &archive).unwrap_or(archive_time);
    schedule.is_due_since(completion.max(archive_time), Local::now())
}

fn read_completion(config: &AppConfig, archive: &Path) -> Option<DateTime<Local>> {
    let path = state_path(config);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            log!(
                "Could not read schedule state '{}': {error}",
                path.display()
            );
            return None;
        }
    };
    let state = match serde_json::from_str::<ScheduleState>(&contents) {
        Ok(state) => state,
        Err(error) => {
            log!("Invalid schedule state '{}': {error}", path.display());
            return None;
        }
    };
    if archive.file_name()?.to_str()? != state.archive_name {
        return None;
    }
    match DateTime::parse_from_rfc3339(&state.completed_at) {
        Ok(completed_at) => Some(completed_at.with_timezone(&Local)),
        Err(error) => {
            log!(
                "Invalid schedule completion time '{}': {error}",
                path.display()
            );
            None
        }
    }
}

pub fn record_completion(config: &AppConfig, archive: &Path) {
    if config.schedule.is_none() || !archive.is_file() {
        return;
    }
    let state = ScheduleState {
        completed_at: Local::now().to_rfc3339(),
        archive_name: archive.file_name().unwrap().to_string_lossy().into_owned(),
    };
    let path = state_path(config);
    let result = serde_json::to_vec(&state)
        .map_err(std::io::Error::other)
        .and_then(|contents| fs::write(&path, contents));
    if let Err(error) = result {
        log!(
            "Could not save schedule state '{}': {error}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn subhour_periods_follow_local_boundaries() {
        let cases = [
            (
                "secondly",
                "2026-10-28T08:01:30.100-03:00",
                "2026-10-28T08:01:30.999-03:00",
                false,
            ),
            (
                "secondly",
                "2026-10-28T08:01:30.999-03:00",
                "2026-10-28T08:01:31-03:00",
                true,
            ),
            (
                "secondly",
                "2026-10-28T08:01:59.999-03:00",
                "2026-10-28T08:02:00-03:00",
                true,
            ),
            (
                "secondly",
                "2026-10-28T23:59:59.999-03:00",
                "2026-10-29T00:00:00-03:00",
                true,
            ),
            (
                "secondly",
                "2026-11-01T01:30:30-04:00",
                "2026-11-01T01:30:30-05:00",
                false,
            ),
            (
                "minutely",
                "2026-10-28T08:01:00-03:00",
                "2026-10-28T08:01:59.999-03:00",
                false,
            ),
            (
                "minutely",
                "2026-10-28T08:01:59.999-03:00",
                "2026-10-28T08:02:00-03:00",
                true,
            ),
            (
                "minutely",
                "2026-10-28T08:59:59.999-03:00",
                "2026-10-28T09:00:00-03:00",
                true,
            ),
            (
                "minutely",
                "2026-10-28T23:59:59.999-03:00",
                "2026-10-29T00:00:00-03:00",
                true,
            ),
            (
                "minutely",
                "2026-11-01T01:30:00-04:00",
                "2026-11-01T01:30:00-05:00",
                false,
            ),
        ];
        for (period, last, now, due) in cases {
            let schedule: ScheduleConfig =
                serde_json::from_value(serde_json::json!({ "period": period })).unwrap();
            let last = DateTime::parse_from_rfc3339(last).unwrap();
            let now = DateTime::parse_from_rfc3339(now).unwrap();
            assert_eq!(
                schedule.is_due_since(last, now),
                due,
                "{period}: {last} -> {now}"
            );
        }
    }

    #[test]
    fn calendar_periods_follow_local_boundaries() {
        let cases = [
            (
                BackupPeriod::Hourly,
                "2026-10-28T08:01:00-03:00",
                "2026-10-28T08:59:00-03:00",
                false,
            ),
            (
                BackupPeriod::Hourly,
                "2026-10-28T08:59:00-03:00",
                "2026-10-28T09:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Daily,
                "2026-10-28T18:00:00-03:00",
                "2026-10-29T08:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Daily,
                "2026-10-29T00:01:00-03:00",
                "2026-10-29T23:59:00-03:00",
                false,
            ),
            (
                BackupPeriod::Daily,
                "2026-10-28T23:59:59-03:00",
                "2026-10-29T00:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Weekly,
                "2026-10-25T23:59:59-03:00",
                "2026-10-26T00:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Weekly,
                "2026-10-26T08:00:00-03:00",
                "2026-11-01T23:59:00-03:00",
                false,
            ),
            (
                BackupPeriod::Weekly,
                "2020-12-31T08:00:00-03:00",
                "2021-01-01T08:00:00-03:00",
                false,
            ),
            (
                BackupPeriod::Weekly,
                "2021-01-03T23:59:59-03:00",
                "2021-01-04T00:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Monthly,
                "2028-02-29T23:59:59-03:00",
                "2028-03-01T00:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Monthly,
                "2026-10-01T00:00:00-03:00",
                "2026-10-31T23:59:59-03:00",
                false,
            ),
            (
                BackupPeriod::Yearly,
                "2026-12-31T23:59:59-03:00",
                "2027-01-01T00:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Yearly,
                "2026-01-01T00:00:00-03:00",
                "2026-12-31T23:59:59-03:00",
                false,
            ),
            (
                BackupPeriod::Daily,
                "2026-01-01T12:00:00-03:00",
                "2026-10-29T08:00:00-03:00",
                true,
            ),
            (
                BackupPeriod::Daily,
                "2026-03-07T23:30:00-05:00",
                "2026-03-08T03:30:00-04:00",
                true,
            ),
            (
                BackupPeriod::Daily,
                "2026-11-01T00:30:00-04:00",
                "2026-11-01T23:30:00-05:00",
                false,
            ),
            (
                BackupPeriod::Hourly,
                "2026-11-01T01:30:00-04:00",
                "2026-11-01T01:30:00-05:00",
                false,
            ),
        ];
        for (period, last, now, due) in cases {
            let last = DateTime::parse_from_rfc3339(last).unwrap();
            let now = DateTime::parse_from_rfc3339(now).unwrap();
            assert_eq!(
                ScheduleConfig::Period(period).is_due_since(last, now),
                due,
                "{period:?}: {last} -> {now}"
            );
        }
    }

    #[test]
    fn intervals_require_the_full_elapsed_duration() {
        let last = DateTime::parse_from_rfc3339("2026-10-28T18:00:00-03:00").unwrap();
        let schedule = ScheduleConfig::MinInterval(Duration::from_secs(24 * 3600));
        for (now, due) in [
            ("2026-10-29T08:00:00-03:00", false),
            ("2026-10-29T17:59:59-03:00", false),
            ("2026-10-29T18:00:00-03:00", true),
            ("2026-10-30T08:00:00-03:00", true),
        ] {
            let now = DateTime::parse_from_rfc3339(now).unwrap();
            assert_eq!(schedule.is_due_since(last, now), due);
        }
        assert!(!ScheduleConfig::MinInterval(Duration::MAX).is_due_since(last, last));
        assert!(ScheduleConfig::MinInterval(Duration::ZERO).is_due_since(last, last));
    }

    #[test]
    fn future_completions_do_not_trigger_early_backups() {
        let last = DateTime::parse_from_rfc3339("2026-10-29T18:00:00-03:00").unwrap();
        let now = DateTime::parse_from_rfc3339("2026-10-28T18:00:00-03:00").unwrap();
        for schedule in [
            ScheduleConfig::Period(BackupPeriod::Secondly),
            ScheduleConfig::Period(BackupPeriod::Minutely),
            ScheduleConfig::Period(BackupPeriod::Hourly),
            ScheduleConfig::Period(BackupPeriod::Daily),
            ScheduleConfig::Period(BackupPeriod::Weekly),
            ScheduleConfig::Period(BackupPeriod::Monthly),
            ScheduleConfig::Period(BackupPeriod::Yearly),
            ScheduleConfig::MinInterval(Duration::from_secs(1)),
        ] {
            assert!(!schedule.is_due_since(last, now));
        }
    }
}
