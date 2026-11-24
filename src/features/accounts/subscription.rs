// SPDX-License-Identifier: GPL-3.0-only

use accounts::{proxy::ServicesChangedStream, AccountsClient};
use cosmic::iced::{
    futures::{stream, StreamExt},
    Subscription,
};
use uuid::Uuid;

use crate::app::Message;

use super::AccountsAction;

/// Reloads Calendar's account list when Accounts adds, removes, enables, or
/// reconfigures an account, or when a service toggle changes.
pub fn subscription(ids: Vec<Uuid>) -> Subscription<Message> {
    Subscription::run_with(ids, |ids| {
        stream::unfold(ids.clone(), |ids| async move {
            // Recover from broker startup, restarts, and missed signals.
            tokio::select! {
                result = wait_for_change(&ids) => {
                    if result.is_none() {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
            }
            Some((Message::Accounts(AccountsAction::Changed), ids))
        })
    })
}

async fn wait_for_change(ids: &[Uuid]) -> Option<()> {
    let client = AccountsClient::connect().await.ok()?;
    let mut added = client.receive_account_added().await.ok()?;
    let mut removed = client.receive_account_removed().await.ok()?;
    let mut toggled = client.receive_account_toggled().await.ok()?;
    let services = service_streams(&client, ids).await;

    if services.is_empty() {
        tokio::select! {
            signal = added.next() => { signal?; }
            signal = removed.next() => { signal?; }
            signal = toggled.next() => { signal?; }
        }
    } else {
        let mut services = stream::select_all(services);
        tokio::select! {
            signal = added.next() => { signal?; }
            signal = removed.next() => { signal?; }
            signal = toggled.next() => { signal?; }
            signal = services.next() => { signal?; }
        }
    }

    Some(())
}

async fn service_streams(client: &AccountsClient, ids: &[Uuid]) -> Vec<ServicesChangedStream> {
    let mut streams = Vec::with_capacity(ids.len());
    for id in ids {
        if let Ok(stream) = client.receive_services_changed(id).await {
            streams.push(stream);
        }
    }
    streams
}
