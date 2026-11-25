use cosmic::widget::text_editor;
use time::OffsetDateTime;

use super::{location::OnlinePlaces, state::EventDateField};
use crate::shared::ids::EventId;

#[derive(Debug, Clone)]
pub enum EventMessage {
    AddEvent(OffsetDateTime),
    OpenEventEditor(Option<EventId>, OffsetDateTime, bool),
    CloseEventEditor,
    EscapeEventEditor,
    EventEditorSummaryChanged(String),
    ToggleEventTitleEditing(bool),
    EventEditorNotesChanged(text_editor::Action),
    EventEditorLocationChanged(String),
    SelectEventLocation(String),
    CloseEventLocationSuggestions,
    OnlineLocationsFound(uuid::Uuid, Result<OnlinePlaces, ()>),
    EventEditorAllDayToggled(bool),
    EventEditorTimezoneChanged(String),
    EventEditorFloatingToggled(bool),
    EventEditorTaskToggled(bool),
    OpenEventDatePicker(EventDateField),
    CloseEventDatePicker,
    BrowseEventDatePicker(i32),
    SelectEventDate(jiff::civil::Date),
    EventClockChanged(bool, String),
    CloseEventClockSearch(bool),
    ToggleEventOptions,
    EventEditorRecurrenceChanged(usize),
    EventEditorRecurrenceIntervalChanged(usize),
    EventEditorRecurrenceWeekdayToggled(usize, bool),
    EventEditorRecurrenceEndChanged(usize),
    EventEditorRecurrenceCountChanged(usize),
    EventEditorCalendarChanged(usize),
    SaveEvent,
    EventEditorScopeChanged(usize),
    EventSaved(std::result::Result<(), String>),
    DeleteEvent(EventId),
    DeleteOccurrence(EventId, OffsetDateTime),
    DeleteFollowing(EventId, OffsetDateTime),
    EventDeleted(std::result::Result<EventId, String>),
    SetEventAlert(usize, Option<i64>),
    EditCustomEventAlert(usize, String),
    RemoveEventAlert(usize),
}
