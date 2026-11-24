// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{widget, Element};

use crate::{
    app::{AppModel, Message},
    fl,
};

use super::AccountsAction;

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
        section = section.add(widget::settings::item::builder(account.label()).control(
            widget::toggler(app.account_locally_enabled(id)).on_toggle(move |enabled| {
                Message::Accounts(AccountsAction::SetLocallyEnabled(id, enabled))
            }),
        ));
    }

    section.into()
}
