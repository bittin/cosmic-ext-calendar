// SPDX-License-Identifier: GPL-3.0-only

mod dav;
mod graph;
pub mod subscription;
pub mod view;

use crate::{features::event::model::Event, fl};
use accounts::{models::Service, AccountsClient, AccountsError};
use uuid::Uuid;

/// Calendar's local view of an Accounts account.
///
/// The broker owns the account and its settings; Calendar only keeps the
/// fields it needs to present and filter remote calendars.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Account {
    pub id: Uuid,
    pub email: Option<String>,
    pub provider: String,
    pub server: Option<String>,
}

impl Account {
    pub fn address(&self) -> Option<&str> {
        self.email
            .as_deref()
            .filter(|email| !email.is_empty())
            .or(self.server.as_deref())
    }

    pub fn provider_label(&self) -> &str {
        match self.provider.as_str() {
            "google" => "Google",
            "microsoft" => "Microsoft",
            accounts::models::CUSTOM_CALDAV_PROVIDER => "CalDAV",
            provider => provider,
        }
    }

    fn from_broker(account: &accounts::models::Account) -> Self {
        let server = account
            .caldav_url
            .as_deref()
            .and_then(|url| reqwest::Url::parse(url).ok())
            .and_then(|url| {
                url.host_str().map(|host| {
                    url.port()
                        .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"))
                })
            });
        Self {
            id: account.id,
            email: account.email.clone(),
            provider: account.provider.clone(),
            server,
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
    ServerAccessDenied,
    GoogleAccessDenied,
    GoogleApiDisabled,
    GoogleInsufficientScope,
    ConsentTimeout,
    Failed,
    Unsupported,
    MicrosoftAccessDenied,
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
            Self::ServerAccessDenied => fl!("accounts-error-server-denied"),
            Self::GoogleAccessDenied => fl!("accounts-error-google-denied"),
            Self::GoogleApiDisabled => fl!("accounts-error-google-api-disabled"),
            Self::GoogleInsufficientScope => fl!("accounts-error-google-scope"),
            Self::ConsentTimeout => fl!("accounts-error-consent-timeout"),
            Self::Failed => fl!("accounts-error-failed"),
            Self::Unsupported => fl!("accounts-error-unsupported"),
            Self::MicrosoftAccessDenied => fl!("accounts-error-microsoft-denied"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Calendar {
    pub id: String,
    pub name: String,
    pub sync_token: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub calendars: Vec<Calendar>,
    pub events: Vec<Event>,
}

pub async fn sync(account: &Account, previous: Snapshot) -> Result<Snapshot, Error> {
    match account.provider.as_str() {
        "microsoft" => graph::sync(account).await,
        _ => dav::sync(account, previous).await,
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
    Synced(Uuid, u64, Result<Snapshot, Error>),
}

#[derive(Clone, Debug)]
pub struct Discovery {
    pub accounts: Vec<Account>,
    pub watched_ids: Vec<Uuid>,
}

/// Keep enabled Calendar accounts discoverable even if their endpoint cannot
/// be opened. Sync validates the endpoint and reports its error per account.
pub async fn discover() -> Result<Discovery, Error> {
    let client = AccountsClient::connect().await.map_err(Error::from)?;
    let (eligible, mut watched_ids) = match client.list_accounts().await {
        Ok(accounts) => {
            let watched = accounts.iter().map(|account| account.id).collect();
            let eligible = accounts
                .into_iter()
                .filter(|account| account.is_enabled_for(&Service::Calendar))
                .collect::<Vec<_>>();
            (eligible, watched)
        }
        Err(_) => {
            // The tolerant helper still returns usable accounts when one
            // account object could not be read by the strict listing.
            let eligible: Vec<_> = client
                .list_calendar_accounts()
                .await
                .map_err(Error::from)?
                .into_iter()
                .map(|(account, _)| account)
                .collect();
            let watched = eligible
                .iter()
                .map(|account| account.id)
                .collect::<Vec<_>>();
            (eligible, watched)
        }
    };
    watched_ids.sort_unstable();
    watched_ids.dedup();
    let mut accounts: Vec<_> = eligible.iter().map(Account::from_broker).collect();
    accounts.sort_by(|a, b| (a.address(), a.id).cmp(&(b.address(), b.id)));
    Ok(Discovery {
        accounts,
        watched_ids,
    })
}

pub enum SyncStatus {
    Running {
        request_id: u64,
        previous_error: Option<Error>,
    },
    Finished(std::time::Instant, Option<Error>),
}

impl SyncStatus {
    pub fn error(&self) -> Option<Error> {
        match self {
            Self::Running { previous_error, .. } => *previous_error,
            Self::Finished(_, error) => *error,
        }
    }
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
