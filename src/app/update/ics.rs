// SPDX-License-Identifier: GPL-3.0-only

use super::super::{AppModel, ContextPage, IcsImportState, Message};
use crate::features::calendar_management::model::CalendarKey;
use crate::features::event::model::Event;
use crate::features::ics::{self, IcsMessage};
use crate::fl;
use crate::shared::ids::EventId;
use cosmic::dialog::file_chooser;
use cosmic::prelude::*;
use std::collections::HashMap;

impl AppModel {
    pub(in crate::app) fn update_ics(
        &mut self,
        message: IcsMessage,
    ) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::new();
        match message {
            IcsMessage::ImportIcs if self.ics_import.busy => {
                self.show_context_page(ContextPage::IcsImport);
            }
            IcsMessage::ImportIcs => {
                tasks.push(Task::perform(
                    async {
                        let dialog = file_chooser::open::Dialog::new().title(fl!("import-ics"));
                        match dialog.open_file().await {
                            Ok(response) => response.url().to_file_path().ok(),
                            Err(_) => None,
                        }
                    },
                    |path| cosmic::action::app(Message::Ics(IcsMessage::IcsFileChosen(path))),
                ));
            }
            IcsMessage::IcsFileChosen(Some(path)) if !self.ics_import.busy => {
                let storage = self.storage.lock().unwrap();
                let calendar = storage
                    .list_calendars()
                    .into_iter()
                    .min_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)))
                    .map(|calendar| calendar.id);
                drop(storage);
                self.ics_import = IcsImportState {
                    path: Some(path),
                    calendar,
                    ..IcsImportState::default()
                };
                self.show_context_page(ContextPage::IcsImport);
            }
            IcsMessage::IcsFileChosen(_) => {}
            IcsMessage::SelectImportCalendar(id) => self.ics_import.calendar = Some(id),
            IcsMessage::ConfirmIcsImport => {
                if let (Some(path), Some(calendar_id)) = (
                    self.ics_import
                        .path
                        .take_if(|_| !self.ics_import.busy && self.ics_import.calendar.is_some()),
                    self.ics_import.calendar,
                ) {
                    self.ics_import.busy = true;
                    let storage = self.storage.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let text = std::fs::read_to_string(&path)
                                    .map_err(|err| err.to_string())?;
                                let parsed =
                                    ics::parse_import(&text, CalendarKey::Local(calendar_id))
                                        .map_err(|err| err.to_string())?;
                                let mut storage = storage.lock().unwrap();
                                if storage.get_calendar(calendar_id).is_none() {
                                    return Err("destination calendar no longer exists".to_string());
                                }
                                let mut count = 0;
                                let mut skipped = parsed.skipped;
                                let mut resolved_series = HashMap::new();
                                for mut event in parsed.events {
                                    if let Some(series) = &mut event.series_id {
                                        if let Some(existing) = resolved_series.get(series) {
                                            *series = *existing;
                                        }
                                    }
                                    let duplicate =
                                        storage.all_events().into_iter().find(|existing| {
                                            existing.calendar == event.calendar
                                                && existing.recurrence_id == event.recurrence_id
                                                && (existing.id == event.id
                                                    || event.ics_uid.as_deref().is_some_and(
                                                        |uid| {
                                                            existing.ics_uid.as_deref() == Some(uid)
                                                                || existing.id.0.to_string() == uid
                                                        },
                                                    ))
                                        });
                                    if let Some(duplicate) = duplicate {
                                        if event.recurrence_id.is_none() {
                                            resolved_series.insert(event.id, duplicate.id);
                                        }
                                        skipped += 1;
                                        continue;
                                    }
                                    if storage.contains_event_id(event.id) {
                                        let original = event.id;
                                        event.id = EventId::new();
                                        resolved_series.insert(original, event.id);
                                    }
                                    storage.create_event(event).map_err(|err| err.to_string())?;
                                    count += 1;
                                }
                                Ok((count, skipped))
                            })
                            .await
                            .map_err(|err| err.to_string())?
                        },
                        |result| cosmic::action::app(Message::Ics(IcsMessage::IcsImported(result))),
                    ));
                }
            }
            IcsMessage::IcsImported(result) => {
                self.ics_import.busy = false;
                if self.context_page == ContextPage::IcsImport {
                    self.core.set_show_context(false);
                }
                let notice = match result {
                    Ok((imported, skipped)) => fl!(
                        "ics-import-result",
                        imported = imported.to_string(),
                        skipped = skipped.to_string()
                    ),
                    Err(error) => fl!("ics-import-error", error = error.as_str()),
                };
                tasks.push(self.toast(notice));
            }
            IcsMessage::ExportIcs => {
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        let dialog = file_chooser::save::Dialog::new()
                            .title(fl!("export-ics"))
                            .file_name("calendar.ics".to_string());
                        let path = match dialog.save_file().await {
                            Ok(response) => response.url().and_then(|url| url.to_file_path().ok()),
                            Err(_) => None,
                        };
                        let Some(path) = path else {
                            return Ok(None);
                        };

                        tokio::task::spawn_blocking(move || {
                            let storage = storage.lock().unwrap();
                            let events: Vec<Event> = storage.events_for_export().cloned().collect();
                            let calendar = ics::export_events(&events);
                            std::fs::write(&path, calendar)
                                .map(|()| Some(path))
                                .map_err(|err| err.to_string())
                        })
                        .await
                        .map_err(|err| err.to_string())?
                    },
                    |result| cosmic::action::app(Message::Ics(IcsMessage::IcsExported(result))),
                ));
            }
            IcsMessage::IcsExported(result) => match result {
                Ok(Some(path)) => tracing::info!("exported calendar to {}", path.display()),
                Ok(None) => {}
                Err(err) => {
                    tasks.push(self.toast(fl!("calendar-export-error", error = err.as_str())))
                }
            },
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }
}
