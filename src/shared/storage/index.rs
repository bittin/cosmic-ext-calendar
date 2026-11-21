use std::collections::{BTreeMap, HashMap};

use time::OffsetDateTime;

use crate::features::calendar_management::model::CalendarKey;
use crate::features::event::{model::Event, resolve_series_occurrence};
use crate::shared::ids::EventId;

/// In-memory index over all loaded [`Event`]s, rebuilt (or loaded from a
/// disposable cache) at startup so date-range queries don't need to scan
/// every JSON file on disk each time.
#[derive(Default)]
pub struct EventIndex {
    by_id: HashMap<EventId, Event>,
    by_calendar: BTreeMap<CalendarKey, Vec<EventId>>,
    by_start: BTreeMap<OffsetDateTime, Vec<EventId>>,
}

impl EventIndex {
    pub fn upsert(&mut self, event: Event) {
        self.remove(event.id);

        let calendar = event.calendar.clone();
        let start = event_sort_key(&event);
        let id = event.id;

        self.by_id.insert(id, event);
        self.by_calendar.entry(calendar).or_default().push(id);
        self.by_start.entry(start).or_default().push(id);
    }

    pub fn remove(&mut self, id: EventId) {
        let Some(existing) = self.by_id.remove(&id) else {
            return;
        };

        if let Some(ids) = self.by_calendar.get_mut(&existing.calendar) {
            ids.retain(|other| *other != id);
        }
        let start = event_sort_key(&existing);
        if let Some(ids) = self.by_start.get_mut(&start) {
            ids.retain(|other| *other != id);
        }
    }

    pub fn get(&self, id: EventId) -> Option<&Event> {
        self.by_id.get(&id)
    }

    pub fn events_for_calendar(&self, calendar: &CalendarKey) -> Vec<&Event> {
        self.by_calendar
            .get(calendar)
            .into_iter()
            .flatten()
            .filter_map(|id| self.by_id.get(id))
            .collect()
    }

    pub fn all_records(&self) -> impl Iterator<Item = &Event> + Clone {
        self.by_id.values()
    }

    pub fn all(&self) -> impl Iterator<Item = &Event> {
        self.all_records().filter(|event| {
            if event
                .recurrence_cutoff()
                .is_some_and(|cutoff| cutoff <= event.first_possible_occurrence())
            {
                return false;
            }
            let Some((series, occurrence)) = event.series_id.zip(event.recurrence_id) else {
                return true;
            };
            let (series, occurrence) =
                resolve_series_occurrence(series, occurrence, self.all_records());
            self.get(series)
                .and_then(|parent| parent.repeat_before)
                .is_none_or(|cutoff| occurrence < cutoff)
        })
    }

    /// Case-insensitive substring search over `summary`/`description`/
    /// `location`, sorted by start time. No search index is built given the
    /// expected data volumes (a personal calendar, hundreds-to-low-thousands
    /// of events).
    pub fn search(&self, query: &str) -> Vec<&Event> {
        let query = query.to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<&Event> = self
            .all()
            .filter(|event| {
                event.summary.to_lowercase().contains(&query)
                    || event
                        .description
                        .as_ref()
                        .is_some_and(|text| text.to_lowercase().contains(&query))
                    || event
                        .location
                        .as_ref()
                        .is_some_and(|text| text.to_lowercase().contains(&query))
            })
            .collect();
        results.sort_by_key(|event| event_sort_key(event));
        results
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

fn event_sort_key(event: &Event) -> OffsetDateTime {
    match &event.time {
        crate::features::event::model::EventTime::Timed { start, .. } => *start,
        crate::features::event::model::EventTime::AllDay { start, .. } => {
            start.midnight().assume_offset(time::UtcOffset::UTC)
        }
    }
}
