// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Index;

use super::super::{AppModel, CalendarClick, CalendarClipboard, Message, Tab};
use crate::features::calendar::{CalendarAction, CalendarMessage, CalendarSlot};
use crate::features::calendar_management::model::CalendarKey;
use crate::features::event::state::EventEditorState;
use crate::features::event::EventMessage;
use crate::fl;
use cosmic::prelude::*;
use cosmic::widget;
use cosmic::Application;

impl AppModel {
    pub(in crate::app) fn update_calendar(
        &mut self,
        message: CalendarMessage,
    ) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::new();
        match message {
            CalendarMessage::CalendarEventClicked(key, anchor) => {
                self.calendar.selected_event = Some(key);
                self.calendar.selected_slot = None;
                if self.calendar_double_click(CalendarClick::Event(key)) {
                    self.open_event_details(key, Some(anchor));
                } else {
                    self.calendar_popup = None;
                }
            }
            CalendarMessage::CalendarEmptySlotClicked(slot) => {
                self.calendar.select_slot(slot);
                self.calendar_popup = None;
                if self.calendar_double_click(CalendarClick::EmptySlot(slot)) {
                    return self.update(self.create_event_at_slot(slot));
                }
                if matches!(slot, CalendarSlot::MonthDay(_)) {
                    self.open_day_agenda();
                }
            }
            CalendarMessage::CloseCalendarPopup => self.calendar_popup = None,
            CalendarMessage::EventAction(action, selection, date) => {
                self.calendar_click = None;
                self.calendar_popup = None;
                match action {
                    CalendarAction::Edit => {
                        if let Some((id, start)) = selection {
                            return self.update(Message::Event(EventMessage::OpenEventEditor(
                                Some(id),
                                start,
                                false,
                            )));
                        }
                    }
                    CalendarAction::Delete => {
                        if self.editing_event.is_some() {
                            return Task::none();
                        }
                        if let Some((id, start)) = selection {
                            let action = {
                                let storage = self.storage.lock().unwrap();
                                storage
                                    .get_event(id)
                                    .filter(|event| matches!(event.calendar, CalendarKey::Local(_)))
                                    .map(|event| {
                                        self.editing_event =
                                            Some(EventEditorState::from_event(event, start));
                                        if event.is_recurring() {
                                            Message::Event(EventMessage::DeleteOccurrence(
                                                id, start,
                                            ))
                                        } else {
                                            Message::Event(EventMessage::DeleteEvent(id))
                                        }
                                    })
                            };
                            if let Some(action) = action {
                                return self.update(action);
                            }
                        }
                    }
                    CalendarAction::Copy | CalendarAction::Cut => {
                        if let Some(key) = selection {
                            self.calendar_clipboard = if matches!(action, CalendarAction::Cut) {
                                self.event_is_editable(key.0)
                                    .then_some(CalendarClipboard::Cut(key))
                            } else {
                                self.storage
                                    .lock()
                                    .unwrap()
                                    .get_event(key.0)
                                    .cloned()
                                    .map(CalendarClipboard::Copy)
                            };
                        }
                    }
                    CalendarAction::Duplicate | CalendarAction::Paste => {
                        let source = if matches!(action, CalendarAction::Duplicate) {
                            selection.and_then(|key| {
                                self.storage
                                    .lock()
                                    .unwrap()
                                    .get_event(key.0)
                                    .cloned()
                                    .map(CalendarClipboard::Copy)
                            })
                        } else {
                            self.calendar_clipboard.clone()
                        };
                        if let Some(source) = source {
                            let cut = matches!(source, CalendarClipboard::Cut(_));
                            match self.paste_occurrence(source, date) {
                                Ok(key) => {
                                    self.calendar.selected_event = Some(key);
                                    self.calendar.selected_slot = None;
                                    if cut {
                                        self.calendar_clipboard = None;
                                    }
                                }
                                Err(err) => {
                                    tracing::error!("failed to paste calendar event: {err}");
                                    tasks.push(self.toast(fl!("calendar-paste-error")));
                                }
                            }
                        }
                    }
                }
            }
            CalendarMessage::TabSelected(tab) => {
                self.calendar_popup = None;
                self.calendar_click = None;
                let changed = self.tabs.active() != tab;
                self.tabs.activate(tab);
                if changed {
                    self.calendar.selected_slot = None;
                    tasks.push(self.scroll_calendar_to_now());
                }
                if !matches!(self.tabs.active_data::<Tab>(), Some(Tab::Month)) {
                    self.close_day_agenda();
                }
            }
            CalendarMessage::SearchChanged(query) => {
                let returning_to_calendar = !self.search_query.is_empty() && query.is_empty();
                self.search_query = query;
                if returning_to_calendar {
                    tasks.push(self.scroll_calendar_to_now());
                }
                if !self.search_query.is_empty() {
                    self.close_day_agenda();
                }
            }
            CalendarMessage::SelectSearchResult(id, start) => {
                self.search_query.clear();
                self.calendar.set_date(start);
                self.calendar.selected_event = Some((id, start));
                self.open_event_details((id, start), None);
            }
            CalendarMessage::ShowDayAgenda(date) => {
                self.calendar
                    .set_date(self.calendar.selected_date.replace_date(date));
                self.open_day_agenda();
            }
            CalendarMessage::CloseDayAgenda => {
                if self.close_day_agenda() {
                    tasks.push(widget::button::focus(
                        crate::features::calendar::month::day_id(
                            self.calendar.selected_date.date(),
                        ),
                    ));
                }
            }
            CalendarMessage::SelectDate(date) => {
                self.calendar.set_date(date);
            }
            CalendarMessage::SelectDay(idx) => {
                let days = self.calendar.days();
                let day = days.index(idx);
                if let Ok(date) = self.calendar.selected_date.replace_day(*day) {
                    self.calendar.set_date(date);
                } else {
                    tracing::error!("failed to select date");
                }
            }
            CalendarMessage::SelectMonth(idx) => {
                let months = self.calendar.months();
                let month = months.index(idx);
                if let Err(err) = self
                    .calendar
                    .select_month_of_year(self.calendar.selected_date.year(), *month)
                {
                    tracing::error!("failed to select month: {err}");
                }
            }
            CalendarMessage::SelectYear(idx) => {
                let years = self.calendar.years();
                let year = years.index(idx);
                if let Err(err) = self
                    .calendar
                    .select_month_of_year(*year, self.calendar.selected_date.month())
                {
                    tracing::error!("failed to select year: {err}");
                }
            }
            CalendarMessage::NavigateToday => {
                self.calendar.set_today();
                tasks.push(self.scroll_calendar_to_now());
            }
            CalendarMessage::NavigateNextDay => {
                if let Err(err) = self.calendar.next_day() {
                    tracing::error!("failed to navigate to next day: {err}");
                }
            }
            CalendarMessage::NavigatePreviousDay => {
                if let Err(err) = self.calendar.previous_day() {
                    tracing::error!("failed to navigate to previous day: {err}");
                }
            }
            CalendarMessage::NavigateNextMonth => {
                if let Err(err) = self.calendar.next_month() {
                    tracing::error!("failed to navigate to next month: {err}");
                }
            }
            CalendarMessage::NavigatePreviousMonth => {
                if let Err(err) = self.calendar.previous_month() {
                    tracing::error!("failed to navigate to previous month: {err}");
                }
            }
            CalendarMessage::NavigateNextYear => {
                if let Err(err) = self.calendar.next_year() {
                    tracing::error!("failed to navigate to next year: {err}");
                }
            }
            CalendarMessage::NavigatePreviousYear => {
                if let Err(err) = self.calendar.previous_year() {
                    tracing::error!("failed to navigate to previous year: {err}");
                }
            }
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }
}
