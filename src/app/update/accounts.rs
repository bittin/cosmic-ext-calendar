// SPDX-License-Identifier: GPL-3.0-only

use super::super::{AppModel, CalendarPopup, Message};
use crate::config::Config;
use crate::features::accounts::{self as accounts_feature, AccountsAction};
use crate::features::calendar_management::model::CalendarKey;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::prelude::*;
use cosmic::Application;

impl AppModel {
    pub(in crate::app) fn update_accounts(
        &mut self,
        action: AccountsAction,
    ) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::new();
        match action {
            AccountsAction::Discover => {
                if self.discovering_accounts {
                    return Task::none();
                }
                self.discovering_accounts = true;
                return Task::perform(accounts_feature::discover(), |result| {
                    cosmic::action::app(Message::Accounts(AccountsAction::Discovered(result)))
                });
            }
            AccountsAction::Discovered(result) => {
                self.discovering_accounts = false;
                self.accounts_checked = true;
                match result {
                    Ok(discovery) => {
                        self.accounts = discovery.accounts;
                        self.watched_account_ids = discovery.watched_ids;
                        self.accounts_error = None;
                    }
                    Err(error) => {
                        self.accounts.clear();
                        self.accounts_error = Some(error);
                    }
                }
                self.refresh_active_accounts();
                return self.sync_active_accounts();
            }
            AccountsAction::Changed => {
                return cosmic::task::message(Message::Accounts(AccountsAction::Discover));
            }
            AccountsAction::OpenAccountsApp => {
                return Task::perform(accounts_feature::open_add_account(), |result| {
                    cosmic::action::app(Message::Accounts(AccountsAction::AccountsAppOpened(
                        result,
                    )))
                });
            }
            AccountsAction::AccountsAppOpened(result) => {
                if let Err(error) = result {
                    tasks.push(self.toast(fl!("accounts-app-launch-error", error = error)));
                }
            }
            AccountsAction::Sync(id) => return self.sync_account(id),
            AccountsAction::Synced(id, request_id, result) => {
                if !matches!(self.account_sync.get(&id), Some(accounts_feature::SyncStatus::Running { request_id: current, .. }) if *current == request_id)
                {
                    return Task::none();
                }

                if self.calendar_popup.as_ref().is_some_and(|popup| matches!(
                    &popup.content,
                    CalendarPopup::Details(_, event, _) if matches!(event.calendar, CalendarKey::Remote { account_id, .. } if account_id == id)
                )) {
                    self.calendar_popup = None;
                }
                let error = match result {
                    Ok(snapshot) => {
                        self.storage
                            .lock()
                            .unwrap()
                            .replace_remote_snapshot(id, snapshot);
                        None
                    }
                    Err(error) => {
                        if matches!(
                            error,
                            accounts_feature::Error::AccessDenied
                                | accounts_feature::Error::ServerAccessDenied
                                | accounts_feature::Error::GoogleAccessDenied
                                | accounts_feature::Error::GoogleApiDisabled
                                | accounts_feature::Error::GoogleInsufficientScope
                                | accounts_feature::Error::MicrosoftAccessDenied
                                | accounts_feature::Error::Disabled
                                | accounts_feature::Error::NeedsReauth
                        ) {
                            self.storage.lock().unwrap().clear_remote_snapshot(id);
                        }
                        Some(error)
                    }
                };
                self.account_sync.insert(
                    id,
                    accounts_feature::SyncStatus::Finished(std::time::Instant::now(), error),
                );
                self.refresh_calendar_visibility();
            }
            AccountsAction::SetLocallyEnabled(id, enabled) => {
                let mut disabled = self.config.disabled_accounts.clone();
                if enabled {
                    disabled.remove(&id);
                } else {
                    disabled.insert(id);
                }
                if disabled != self.config.disabled_accounts {
                    let mut config = self.config.clone();
                    match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                        .and_then(|context| config.set_disabled_accounts(&context, disabled))
                    {
                        Ok(_) => {
                            self.config = config;
                            self.refresh_active_accounts();
                            tasks.push(self.sync_active_accounts());
                        }
                        Err(err) => tasks
                            .push(self.toast(fl!("account-save-error", error = err.to_string()))),
                    }
                }
            }
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }
}
