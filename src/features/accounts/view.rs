// SPDX-License-Identifier: GPL-3.0-only

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
    section = section.add(
        widget::settings::item::builder(fl!("manage-accounts"))
            .description(fl!("manage-accounts-description"))
            .control(
                widget::button::standard(fl!("open"))
                    .on_press(Message::Accounts(AccountsAction::OpenAccountsApp)),
            ),
    );

    if !app.accounts_checked() {
        return section
            .add(widget::text::body(fl!("accounts-checking")))
            .into();
    }

    if let Some(error) = app.accounts_error() {
        return section.add(widget::text::body(error.text())).into();
    }

    let accounts = app.accounts();
    if accounts.is_empty() {
        return section
            .add(widget::text::body(fl!("no-accounts-connected")))
            .into();
    }

    for account in accounts {
        let id = account.id;
        let enabled = app.account_locally_enabled(id);
        let status = app.account_sync_status(id);
        let mut row = widget::settings::item::builder(account.label());
        if enabled {
            match status {
                Some(SyncStatus::Running(_)) => row = row.description(fl!("accounts-syncing")),
                Some(SyncStatus::Finished(_, Some(error))) => row = row.description(error.text()),
                Some(SyncStatus::Finished(_, None)) => {
                    let names = app.remote_calendar_names(id);
                    row = row.description(if names.is_empty() {
                        fl!("accounts-no-calendars")
                    } else {
                        names.join(", ")
                    });
                }
                None => {}
            }
        }
        let toggle = widget::toggler(enabled).on_toggle(move |enabled| {
            Message::Accounts(AccountsAction::SetLocallyEnabled(id, enabled))
        });
        let mut controls = widget::row::with_children(vec![toggle.into()])
            .spacing(cosmic::theme::spacing().space_xs)
            .align_y(cosmic::iced::Alignment::Center);
        if enabled && !matches!(status, Some(SyncStatus::Running(_))) {
            controls = controls.push(
                widget::button::standard(fl!("accounts-sync-now"))
                    .on_press(Message::Accounts(AccountsAction::Sync(id))),
            );
        }
        section = section.add(row.control(controls));
    }

    section.into()
}
