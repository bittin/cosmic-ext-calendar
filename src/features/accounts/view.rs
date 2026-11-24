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
        let status = app.account_sync_status(id);
        let mut row = widget::settings::item::builder(account.label());
        if enabled {
            match status {
                Some(SyncStatus::Running {
                    previous_error: Some(error),
                    ..
                }) => {
                    row = row.description(fl!("accounts-retrying", error = error.text()));
                }
                Some(SyncStatus::Running { .. }) => row = row.description(fl!("accounts-syncing")),
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
        section = section.add(row.control(toggle));
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
