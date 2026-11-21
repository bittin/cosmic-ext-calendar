//! Due reminder checks for events loaded by the desktop app.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use chrono::TimeZone;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::features::event::model::{Event, EventTime};
use crate::fl;
use crate::shared::ids::EventId;
use crate::shared::storage::LocalStorage;

const GRACE: Duration = Duration::minutes(5);

pub type ReminderKey = (EventId, OffsetDateTime, i64);

pub fn check_reminders(
    storage: &Arc<Mutex<LocalStorage>>,
    already_fired: &HashSet<ReminderKey>,
) -> Vec<ReminderKey> {
    let now = OffsetDateTime::now_utc();
    let mut queued = Vec::new();
    let mut seen = HashSet::new();
    {
        let storage = storage.lock().unwrap();
        for event in storage.all_events() {
            let civil = event.floating || matches!(event.time, EventTime::AllDay { .. });
            let padding = if civil { Duration::DAY } else { Duration::ZERO };
            for reminder in &event.reminders {
                let minutes = reminder.minutes_before;
                let Some(seconds) = minutes.checked_mul(60).filter(|seconds| *seconds >= 0) else {
                    continue;
                };
                let lead = Duration::seconds(seconds);
                let end = now.saturating_add(lead);
                let window = end.saturating_sub(GRACE + padding)
                    ..end.saturating_add(padding + Duration::NANOSECOND);
                for occurrence in event.occurrences(window) {
                    let key = (event.id, occurrence, minutes);
                    if already_fired.contains(&key) || !seen.insert(key) {
                        continue;
                    }
                    let Some(start) = reminder_time(event, occurrence) else {
                        continue;
                    };
                    let trigger = start.saturating_sub(lead);
                    if trigger <= now && trigger > now - GRACE {
                        queued.push((key, event.summary.clone(), start));
                    }
                }
            }
        }
    }

    queued
        .into_iter()
        .filter_map(|(key, summary, start)| notify(&summary, start).then_some(key))
        .collect()
}

fn reminder_time(event: &Event, occurrence: OffsetDateTime) -> Option<OffsetDateTime> {
    if !event.floating && !matches!(event.time, EventTime::AllDay { .. }) {
        return Some(occurrence);
    }

    let wall =
        chrono::DateTime::from_timestamp(occurrence.unix_timestamp(), occurrence.nanosecond())?
            .naive_utc();
    let offset = chrono::Local
        .from_local_datetime(&wall)
        .earliest()?
        .offset()
        .local_minus_utc();
    Some(occurrence.replace_offset(UtcOffset::from_whole_seconds(offset).ok()?))
}

fn notify(summary: &str, start: OffsetDateTime) -> bool {
    let body = format!(
        "{:02}:{:02} · {}",
        start.hour(),
        start.minute(),
        start.date()
    );
    match notify_rust::Notification::new()
        .appname(&fl!("app-title"))
        .summary(summary)
        .body(&body)
        .icon("office-calendar-symbolic")
        .show()
    {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!("failed to show reminder notification: {err}");
            false
        }
    }
}
