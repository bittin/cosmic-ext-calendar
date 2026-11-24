// SPDX-License-Identifier: GPL-3.0-only

use super::super::{AppModel, Message, SettingsMessage};
use crate::config::Config;
use crate::features::event::location::photon_api_url;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::prelude::*;
use cosmic::Application;

impl AppModel {
    pub(in crate::app) fn update_settings(
        &mut self,
        message: SettingsMessage,
    ) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::new();
        match message {
            SettingsMessage::UpdateConfig(config) => {
                if self.config.photon_endpoint != config.photon_endpoint {
                    self.photon_endpoint_input = None;
                    self.clear_online_locations();
                }
                let theme_changed = self.config.app_theme != config.app_theme;
                let accounts_changed = self.config.disabled_accounts != config.disabled_accounts;
                let visibility_changed =
                    self.config.hidden_remote_calendars != config.hidden_remote_calendars;
                self.config = config;
                if accounts_changed {
                    self.refresh_active_accounts();
                    tasks.push(self.sync_active_accounts());
                } else if visibility_changed {
                    self.refresh_calendar_visibility();
                }
                self.refresh_display_timezone();
                if theme_changed {
                    tasks.push(cosmic::command::set_theme(self.config.app_theme.theme()));
                }
            }
            SettingsMessage::SetTheme(app_theme) => {
                if app_theme != self.config.app_theme {
                    let mut config = self.config.clone();
                    match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                        .and_then(|context| config.set_app_theme(&context, app_theme))
                    {
                        Ok(_) => {
                            self.config = config;
                            tasks.push(cosmic::command::set_theme(app_theme.theme()));
                        }
                        Err(err) => {
                            tasks
                                .push(self.toast(fl!("theme-save-error", error = err.to_string())));
                        }
                    }
                }
            }
            SettingsMessage::SetDisplayTimezone(index) => {
                if let Some(option) = self.display_timezone_options.get(index) {
                    let value = if index == 0 {
                        String::new()
                    } else {
                        option.clone()
                    };
                    if value != self.config.display_timezone {
                        let mut config = self.config.clone();
                        match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                            .and_then(|context| config.set_display_timezone(&context, value))
                        {
                            Ok(_) => {
                                self.config = config;
                                self.refresh_display_timezone();
                            }
                            Err(err) => tasks.push(self.toast(fl!(
                                "display-timezone-save-error",
                                error = err.to_string()
                            ))),
                        }
                    }
                }
            }
            SettingsMessage::PhotonEndpointChanged(value) => {
                self.photon_endpoint_input = Some(value)
            }
            SettingsMessage::SavePhotonEndpoint | SettingsMessage::DisableOnlineLocations => {
                let endpoint = if matches!(message, SettingsMessage::DisableOnlineLocations) {
                    String::new()
                } else {
                    let Some(url) = photon_api_url(self.photon_endpoint_input()) else {
                        return Task::none();
                    };
                    url.into()
                };
                let mut config = self.config.clone();
                match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                    .and_then(|context| config.set_photon_endpoint(&context, endpoint))
                {
                    Ok(_) => {
                        self.config = config;
                        self.photon_endpoint_input = None;
                        self.clear_online_locations();
                    }
                    Err(_) => tasks.push(self.toast(fl!("online-location-save-error"))),
                }
            }
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }
}
