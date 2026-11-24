// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use cosmic::theme;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
pub enum AppTheme {
    #[default]
    System,
    Dark,
    Light,
}

impl AppTheme {
    pub const OPTIONS: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    pub fn theme(self) -> theme::Theme {
        let (mut theme, prefer_dark) = match self {
            Self::System => return theme::system_preference(),
            Self::Dark => (theme::system_dark(), true),
            Self::Light => (theme::system_light(), false),
        };
        theme.theme_type.prefer_dark(Some(prefer_dark));
        theme
    }
}

#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    demo: String,
    pub display_timezone: String,
    pub app_theme: AppTheme,
    pub photon_endpoint: String,
    /// Accounts whose calendar data Calendar should ignore locally. The
    /// broker still owns whether the Calendar service is available at all.
    pub disabled_accounts: BTreeSet<Uuid>,
}
