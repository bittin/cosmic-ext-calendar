// SPDX-License-Identifier: GPL-3.0-only

use super::super::{AppModel, Message};
use crate::features::calendar_management::model::CalendarKey;
use crate::features::event::notifications::{check_alerts, NotificationMessage};
use cosmic::prelude::*;

impl AppModel {
    pub(in crate::app) fn update_notifications(
        &mut self,
        message: NotificationMessage,
    ) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::new();
        match message {
            NotificationMessage::CheckAlerts => {
                self.refresh_display_timezone();
                if !self.checking_alerts {
                    self.checking_alerts = true;
                    tasks.push(Task::perform(
                        {
                            let storage = self.storage.clone();
                            let active_accounts = self.active_accounts.clone();
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    let remote_events: Vec<_> = storage.lock().unwrap()
                                        .all_events().into_iter()
                                        .filter(|event| matches!(event.calendar, CalendarKey::Remote { account_id, .. } if active_accounts.contains(&account_id)))
                                        .cloned()
                                        .collect();
                                    check_alerts(&remote_events)
                                }).await
                            }
                        },
                        |result| {
                            match result {
                                Ok(Ok(())) => {}
                                Ok(Err(err)) => tracing::warn!("could not check event alerts: {err}"),
                                Err(err) => tracing::warn!("event alert check stopped: {err}"),
                            }
                            cosmic::action::app(Message::Notifications(NotificationMessage::AlertsChecked))
                        },
                    ));
                }
            }
            NotificationMessage::AlertsChecked => self.checking_alerts = false,
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }
}
