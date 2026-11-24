// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::{widget::text::Wrapping, Alignment, Length};
use cosmic::{widget, Element};

use crate::{
    app::{AppModel, Message},
    fl,
};

use super::{AccountsAction, SyncStatus};

/// Accounts section rendered inside Calendar's Settings drawer.
///
/// Accounts are created and removed in the Accounts app. This section only
/// lists accounts that expose the Calendar service and lets the user pause
/// each one locally.
pub fn settings_view(app: &AppModel) -> Element<'_, Message> {
    let mut section = widget::settings::section().title(fl!("accounts"));
    if !app.accounts_checked() {
        section = section.add(widget::text::body(fl!("accounts-checking")));
    } else if let Some(error) = app.accounts_error() {
        section = section.add(widget::text::body(error.text()));
    } else if app.accounts().is_empty() {
        section = section.add(widget::text::body(fl!("no-accounts-connected")));
    }

    for account in app.accounts() {
        let id = account.id;
        let enabled = app.account_locally_enabled(id);
        let mut controls = widget::row::with_capacity(2)
            .spacing(cosmic::theme::spacing().space_xs)
            .align_y(Alignment::Center);
        if enabled {
            let indicator = match app.account_sync_status(id) {
                Some(SyncStatus::Running {
                    previous_error: Some(error),
                    ..
                }) => Some((
                    "dialog-warning-symbolic",
                    fl!("accounts-retrying", error = error.text()),
                )),
                Some(SyncStatus::Finished(_, Some(error))) => {
                    Some(("dialog-warning-symbolic", error.text()))
                }
                Some(SyncStatus::Running { .. }) => {
                    Some(("dialog-information-symbolic", fl!("accounts-syncing")))
                }
                Some(SyncStatus::Finished(_, None)) if !app.account_has_calendars(id) => {
                    Some(("dialog-information-symbolic", fl!("accounts-no-calendars")))
                }
                _ => None,
            };
            if let Some((icon, message)) = indicator {
                controls = controls.push(widget::tooltip(
                    widget::icon::from_name(icon).size(16).icon(),
                    widget::text::body(message)
                        .width(Length::Fixed(240.0))
                        .wrapping(Wrapping::Word),
                    widget::tooltip::Position::Bottom,
                ));
            }
        }
        let toggle = widget::toggler(enabled).on_toggle(move |enabled| {
            Message::Accounts(AccountsAction::SetLocallyEnabled(id, enabled))
        });
        let address = account
            .address()
            .map_or_else(|| fl!("sidebar-online-account"), str::to_owned);
        section = section.add(
            widget::settings::item::builder(address)
                .description(account.provider_label())
                .control(controls.push(toggle)),
        );
    }

    section
        .add(
            widget::settings::item::builder(fl!("manage-accounts"))
                .description(fl!("manage-accounts-description"))
                .control(
                    widget::button::standard(fl!("open"))
                        .on_press(Message::Accounts(AccountsAction::OpenAccountsApp)),
                ),
        )
        .into()
}
