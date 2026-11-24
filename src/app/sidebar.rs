// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeSet, HashMap};

use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::prelude::*;
use cosmic::widget::{self, menu, nav_bar};
use cosmic::Application;
use uuid::Uuid;

use super::{AppModel, Message};
use crate::config::Config;
use crate::features::accounts::{Account, AccountsAction};
use crate::features::calendar_management::{
    model::{calendar_color, CalendarKey},
    view::calendar_color_icon,
    CalendarManagementMessage,
};
use crate::fl;
use crate::shared::{ids::CalendarId, storage::LocalStorage};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Local,
    Account(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Group(Group),
    Calendar(CalendarKey),
}

#[derive(PartialEq, Eq)]
enum Icon {
    Group {
        collapsed: bool,
    },
    Calendar {
        color: Option<String>,
        visible: bool,
    },
}

#[derive(PartialEq, Eq)]
struct Row {
    item: Item,
    label: String,
    indent: u16,
    icon: Icon,
}

#[derive(Default)]
pub(super) struct Sidebar {
    collapsed: BTreeSet<Group>,
    rows: Vec<Row>,
}

impl Sidebar {
    fn group(&self, group: Group, label: String, indent: u16, rows: &mut Vec<Row>) -> bool {
        let collapsed = self.collapsed.contains(&group);
        rows.push(Row {
            item: Item::Group(group),
            label,
            indent,
            icon: Icon::Group { collapsed },
        });
        !collapsed
    }

    fn build_rows(
        &self,
        storage: &LocalStorage,
        accounts: &[Account],
        active_accounts: &BTreeSet<Uuid>,
        config: &Config,
    ) -> Vec<Row> {
        let mut rows = Vec::new();
        if self.group(Group::Local, fl!("local-calendars"), 0, &mut rows) {
            let mut local = storage.list_calendars();
            local.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
            rows.extend(local.into_iter().map(|calendar| Row {
                item: Item::Calendar(CalendarKey::Local(calendar.id)),
                label: calendar.name.clone(),
                indent: 1,
                icon: Icon::Calendar {
                    color: calendar.color.clone(),
                    visible: calendar.visible,
                },
            }));
        }

        for account in accounts
            .iter()
            .filter(|account| active_accounts.contains(&account.id))
        {
            if !self.group(Group::Account(account.id), account.label(), 0, &mut rows) {
                continue;
            }
            let mut calendars: Vec<_> = storage.remote_calendars(account.id).iter().collect();
            calendars.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
            rows.extend(calendars.into_iter().map(|calendar| Row {
                item: Item::Calendar(CalendarKey::Remote {
                    account_id: account.id,
                    calendar_id: calendar.id.clone(),
                }),
                label: calendar.name.clone(),
                indent: 1,
                icon: Icon::Calendar {
                    color: None,
                    visible: !config.remote_calendar_hidden(account.id, &calendar.id),
                },
            }));
        }
        rows
    }
}

impl AppModel {
    pub(super) fn refresh_calendar_nav(&mut self) {
        let rows = self.sidebar.build_rows(
            &self.storage.lock().unwrap(),
            &self.accounts,
            &self.active_accounts,
            &self.config,
        );
        if self.sidebar.rows == rows {
            return;
        }
        let old_ids: Vec<_> = self.nav.iter().collect();
        for id in old_ids {
            self.nav.remove(id);
        }
        for row in &rows {
            let icon = match &row.icon {
                Icon::Group { collapsed } => widget::icon::from_name(if *collapsed {
                    "pan-end-symbolic"
                } else {
                    "pan-down-symbolic"
                })
                .into(),
                Icon::Calendar { color, visible } => {
                    calendar_color_icon(color.as_deref().and_then(calendar_color), *visible)
                }
            };
            self.nav
                .insert()
                .indent(row.indent)
                .text(row.label.clone())
                .icon(icon.icon().size(16))
                .data(row.item.clone());
        }
        self.sidebar.rows = rows;
    }

    pub(super) fn update_sidebar(&mut self, id: nav_bar::Id) -> Task<cosmic::Action<Message>> {
        match self.nav.data::<Item>(id).cloned() {
            Some(Item::Group(group)) => {
                if !self.sidebar.collapsed.remove(&group) {
                    self.sidebar.collapsed.insert(group);
                }
                self.refresh_calendar_nav();
                Task::none()
            }
            Some(Item::Calendar(CalendarKey::Local(calendar_id))) => {
                let visible = self
                    .storage
                    .lock()
                    .unwrap()
                    .get_calendar(calendar_id)
                    .is_some_and(|calendar| !calendar.visible);
                self.update(Message::CalendarManagement(
                    CalendarManagementMessage::ToggleCalendarVisibility(calendar_id, visible),
                ))
            }
            Some(Item::Calendar(CalendarKey::Remote {
                account_id,
                calendar_id,
            })) => {
                let mut hidden = self.config.hidden_remote_calendars.clone();
                let calendars = hidden.entry(account_id).or_default();
                if !calendars.remove(&calendar_id) {
                    calendars.insert(calendar_id);
                }
                if calendars.is_empty() {
                    hidden.remove(&account_id);
                }
                let mut config = self.config.clone();
                match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                    .and_then(|context| config.set_hidden_remote_calendars(&context, hidden))
                {
                    Ok(_) => {
                        self.config = config;
                        self.refresh_calendar_visibility();
                        Task::none()
                    }
                    Err(error) => {
                        self.toast(fl!("calendar-visibility-error", error = error.to_string()))
                    }
                }
            }
            None => Task::none(),
        }
    }

    pub(super) fn sidebar_context_menu(&self) -> Vec<menu::Tree<cosmic::Action<Message>>> {
        menu::nav_context(
            &HashMap::new(),
            self.nav
                .iter()
                .map(|id| match self.nav.data::<Item>(id) {
                    Some(Item::Calendar(CalendarKey::Local(id))) if !self.storage_read_only => {
                        vec![
                            menu::Item::Button(
                                fl!("rename"),
                                None,
                                MenuAction::RenameCalendar(*id),
                            ),
                            menu::Item::Button(
                                fl!("delete-calendar"),
                                None,
                                MenuAction::DeleteCalendar(*id),
                            ),
                        ]
                    }
                    Some(Item::Group(Group::Local)) if !self.storage_read_only => {
                        vec![menu::Item::Button(
                            fl!("new-calendar"),
                            None,
                            MenuAction::NewCalendar,
                        )]
                    }
                    Some(Item::Group(Group::Account(id))) => vec![menu::Item::Button(
                        fl!("accounts-sync-now"),
                        None,
                        MenuAction::SyncAccount(*id),
                    )],
                    _ => Vec::new(),
                })
                .collect(),
        )
    }

    pub(super) fn refresh_calendar_visibility(&mut self) {
        let storage = self.storage.lock().unwrap();
        if let Some((id, _)) = self.calendar.selected_event {
            if !storage
                .get_event(id)
                .is_some_and(|event| self.event_is_visible(&storage, event))
            {
                self.calendar.selected_event = None;
                self.calendar_click = None;
                self.calendar_popup = None;
            }
        }
        drop(storage);
        self.refresh_calendar_nav();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuAction {
    NewCalendar,
    RenameCalendar(CalendarId),
    DeleteCalendar(CalendarId),
    SyncAccount(Uuid),
}

impl menu::action::MenuAction for MenuAction {
    type Message = cosmic::Action<Message>;

    fn message(&self) -> Self::Message {
        use CalendarManagementMessage as Calendar;
        cosmic::Action::App(match *self {
            Self::NewCalendar => Message::CalendarManagement(Calendar::OpenNewCalendarForm),
            Self::RenameCalendar(id) => {
                Message::CalendarManagement(Calendar::StartEditCalendar(id))
            }
            Self::DeleteCalendar(id) => Message::CalendarManagement(Calendar::DeleteCalendar(id)),
            Self::SyncAccount(id) => Message::Accounts(AccountsAction::Sync(id)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::accounts::dav::{Calendar, Snapshot};

    #[test]
    fn lists_accounts_directly_and_keeps_visibility_per_account() {
        let account = |provider: &str| Account {
            id: Uuid::new_v4(),
            display_name: provider.into(),
            email: None,
            provider: provider.into(),
            server: None,
        };
        let accounts = [
            account("google"),
            account("google"),
            account("custom-caldav"),
            account("microsoft"),
        ];
        let [first, second, caldav, inactive] = accounts.each_ref().map(|account| account.id);
        let mut storage = LocalStorage::unavailable("test".into());
        let collection = "https://example.test/calendars/personal/";
        for account in &accounts {
            storage.replace_remote_snapshot(
                account.id,
                Snapshot {
                    calendars: vec![Calendar {
                        id: collection.into(),
                        name: "Personal".into(),
                        sync_token: None,
                    }],
                    events: Vec::new(),
                },
            );
        }

        let mut config = Config::default();
        config
            .hidden_remote_calendars
            .insert(first, BTreeSet::from([collection.into()]));
        let active = BTreeSet::from([first, second, caldav]);
        let mut sidebar = Sidebar::default();
        let rows = sidebar.build_rows(&storage, &accounts, &active, &config);
        let remote: Vec<_> = rows
            .iter()
            .filter_map(|row| match (&row.item, &row.icon) {
                (
                    Item::Calendar(CalendarKey::Remote { account_id, .. }),
                    Icon::Calendar { visible, .. },
                ) => Some((*account_id, *visible)),
                _ => None,
            })
            .collect();
        assert_eq!(remote, [(first, false), (second, true), (caldav, true)]);
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[1].item, Item::Group(Group::Account(first)));
        assert_eq!(rows[1].indent, 0);
        assert_eq!(rows[2].indent, 1);
        assert!(!rows
            .iter()
            .any(|row| row.item == Item::Group(Group::Account(inactive))));

        sidebar.collapsed.insert(Group::Account(first));
        let rows = sidebar.build_rows(&storage, &accounts, &active, &config);
        assert!(!rows.iter().any(|row| matches!(
            row.item,
            Item::Calendar(CalendarKey::Remote { account_id, .. }) if account_id == first
        )));
        assert!(rows.iter().any(|row| matches!(
            row.item,
            Item::Calendar(CalendarKey::Remote { account_id, .. }) if account_id == second
        )));
    }
}
