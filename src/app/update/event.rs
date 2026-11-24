// SPDX-License-Identifier: GPL-3.0-only

use super::super::{AppModel, Message};
use crate::features::calendar_management::model::CalendarKey;
use crate::features::calendar_management::CalendarManagementMessage;
use crate::features::event::location::{find_online, photon_api_url, LocationSuggestions};
use crate::features::event::recurrence::{RecurrenceEnd, RecurrenceFreq};
use crate::features::event::state::{AlertDraft, EventEditorState, RecurrenceScope};
use crate::features::event::EventMessage;
use crate::fl;
use crate::shared::ids::EventId;
use cosmic::prelude::*;
use cosmic::widget;
use time::OffsetDateTime;

impl AppModel {
    pub(in crate::app) fn update_event(
        &mut self,
        message: EventMessage,
    ) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::new();
        match message {
            EventMessage::AddEvent(date) => {
                self.calendar_click = None;
                tasks.push(cosmic::task::message(Message::Event(
                    EventMessage::OpenEventEditor(None, date, false),
                )));
            }
            EventMessage::OpenEventEditor(event_id, date, all_day) => {
                if self.editing_event.is_some() {
                    return Task::none();
                }
                let state = if let Some(id) = event_id {
                    let storage = self.storage.lock().unwrap();
                    let Some(event) = storage.get_event(id) else {
                        return Task::none();
                    };
                    if !matches!(event.calendar, CalendarKey::Local(_)) {
                        return Task::none();
                    }
                    let mut state = EventEditorState::from_event(event, date);
                    if state.recurring_series {
                        state.set_recurrence_scope(event, RecurrenceScope::Occurrence);
                    }
                    state
                } else {
                    let calendar = {
                        let storage = self.storage.lock().unwrap();
                        storage
                            .list_calendars()
                            .into_iter()
                            .min_by(|a, b| {
                                (!a.visible, &a.name, a.id).cmp(&(!b.visible, &b.name, b.id))
                            })
                            .map(|meta| CalendarKey::Local(meta.id))
                    };
                    let Some(calendar) = calendar else {
                        return cosmic::task::message(Message::CalendarManagement(
                            CalendarManagementMessage::OpenNewCalendarForm,
                        ));
                    };
                    let timezone = self
                        .calendar
                        .display_timezone
                        .map(|zone| zone.name().to_string())
                        .unwrap_or_default();
                    EventEditorState::new(calendar, date, all_day, timezone)
                };
                self.calendar_click = None;
                self.event_notes = widget::text_editor::Content::with_text(&state.description);
                self.location_lookup = None;
                self.event_locations = LocationSuggestions::new(
                    self.storage
                        .lock()
                        .unwrap()
                        .all_events()
                        .into_iter()
                        .filter(|event| self.event_is_visible(event))
                        .collect::<Vec<_>>(),
                    &state.location,
                );
                if state.editing_title {
                    tasks.push(widget::text_input::focus(
                        crate::features::event::view::title_id(),
                    ));
                }
                self.editing_event = Some(state);
            }
            EventMessage::EscapeEventEditor => {
                if let Some(state) = self.editing_event.as_mut() {
                    if state.date_picker.take().is_none() && state.can_dismiss_with_escape() {
                        self.location_lookup = None;
                        self.editing_event = None;
                    }
                }
            }
            EventMessage::CloseEventEditor => {
                if !self
                    .editing_event
                    .as_ref()
                    .is_some_and(|state| state.writing)
                {
                    self.location_lookup = None;
                    self.editing_event = None;
                }
            }
            EventMessage::EventEditorSummaryChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.summary = value;
                }
            }
            EventMessage::ToggleEventTitleEditing(editing) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.editing_title = editing;
                }
            }
            EventMessage::EventEditorNotesChanged(action) => {
                if let Some(state) = self.editing_event.as_mut() {
                    self.event_notes.perform(action);
                    state.description = self.event_notes.text();
                }
            }
            EventMessage::EventEditorLocationChanged(value) => {
                if let Some(state) = self.editing_event.as_mut().filter(|state| !state.writing) {
                    state.location = value;
                    self.location_lookup = None;
                    self.event_locations.clear_online(&state.location);

                    let query = state.location.trim();
                    if let Some(endpoint) = photon_api_url(&self.config.photon_endpoint)
                        .filter(|_| (3..=256).contains(&query.chars().count()))
                    {
                        let request = uuid::Uuid::new_v4();
                        let (task, handle) =
                            Task::perform(find_online(query.to_owned(), endpoint), move |result| {
                                cosmic::action::app(Message::Event(
                                    EventMessage::OnlineLocationsFound(request, result),
                                ))
                            })
                            .abortable();
                        self.location_lookup = Some((request, handle.abort_on_drop()));
                        tasks.push(task);
                    }
                }
            }
            EventMessage::SelectEventLocation(value) => {
                if let Some(state) = self.editing_event.as_mut().filter(|state| !state.writing) {
                    state.location = value;
                    self.location_lookup = None;
                    self.event_locations.select(&state.location);
                }
            }
            EventMessage::CloseEventLocationSuggestions => self.location_lookup = None,
            EventMessage::OnlineLocationsFound(request, result) => {
                if self
                    .location_lookup
                    .as_ref()
                    .is_some_and(|(pending, _)| *pending == request)
                {
                    self.location_lookup = None;
                    if let Some(state) = self.editing_event.as_ref().filter(|state| !state.writing)
                    {
                        match result {
                            Ok(places) => self.event_locations.set_online(places, &state.location),
                            Err(()) => self.event_locations.online_failed = true,
                        }
                    }
                }
            }
            EventMessage::EventEditorFloatingToggled(floating) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.floating = floating;
                }
            }
            EventMessage::EventEditorTimezoneChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.timezone = value;
                }
            }
            EventMessage::EventEditorAllDayToggled(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.all_day = value;
                }
            }
            EventMessage::EventEditorTaskToggled(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.is_task = value;
                }
            }
            EventMessage::OpenEventDatePicker(field) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if state.date_picker.is_some_and(|(open, _)| open == field) {
                        state.date_picker = None;
                    } else if let Some(date) = state.date(field) {
                        if let Ok(date) = jiff::civil::Date::new(
                            date.year() as i16,
                            u8::from(date.month()) as i8,
                            date.day() as i8,
                        ) {
                            state.date_picker =
                                Some((field, widget::calendar::CalendarModel::new(date, date)));
                        }
                    }
                }
            }
            EventMessage::CloseEventDatePicker => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.date_picker = None;
                }
            }
            EventMessage::BrowseEventDatePicker(delta) => {
                if let Some((_, model)) = self
                    .editing_event
                    .as_mut()
                    .and_then(|state| state.date_picker.as_mut())
                {
                    if let Ok(month) = model.visible.checked_add(jiff::Span::new().months(delta)) {
                        model.visible = month;
                    }
                }
            }
            EventMessage::SelectEventDate(date) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let Some((field, _)) = state.date_picker.take() {
                        if let Ok(date) =
                            time::Month::try_from(date.month() as u8).and_then(|month| {
                                time::Date::from_calendar_date(
                                    i32::from(date.year()),
                                    month,
                                    date.day() as u8,
                                )
                            })
                        {
                            state.select_date(field, date);
                        }
                    }
                }
            }
            EventMessage::EventClockChanged(is_start, input) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.input_clock(is_start, input);
                }
            }
            EventMessage::CloseEventClockSearch(is_start) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.reset_clock_search(is_start);
                }
            }
            EventMessage::ToggleEventOptions => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.more_options = !state.more_options;
                }
            }
            EventMessage::EventEditorRecurrenceChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let Some(freq) = RecurrenceFreq::ALL.get(idx) {
                        state.recurrence.freq = *freq;
                    }
                }
            }
            EventMessage::EventEditorRecurrenceIntervalChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.recurrence.interval = idx as u16 + 1;
                }
            }
            EventMessage::EventEditorRecurrenceWeekdayToggled(idx, on) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if idx < 7 {
                        state.recurrence.by_weekday[idx] = on;
                    }
                }
            }
            EventMessage::EventEditorRecurrenceEndChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.recurrence.end = match idx {
                        1 => RecurrenceEnd::Until(state.start.date() + time::Duration::days(30)),
                        2 => RecurrenceEnd::Count(10),
                        _ => RecurrenceEnd::Never,
                    };
                }
            }
            EventMessage::EventEditorRecurrenceCountChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let RecurrenceEnd::Count(count) = &mut state.recurrence.end {
                        *count = idx as u32 + 1;
                    }
                }
            }
            EventMessage::EventEditorCalendarChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    let storage = self.storage.lock().unwrap();
                    let mut calendars = storage.list_calendars();
                    calendars.sort_by(|a, b| a.name.cmp(&b.name));
                    if let Some(meta) = calendars.get(idx) {
                        state.calendar = CalendarKey::Local(meta.id);
                    }
                }
            }
            EventMessage::EventEditorScopeChanged(scope) => {
                if let Some(state) = self.editing_event.as_mut() {
                    let storage = self.storage.lock().unwrap();
                    if let Some(parent) = state.event_id.and_then(|id| storage.get_event(id)) {
                        if let Some(&scope) = RecurrenceScope::OPTIONS.get(scope) {
                            state.set_recurrence_scope(parent, scope);
                        }
                    }
                }
            }
            EventMessage::SaveEvent => {
                if let Some(state) = self
                    .editing_event
                    .as_mut()
                    .filter(|state| !state.writing && state.validation_error().is_none())
                {
                    state.writing = true;
                    self.location_lookup = None;
                    let state = state.clone();
                    let storage = self.storage.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let mut storage = storage.lock().unwrap();
                                let existing = state
                                    .event_id
                                    .map(|id| {
                                        storage
                                            .get_event(id)
                                            .ok_or_else(|| "event not found".to_string())
                                    })
                                    .transpose()?;
                                let mut event = state.build_event(existing);
                                if state.recurrence_scope == RecurrenceScope::Occurrence {
                                    let parent =
                                        existing.ok_or_else(|| "series not found".to_string())?;
                                    if parent.exdates.contains(&state.occurrence_start)
                                        || parent
                                            .recurrence_cutoff()
                                            .is_some_and(|end| state.occurrence_start >= end)
                                    {
                                        return Err("occurrence was already changed".to_string());
                                    }
                                    event.id = EventId::new();
                                    event.series_id = Some(parent.id);
                                    event.recurrence_id = Some(state.occurrence_start);
                                    event.rrule = None;
                                    event.exdates.clear();
                                    event.rdates.clear();
                                    event.predecessor = None;
                                    event.repeat_before = None;
                                    event.split_before = None;
                                } else if state.recurrence_scope == RecurrenceScope::Following {
                                    let parent =
                                        existing.ok_or_else(|| "series not found".to_string())?;
                                    event
                                        .start_following_series(parent, state.occurrence_start)
                                        .map_err(|err| err.to_string())?;
                                }
                                storage.create_event(event).map_err(|err| err.to_string())
                            })
                            .await
                            .map_err(|err| err.to_string())?
                        },
                        |result| {
                            cosmic::action::app(Message::Event(EventMessage::EventSaved(result)))
                        },
                    ));
                }
            }
            EventMessage::EventSaved(result) => match result {
                Ok(()) => {
                    self.editing_event = None;
                }
                Err(err) => {
                    tracing::error!("failed to save event: {err}");
                    tasks.push(self.toast(fl!("event-save-error")));
                    if let Some(state) = self.editing_event.as_mut() {
                        state.writing = false;
                    }
                }
            },
            EventMessage::DeleteEvent(id) => {
                if !self.begin_event_deletion(id) {
                    return Task::none();
                }
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut storage = storage.lock().unwrap();
                            storage.delete_event(id).map(|()| id)
                        })
                        .await
                        .map_err(|err| err.to_string())?
                        .map_err(|err| err.to_string())
                    },
                    |result| {
                        cosmic::action::app(Message::Event(EventMessage::EventDeleted(result)))
                    },
                ));
            }
            EventMessage::DeleteOccurrence(id, occurrence_start)
            | EventMessage::DeleteFollowing(id, occurrence_start) => {
                if !self.begin_event_deletion(id) {
                    return Task::none();
                }
                let following = matches!(message, EventMessage::DeleteFollowing(..));
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut storage = storage.lock().unwrap();
                            let Some(mut event) = storage.get_event(id).cloned() else {
                                return Err("event not found".to_string());
                            };
                            if following {
                                event.repeat_before = Some(
                                    event
                                        .repeat_before
                                        .map_or(occurrence_start, |end| end.min(occurrence_start)),
                                );
                            } else if !event.exdates.contains(&occurrence_start) {
                                event.exdates.push(occurrence_start);
                            }
                            event.updated_at = OffsetDateTime::now_utc();
                            event.sequence += 1;
                            storage
                                .update_event(event)
                                .map(|()| id)
                                .map_err(|err| err.to_string())
                        })
                        .await
                        .map_err(|err| err.to_string())?
                    },
                    |result| {
                        cosmic::action::app(Message::Event(EventMessage::EventDeleted(result)))
                    },
                ));
            }
            EventMessage::EventDeleted(result) => match result {
                Ok(_id) => {
                    self.editing_event = None;
                }
                Err(err) => {
                    tracing::error!("failed to delete event: {err}");
                    tasks.push(self.toast(fl!("event-delete-error", error = err.as_str())));
                    if let Some(state) = self.editing_event.as_mut() {
                        state.writing = false;
                    }
                }
            },
            EventMessage::SetEventAlert(row, choice) => {
                if let Some(state) = self.editing_event.as_mut().filter(|state| !state.writing) {
                    state.set_alert(row, choice);
                }
            }
            EventMessage::EditCustomEventAlert(row, value) => {
                if let Some(state) = self.editing_event.as_mut().filter(|state| !state.writing) {
                    if let Some(AlertDraft::Custom(input)) = state.alerts.get_mut(row) {
                        *input = value;
                    }
                }
            }
            EventMessage::RemoveEventAlert(row) => {
                if let Some(state) = self.editing_event.as_mut().filter(|state| !state.writing) {
                    if row < state.alerts.len() {
                        state.alerts.remove(row);
                    }
                }
            }
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }
}
