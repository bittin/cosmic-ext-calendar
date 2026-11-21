use std::collections::HashMap;
use std::path::PathBuf;

use time::OffsetDateTime;
use tracing::warn;

use super::{atomic, index::EventIndex, migration, paths};
use crate::features::calendar_management::model::{
    CalendarKey, LocalCalendarMeta, CURRENT_CALENDAR_SCHEMA_VERSION,
};
use crate::features::event::model::{Event, CURRENT_EVENT_SCHEMA_VERSION};
use crate::features::event::resolve_series_occurrence;
use crate::shared::ids::{CalendarId, EventId};
use crate::Result;

/// The local, file-backed store for calendars and events created in this
/// app.
pub struct LocalStorage {
    index: EventIndex,
    event_files: HashMap<EventId, PathBuf>,
    duplicate_files: HashMap<EventId, Vec<PathBuf>>,
    calendars: HashMap<CalendarId, LocalCalendarMeta>,
}

impl LocalStorage {
    /// An empty store, used as a fallback when [`Self::load`] fails (e.g. the
    /// data directory is inaccessible), mirroring how a corrupt app config
    /// falls back to `Config::default()` today.
    pub fn empty() -> Self {
        Self {
            index: EventIndex::default(),
            event_files: HashMap::new(),
            duplicate_files: HashMap::new(),
            calendars: HashMap::new(),
        }
    }

    /// Loads all calendars and events from disk, skipping (and logging) any
    /// individual file that fails to parse rather than failing the whole
    /// load — a single corrupt event shouldn't take down the app.
    pub fn load() -> Result<Self> {
        let mut calendars = HashMap::new();
        for entry in read_dir_json(&paths::calendars_dir())? {
            match load_calendar_file(&entry) {
                Ok(meta) => {
                    calendars.insert(meta.id, meta);
                }
                Err(err) => warn!("failed to load calendar {entry:?}: {err}"),
            }
        }

        let mut index = EventIndex::default();
        let mut event_files = HashMap::new();
        let mut duplicate_files: HashMap<EventId, Vec<PathBuf>> = HashMap::new();
        let events_root = paths::data_dir().join("events");
        if events_root.is_dir() {
            let calendar_dirs = std::fs::read_dir(&events_root)?;
            for calendar_dir in calendar_dirs.flatten() {
                let path = calendar_dir.path();
                if !path.is_dir() {
                    continue;
                }
                for entry in read_dir_json(&path)? {
                    match load_event_file(&entry) {
                        Ok(event) => {
                            if let Some(previous) = index.get(event.id) {
                                if (
                                    previous.sequence,
                                    previous.updated_at,
                                    &event_files[&event.id],
                                ) >= (event.sequence, event.updated_at, &entry)
                                {
                                    duplicate_files.entry(event.id).or_default().push(entry);
                                    continue;
                                }
                            }
                            if let Some(old_path) = event_files.insert(event.id, entry) {
                                duplicate_files.entry(event.id).or_default().push(old_path);
                            }
                            index.upsert(event);
                        }
                        Err(err) => warn!("failed to load event {entry:?}: {err}"),
                    }
                }
            }
        }

        let mut storage = Self {
            index,
            event_files,
            duplicate_files,
            calendars,
        };
        let splits: Vec<_> = storage
            .index
            .all_records()
            .filter_map(|event| event.predecessor)
            .collect();
        for (series, occurrence) in splits {
            storage.split_series(series, occurrence);
        }
        let overrides: Vec<_> = storage
            .index
            .all_records()
            .filter_map(|event| Some((event.series_id?, event.recurrence_id?)))
            .collect();
        for (series, occurrence) in overrides {
            storage.suppress_occurrence(series, occurrence);
        }
        Ok(storage)
    }

    // -- Calendars --------------------------------------------------------

    pub fn list_calendars(&self) -> Vec<&LocalCalendarMeta> {
        self.calendars.values().collect()
    }

    pub fn get_calendar(&self, id: CalendarId) -> Option<&LocalCalendarMeta> {
        self.calendars.get(&id)
    }

    pub fn create_calendar(&mut self, meta: LocalCalendarMeta) -> Result<()> {
        let path = paths::calendar_path(&meta.id);
        let wrapped = migration::wrap(CURRENT_CALENDAR_SCHEMA_VERSION, &meta)?;
        atomic::write_json_atomic(&path, &wrapped)?;
        self.calendars.insert(meta.id, meta);
        Ok(())
    }

    pub fn update_calendar(&mut self, meta: LocalCalendarMeta) -> Result<()> {
        self.create_calendar(meta)
    }

    pub fn delete_calendar(&mut self, id: CalendarId) -> Result<()> {
        let calendar = CalendarKey::Local(id);
        let event_ids: Vec<EventId> = self
            .index
            .events_for_calendar(&calendar)
            .into_iter()
            .map(|event| event.id)
            .collect();
        for event_id in event_ids {
            self.delete_event(event_id)?;
        }

        let calendar_path = paths::calendar_path(&id);
        if calendar_path.is_file() {
            std::fs::remove_file(&calendar_path)?;
        }
        self.calendars.remove(&id);
        Ok(())
    }

    // -- Events -------------------------------------------------------------

    pub fn get_event(&self, id: EventId) -> Option<&Event> {
        self.index.get(id)
    }

    /// All events currently in storage, regardless of date — used for ICS
    /// export.
    pub fn all_events(&self) -> Vec<&Event> {
        self.index.all().collect()
    }

    /// Include truncated predecessors so the exporter can resolve moved
    /// exceptions even when the old series has no visible occurrences.
    pub fn events_for_export(&self) -> impl Iterator<Item = &Event> {
        self.index
            .all()
            .chain(self.index.all_records().filter(|event| {
                event.split_before.is_some()
                    && event
                        .recurrence_cutoff()
                        .is_some_and(|end| end <= event.first_possible_occurrence())
            }))
    }

    pub fn search(&self, query: &str) -> Vec<&Event> {
        self.index.search(query)
    }

    pub fn create_event(&mut self, event: Event) -> Result<()> {
        if let CalendarKey::Local(calendar_id) = &event.calendar {
            let already_assigned = self
                .index
                .get(event.id)
                .is_some_and(|existing| existing.calendar == event.calendar);
            if !already_assigned && !self.calendars.contains_key(calendar_id) {
                return Err(crate::Error::Storage(
                    "destination calendar no longer exists".into(),
                ));
            }
            // Keep the existing file location when moving an event. The
            // calendar in the payload owns it, and one atomic write changes
            // that ownership without leaving a second copy on disk.
            let path = self
                .event_files
                .get(&event.id)
                .cloned()
                .unwrap_or_else(|| paths::event_path(calendar_id, &event.id));
            let wrapped = migration::wrap(CURRENT_EVENT_SCHEMA_VERSION, &event)?;
            atomic::write_json_atomic(&path, &wrapped)?;
            self.event_files.insert(event.id, path);
        }
        let replacement = event.series_id.zip(event.recurrence_id);
        let split = event.predecessor;
        self.index.upsert(event);
        if let Some((series, occurrence)) = split {
            self.split_series(series, occurrence);
        }
        if let Some((series, occurrence)) = replacement {
            self.suppress_occurrence(series, occurrence);
        }
        Ok(())
    }

    pub fn update_event(&mut self, event: Event) -> Result<()> {
        self.create_event(event)
    }

    fn split_series(&mut self, series: EventId, occurrence: OffsetDateTime) {
        if let Some(mut parent) = self.index.get(series).cloned() {
            parent.split_before = Some(
                parent
                    .split_before
                    .map_or(occurrence, |cutoff| cutoff.min(occurrence)),
            );
            self.index.upsert(parent);
        }
    }

    fn suppress_occurrence(&mut self, series: EventId, occurrence: OffsetDateTime) {
        let (series, occurrence) =
            resolve_series_occurrence(series, occurrence, self.index.all_records());
        if let Some(mut parent) = self.index.get(series).cloned() {
            if !parent.exdates.contains(&occurrence) {
                parent.exdates.push(occurrence);
                self.index.upsert(parent);
            }
        }
    }

    pub fn delete_event(&mut self, id: EventId) -> Result<()> {
        if let Some((series, occurrence)) = self.index.get(id).and_then(|event| event.predecessor) {
            if let Some(mut parent) = self.index.get(series).cloned() {
                parent.repeat_before = Some(
                    parent
                        .repeat_before
                        .map_or(occurrence, |cutoff| cutoff.min(occurrence)),
                );
                self.update_event(parent)?;
            }
        }
        if let Some((series, occurrence)) = self
            .index
            .get(id)
            .and_then(|event| event.series_id.zip(event.recurrence_id))
        {
            let (owner, slot) =
                resolve_series_occurrence(series, occurrence, self.index.all_records());
            if let Some(mut parent) = self.index.get(owner).cloned() {
                if !parent.exdates.contains(&slot) {
                    parent.exdates.push(slot);
                }
                self.update_event(parent)?;
            }
        }
        let mut successors = std::collections::HashSet::from([id]);
        loop {
            let children: Vec<_> = self
                .index
                .all_records()
                .filter(|event| {
                    event
                        .predecessor
                        .is_some_and(|(parent, _)| successors.contains(&parent))
                })
                .map(|event| event.id)
                .collect();
            let count = successors.len();
            successors.extend(children);
            if successors.len() == count {
                break;
            }
        }
        let overrides: Vec<_> = self
            .index
            .all_records()
            .filter_map(|event| {
                let (series, occurrence) = event.series_id.zip(event.recurrence_id)?;
                let (owner, slot) =
                    resolve_series_occurrence(series, occurrence, self.index.all_records());
                (owner == id || (series != owner && successors.contains(&owner))).then_some((
                    event.clone(),
                    owner,
                    slot,
                ))
            })
            .collect();
        for (mut replacement, owner, slot) in overrides {
            if owner == id {
                self.remove_event_file(replacement.id)?;
            } else {
                // A later split is an independent series. Keep the edited
                // occurrence attached to it if the predecessor is removed.
                replacement.series_id = Some(owner);
                replacement.recurrence_id = Some(slot);
                replacement.ics_uid = self
                    .index
                    .get(owner)
                    .and_then(|parent| parent.ics_uid.clone());
                self.update_event(replacement)?;
            }
        }
        self.remove_event_file(id)
    }

    fn remove_event_file(&mut self, id: EventId) -> Result<()> {
        // Remove older copies first so a failed deletion cannot make one
        // reappear as the current event on the next startup.
        for path in self
            .duplicate_files
            .get(&id)
            .into_iter()
            .flatten()
            .chain(self.event_files.get(&id))
        {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        self.duplicate_files.remove(&id);
        self.event_files.remove(&id);
        self.index.remove(id);
        Ok(())
    }
}

fn read_dir_json(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn load_calendar_file(path: &std::path::Path) -> Result<LocalCalendarMeta> {
    let bytes = std::fs::read(path)?;
    let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
    migration::unwrap_current(raw, CURRENT_CALENDAR_SCHEMA_VERSION, "LocalCalendarMeta")
}

fn load_event_file(path: &std::path::Path) -> Result<Event> {
    let bytes = std::fs::read(path)?;
    let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
    migration::unwrap_current(raw, CURRENT_EVENT_SCHEMA_VERSION, "Event")
}
