use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::shared::ids::{CalendarId, EventId};

pub fn data_dir() -> crate::Result<PathBuf> {
    ProjectDirs::from("dev", "edfloreshz", "Calendar")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| {
            crate::Error::Storage("could not determine the calendar data directory".into())
        })
}

pub fn calendar_path(root: &Path, calendar_id: &CalendarId) -> PathBuf {
    root.join("calendars")
        .join(format!("{}.json", calendar_id.0))
}

pub fn event_path(root: &Path, calendar_id: &CalendarId, event_id: &EventId) -> PathBuf {
    root.join("events")
        .join(calendar_id.0.to_string())
        .join(format!("{}.json", event_id.0))
}
