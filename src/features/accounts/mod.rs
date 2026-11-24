// SPDX-License-Identifier: GPL-3.0-only

pub mod dav;
pub mod subscription;
pub mod view;

use crate::fl;
use accounts::{AccountsClient, AccountsError, CalendarClient};
use uuid::Uuid;

/// Calendar's local view of an Accounts account.
///
/// The broker owns the account and its settings; Calendar only keeps the
/// fields it needs to present and filter remote calendars.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Account {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub provider: String,
}

impl Account {
    pub fn label(&self) -> String {
        match self.email.as_deref() {
            Some(email) if !email.is_empty() => format!("{} ({email})", self.display_name),
            _ => self.display_name.clone(),
        }
    }

    fn from_broker(account: &accounts::models::Account) -> Self {
        Self {
            id: account.id,
            display_name: account.display_name.clone(),
            email: account.email.clone(),
            provider: account.provider.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Unavailable,
    UpgradeRequired,
    Disabled,
    NeedsReauth,
    AccessDenied,
    ConsentTimeout,
    Failed,
    Unsupported,
    MicrosoftGraphRequired,
}

impl From<AccountsError> for Error {
    fn from(error: AccountsError) -> Self {
        match error {
            AccountsError::Unavailable => Self::Unavailable,
            AccountsError::UpgradeRequired => Self::UpgradeRequired,
            AccountsError::Disabled => Self::Disabled,
            AccountsError::NeedsReauth => Self::NeedsReauth,
            AccountsError::AccessDenied => Self::AccessDenied,
            AccountsError::ConsentTimeout => Self::ConsentTimeout,
            AccountsError::Failed => Self::Failed,
            AccountsError::Unsupported => Self::Unsupported,
        }
    }
}

impl Error {
    pub fn text(self) -> String {
        match self {
            Self::Unavailable => fl!("accounts-error-unavailable"),
            Self::UpgradeRequired => fl!("accounts-error-upgrade"),
            Self::Disabled => fl!("accounts-error-disabled"),
            Self::NeedsReauth => fl!("accounts-error-needs-reauth"),
            Self::AccessDenied => fl!("accounts-error-access-denied"),
            Self::ConsentTimeout => fl!("accounts-error-consent-timeout"),
            Self::Failed => fl!("accounts-error-failed"),
            Self::Unsupported => fl!("accounts-error-unsupported"),
            Self::MicrosoftGraphRequired => fl!("accounts-error-microsoft-graph"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum AccountsAction {
    Discover,
    Discovered(Result<Discovery, Error>),
    Changed,
    OpenAccountsApp,
    AccountsAppOpened(Result<(), String>),
    SetLocallyEnabled(Uuid, bool),
    Sync(Uuid),
    Synced(Uuid, u64, Result<dav::Snapshot, Error>),
}

#[derive(Clone, Debug)]
pub struct Discovery {
    pub accounts: Vec<Account>,
    pub watched_ids: Vec<Uuid>,
}

/// Lists broker accounts that can supply calendar data.
///
/// The shared helper skips broken account objects and accounts without a
/// usable CalDAV endpoint. The typed endpoint client then rejects accounts
/// advertising an authentication scheme Calendar cannot use.
pub async fn discover() -> Result<Discovery, Error> {
    let client = AccountsClient::connect().await.map_err(Error::from)?;
    // A broken account object can make the strict list fail. Keep usable
    // accounts discoverable in that case and rely on the periodic recheck
    // for any service signals we could not subscribe to.
    let mut watched_ids: Vec<_> = client
        .list_accounts()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|account| account.id)
        .collect();
    let mut accounts = Vec::new();

    for (account, _) in client.list_calendar_accounts().await.map_err(Error::from)? {
        let Ok(endpoint) = CalendarClient::connect(&account).await else {
            continue;
        };
        if endpoint.auth_method_kind().await.is_err() {
            continue;
        }
        watched_ids.push(account.id);
        accounts.push(Account::from_broker(&account));
    }

    watched_ids.sort_unstable();
    watched_ids.dedup();
    accounts.sort_by_key(Account::label);
    Ok(Discovery {
        accounts,
        watched_ids,
    })
}

pub enum SyncStatus {
    Running(u64),
    Finished(std::time::Instant, Option<Error>),
}

pub async fn open_add_account() -> Result<(), String> {
    if std::path::Path::new("/.flatpak-info").is_file() {
        use cosmic::dialog::ashpd::desktop::open_uri::OpenFileRequest;

        let uri = reqwest::Url::parse("cosmic-accounts://add-account")
            .expect("valid Accounts activation URI");
        OpenFileRequest::default()
            .send_uri(&uri)
            .await
            .and_then(|request| request.response())
            .map_err(|error| error.to_string())
    } else {
        accounts::ui::open_add_account().map_err(|error| error.to_string())
    }
}
