use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::Serialize;

use crate::db::import_jobs;
use crate::routes::AppStateInner;
use crate::services::import::{ImportTrigger, start_import};

const SLOT_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_mins(5);
const SLOT_RETRY_LIMIT: u8 = 24;

#[derive(Debug, Clone)]
pub struct ImportSchedulerConfig {
    pub enabled: bool,
    pub schedule: String,
    pub timezone: String,
    pub adapters: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportScheduleStatus {
    pub enabled: bool,
    pub schedule: String,
    pub timezone: String,
    pub adapters: Vec<String>,
    pub next_run: Option<DateTime<Utc>>,
}

impl ImportSchedulerConfig {
    pub fn validate(&self, state: &AppStateInner) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        self.parsed_schedule()?;
        self.parsed_timezone()?;
        if self.adapters.is_empty() {
            return Err("IMPORT_SCHEDULED_ADAPTERS must not be empty".to_string());
        }
        for adapter in &self.adapters {
            if state.adapter_registry.get(adapter).is_none() {
                return Err(format!("Unknown scheduled import adapter '{adapter}'"));
            }
        }
        Ok(())
    }

    pub fn status(&self, now: DateTime<Utc>) -> Result<ImportScheduleStatus, String> {
        Ok(ImportScheduleStatus {
            enabled: self.enabled,
            schedule: self.schedule.clone(),
            timezone: self.timezone.clone(),
            adapters: self.adapters.clone(),
            next_run: self.next_run(now)?,
        })
    }

    pub fn next_run(&self, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
        if !self.enabled {
            return Ok(None);
        }
        let schedule = self.parsed_schedule()?;
        let timezone = self.parsed_timezone()?;
        Ok(schedule
            .after(&now.with_timezone(&timezone))
            .next()
            .map(|date| date.with_timezone(&Utc)))
    }

    pub fn latest_due_run(&self, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
        if !self.enabled {
            return Ok(None);
        }
        let schedule = self.parsed_schedule()?;
        let timezone = self.parsed_timezone()?;
        let local_now = now.with_timezone(&timezone);
        let search_start = (now - Duration::days(8)).with_timezone(&timezone);
        Ok(schedule
            .after(&search_start)
            .take_while(|date| *date <= local_now)
            .last()
            .map(|date| date.with_timezone(&Utc)))
    }

    fn parsed_schedule(&self) -> Result<Schedule, String> {
        Schedule::from_str(&self.schedule)
            .map_err(|error| format!("Invalid IMPORT_SCHEDULE: {error}"))
    }

    fn parsed_timezone(&self) -> Result<Tz, String> {
        Tz::from_str(&self.timezone)
            .map_err(|_| format!("Invalid IMPORT_TIMEZONE: '{}'", self.timezone))
    }
}

pub fn spawn_import_scheduler(
    state: Arc<AppStateInner>,
    config: ImportSchedulerConfig,
) -> Result<Option<tokio::task::JoinHandle<()>>, String> {
    config.validate(&state)?;
    if !config.enabled {
        tracing::info!("Scheduled imports are disabled");
        return Ok(None);
    }

    Ok(Some(tokio::spawn(async move {
        if let Ok(Some(due)) = config.latest_due_run(Utc::now()) {
            reserve_scheduled_slot_with_retry(state.clone(), &config, due).await;
        }

        loop {
            let now = Utc::now();
            let next = match config.next_run(now) {
                Ok(Some(next)) => next,
                Ok(None) => return,
                Err(error) => {
                    tracing::error!(error, "Could not calculate next scheduled import");
                    return;
                }
            };
            tracing::info!(scheduled_for = %next, "Next scheduled import calculated");
            let wait = (next - Utc::now())
                .to_std()
                .unwrap_or(std::time::Duration::ZERO);
            tokio::time::sleep(wait).await;
            reserve_scheduled_slot_with_retry(state.clone(), &config, next).await;
        }
    })))
}

async fn reserve_scheduled_slot_with_retry(
    state: Arc<AppStateInner>,
    config: &ImportSchedulerConfig,
    scheduled_for: DateTime<Utc>,
) {
    for attempt in 0..=SLOT_RETRY_LIMIT {
        if run_scheduled_slot(state.clone(), config, scheduled_for).await {
            return;
        }
        if attempt == SLOT_RETRY_LIMIT {
            tracing::error!(
                scheduled_for = %scheduled_for,
                attempts = u16::from(SLOT_RETRY_LIMIT) + 1,
                "Scheduled import slot could not be fully reserved"
            );
            return;
        }
        tracing::warn!(
            scheduled_for = %scheduled_for,
            attempt = u16::from(attempt) + 1,
            "Scheduled import slot is incomplete; retrying"
        );
        tokio::time::sleep(SLOT_RETRY_INTERVAL).await;
    }
}

async fn run_scheduled_slot(
    state: Arc<AppStateInner>,
    config: &ImportSchedulerConfig,
    scheduled_for: DateTime<Utc>,
) -> bool {
    let mut fully_reserved = true;
    for adapter in &config.adapters {
        let slot = scheduled_for.naive_utc();
        match import_jobs::has_scheduled_job(&state.pool, adapter, slot).await {
            Ok(true) => {
                tracing::info!(adapter, scheduled_for = %scheduled_for, "Scheduled import already reserved");
                continue;
            }
            Ok(false) => {}
            Err(error) => {
                tracing::error!(adapter, error = %error, "Failed to check scheduled import slot");
                fully_reserved = false;
                continue;
            }
        }

        match start_import(
            state.clone(),
            adapter,
            ImportTrigger::Scheduled { scheduled_for },
        )
        .await
        {
            Ok(job) => tracing::info!(
                adapter,
                job_id = job.id,
                scheduled_for = %scheduled_for,
                "Scheduled import started"
            ),
            Err(error) => {
                fully_reserved = false;
                tracing::error!(
                    adapter,
                    scheduled_for = %scheduled_for,
                    error = %error,
                    "Failed to start scheduled import"
                );
            }
        }
    }
    fully_reserved
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    fn config(enabled: bool) -> ImportSchedulerConfig {
        ImportSchedulerConfig {
            enabled,
            schedule: "0 10 6 * * Sat *".to_string(),
            timezone: "Europe/Berlin".to_string(),
            adapters: vec!["maddrax".to_string(), "john-sinclair".to_string()],
        }
    }

    #[test]
    fn next_run_is_saturday_at_0610_in_summer_time() {
        let now = Utc.with_ymd_and_hms(2026, 8, 7, 12, 0, 0).unwrap();
        let next = config(true).next_run(now).unwrap().unwrap();
        let berlin = next.with_timezone(&chrono_tz::Europe::Berlin);
        assert_eq!(berlin.weekday(), chrono::Weekday::Sat);
        assert_eq!(
            (berlin.hour(), berlin.minute(), berlin.second()),
            (6, 10, 0)
        );
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 8, 8, 4, 10, 0).unwrap());
    }

    #[test]
    fn next_run_stays_at_0610_in_winter_time() {
        let now = Utc.with_ymd_and_hms(2026, 12, 4, 12, 0, 0).unwrap();
        let next = config(true).next_run(now).unwrap().unwrap();
        let berlin = next.with_timezone(&chrono_tz::Europe::Berlin);
        assert_eq!((berlin.hour(), berlin.minute()), (6, 10));
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 12, 5, 5, 10, 0).unwrap());
    }

    #[test]
    fn exact_slot_returns_following_week_as_next() {
        let exact = Utc.with_ymd_and_hms(2026, 8, 8, 4, 10, 0).unwrap();
        let next = config(true).next_run(exact).unwrap().unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 8, 15, 4, 10, 0).unwrap());
    }

    #[test]
    fn latest_due_returns_same_day_slot_after_execution_time() {
        let now = Utc.with_ymd_and_hms(2026, 8, 8, 8, 0, 0).unwrap();
        let due = config(true).latest_due_run(now).unwrap().unwrap();
        assert_eq!(due, Utc.with_ymd_and_hms(2026, 8, 8, 4, 10, 0).unwrap());
    }

    #[test]
    fn disabled_schedule_has_no_next_or_due_run() {
        let now = Utc.with_ymd_and_hms(2026, 8, 8, 8, 0, 0).unwrap();
        assert_eq!(config(false).next_run(now).unwrap(), None);
        assert_eq!(config(false).latest_due_run(now).unwrap(), None);
        let status = config(false).status(now).unwrap();
        assert!(!status.enabled);
        assert_eq!(status.next_run, None);
    }

    #[test]
    fn invalid_schedule_and_timezone_return_clear_errors() {
        let now = Utc.with_ymd_and_hms(2026, 8, 8, 8, 0, 0).unwrap();
        let mut invalid = config(true);
        invalid.schedule = "not a cron".to_string();
        assert!(
            invalid
                .next_run(now)
                .unwrap_err()
                .contains("IMPORT_SCHEDULE")
        );

        let mut invalid = config(true);
        invalid.timezone = "Mars/Olympus".to_string();
        assert!(
            invalid
                .next_run(now)
                .unwrap_err()
                .contains("IMPORT_TIMEZONE")
        );
    }

    #[test]
    fn status_contains_configuration_and_next_run() {
        let now = Utc.with_ymd_and_hms(2026, 8, 7, 12, 0, 0).unwrap();
        let status = config(true).status(now).unwrap();
        assert!(status.enabled);
        assert_eq!(status.timezone, "Europe/Berlin");
        assert_eq!(status.adapters.len(), 2);
        assert!(status.next_run.is_some());
    }
}
