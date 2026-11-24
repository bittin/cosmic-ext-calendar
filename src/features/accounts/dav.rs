use std::collections::{HashMap, HashSet};
use std::time::Duration;

use accounts::{AccountsClient, CalendarCredentials};
use reqwest::{Client, Method, StatusCode, Url};
use roxmltree::{Document, Node};
use uuid::Uuid;

use super::{Account, Error};
use crate::features::{
    calendar_management::model::{CalendarKey, Source},
    event::model::{Event, EventKind, ExternalRef},
    ics,
};
use crate::shared::ids::EventId;

const DAV: &str = "DAV:";
const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
const DISCOVERY: &str = r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
    <d:prop><d:resourcetype/><d:displayname/><d:current-user-principal/>
        <c:calendar-home-set/><c:supported-calendar-component-set/></d:prop>
</d:propfind>"#;
const EVENTS: &str = r#"<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
    <d:prop><d:getetag/><c:calendar-data/></d:prop>
    <c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT"/></c:comp-filter></c:filter>
</c:calendar-query>"#;

#[derive(Clone, Debug)]
pub struct Calendar {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub calendars: Vec<Calendar>,
    pub events: Vec<Event>,
}

#[derive(Debug)]
enum RequestError {
    Unauthorized,
    Other(Error),
}

impl From<Error> for RequestError {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}

pub async fn sync(account: &Account) -> Result<Snapshot, Error> {
    let broker = AccountsClient::connect().await.map_err(Error::from)?;
    let http = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| Error::Failed)?;

    let credentials = broker
        .calendar_credentials(&account.id, false)
        .await
        .map_err(Error::from)?;
    match fetch(&http, account, &credentials).await {
        Err(RequestError::Unauthorized)
            if matches!(credentials, CalendarCredentials::OAuth { .. }) =>
        {
            let refreshed = broker
                .calendar_credentials(&account.id, true)
                .await
                .map_err(Error::from)?;
            fetch(&http, account, &refreshed)
                .await
                .map_err(request_error)
        }
        result => result.map_err(request_error),
    }
}

fn request_error(error: RequestError) -> Error {
    match error {
        RequestError::Unauthorized => Error::NeedsReauth,
        RequestError::Other(error) => error,
    }
}

struct Session<'a> {
    client: &'a Client,
    credentials: &'a CalendarCredentials,
    origin: Url,
}

impl Session<'_> {
    fn resolve(&self, base: &Url, href: &str) -> Result<Url, Error> {
        let url = base.join(href.trim()).map_err(|_| Error::Unsupported)?;
        if url.origin() != self.origin.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Unsupported);
        }
        Ok(url)
    }

    async fn request(
        &self,
        url: &Url,
        method: &str,
        depth: &str,
        body: &'static str,
    ) -> Result<String, RequestError> {
        let method = Method::from_bytes(method.as_bytes()).map_err(|_| Error::Failed)?;
        let request = self
            .client
            .request(method, url.clone())
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body);
        let request = match self.credentials {
            CalendarCredentials::Basic {
                username, password, ..
            } => request.basic_auth(username, Some(password)),
            CalendarCredentials::OAuth { token, .. } => request.bearer_auth(token),
        };

        let mut response = request.send().await.map_err(|_| Error::Failed)?;
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(RequestError::Unauthorized),
            StatusCode::FORBIDDEN => return Err(Error::AccessDenied.into()),
            StatusCode::MULTI_STATUS => {}
            _ => return Err(Error::Failed.into()),
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Failed)? {
            if bytes.len() + chunk.len() > 32 * 1024 * 1024 {
                return Err(Error::Failed.into());
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes).map_err(|_| Error::Failed.into())
    }

    async fn collections(&self) -> Result<Vec<(Url, String)>, RequestError> {
        let xml = self
            .request(&self.origin, "PROPFIND", "0", DISCOVERY)
            .await?;
        let doc = multistatus(&xml)?;
        let mut calendars = self.read_collections(&self.origin, &doc)?;
        if !calendars.is_empty() {
            return Ok(calendars);
        }
        let home = property_href(&doc, CALDAV, "calendar-home-set");
        let home = if let Some(home) = home {
            self.resolve(&self.origin, home)?
        } else if let Some(principal) = property_href(&doc, DAV, "current-user-principal") {
            let principal = self.resolve(&self.origin, principal)?;
            let xml = self.request(&principal, "PROPFIND", "0", DISCOVERY).await?;
            let doc = multistatus(&xml)?;
            let home =
                property_href(&doc, CALDAV, "calendar-home-set").ok_or(Error::Unsupported)?;
            self.resolve(&principal, home)?
        } else {
            self.origin.clone()
        };

        let xml = self.request(&home, "PROPFIND", "1", DISCOVERY).await?;
        let doc = multistatus(&xml)?;
        calendars = self.read_collections(&home, &doc)?;
        calendars.sort_by(|left, right| left.1.cmp(&right.1));
        Ok(calendars)
    }

    fn read_collections(
        &self,
        base: &Url,
        doc: &Document<'_>,
    ) -> Result<Vec<(Url, String)>, Error> {
        let mut seen = HashSet::new();
        let mut calendars = Vec::new();
        for response in responses(doc) {
            if !property(response, DAV, "resourcetype")
                .is_some_and(|kind| child(kind, CALDAV, "calendar").is_some())
            {
                continue;
            }
            if property(response, CALDAV, "supported-calendar-component-set").is_some_and(|set| {
                !set.children().any(|entry| {
                    entry.has_tag_name((CALDAV, "comp"))
                        && entry.attribute("name") == Some("VEVENT")
                })
            }) {
                continue;
            }
            let href = child(response, DAV, "href")
                .and_then(|node| node.text())
                .ok_or(Error::Failed)?;
            let url = self.resolve(base, href)?;
            let name = property(response, DAV, "displayname")
                .and_then(|node| node.text())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(url.path())
                .to_owned();
            if seen.insert(url.clone()) {
                calendars.push((url, name));
            }
        }
        Ok(calendars)
    }
}

async fn fetch(
    client: &Client,
    account: &Account,
    credentials: &CalendarCredentials,
) -> Result<Snapshot, RequestError> {
    let uri = match credentials {
        CalendarCredentials::Basic { uri, .. } | CalendarCredentials::OAuth { uri, .. } => uri,
    };
    let origin = Url::parse(uri).map_err(|_| Error::Unsupported)?;
    if origin.scheme() != "https"
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.fragment().is_some()
    {
        return Err(Error::Unsupported.into());
    }
    let session = Session {
        client,
        credentials,
        origin,
    };
    let mut snapshot = Snapshot {
        calendars: Vec::new(),
        events: Vec::new(),
    };
    for (url, name) in session.collections().await? {
        let xml = session.request(&url, "REPORT", "1", EVENTS).await?;
        let id = url.to_string();
        snapshot
            .events
            .extend(parse_events(&session, account, &url, &xml)?);
        snapshot.calendars.push(Calendar { id, name });
    }
    Ok(snapshot)
}

fn parse_events(
    session: &Session<'_>,
    account: &Account,
    url: &Url,
    xml: &str,
) -> Result<Vec<Event>, Error> {
    let doc = multistatus(xml)?;
    let mut events = Vec::new();
    let mut resources = HashSet::new();
    let calendar_id = url.to_string();
    let target = CalendarKey::Remote {
        account_id: account.id,
        calendar_id: calendar_id.clone(),
    };
    for response in responses(&doc) {
        let data = property(response, CALDAV, "calendar-data")
            .and_then(|node| node.text())
            .ok_or(Error::Failed)?;
        let href = child(response, DAV, "href")
            .and_then(|node| node.text())
            .ok_or(Error::Failed)?;
        let resource = session.resolve(url, href)?.to_string();
        if !resources.insert(resource.clone()) {
            continue;
        }
        let etag = property(response, DAV, "getetag")
            .and_then(|node| node.text())
            .map(str::to_owned);
        let mut parsed = ics::parse_import(data, target.clone())
            .map_err(|_| Error::Failed)?
            .events;
        parsed.retain(|event| matches!(event.kind, EventKind::Event));
        let mut ids = HashMap::new();
        for event in &mut parsed {
            // CalDAV stores a recurring event and its overrides in one resource.
            let identity = format!(
                "{calendar_id}\n{resource}\n{:?}\n{:?}",
                event.ics_uid, event.recurrence_id
            );
            let id = EventId(Uuid::new_v5(&account.id, identity.as_bytes()));
            ids.insert(event.id, id);
            event.id = id;
            event.source = Source::Synced {
                provider: account.provider.clone(),
                account_id: account.id,
            };
            event.external = Some(ExternalRef {
                account_id: account.id,
                provider: account.provider.clone(),
                calendar_id: calendar_id.clone(),
                provider_event_id: resource.clone(),
                etag: etag.clone(),
                last_synced: Some(time::OffsetDateTime::now_utc()),
            });
        }
        for event in &mut parsed {
            event.series_id = event.series_id.and_then(|id| ids.get(&id).copied());
        }
        events.extend(parsed);
    }
    Ok(events)
}

fn multistatus(xml: &str) -> Result<Document<'_>, Error> {
    let doc = Document::parse(xml).map_err(|_| Error::Failed)?;
    if !doc.root_element().has_tag_name((DAV, "multistatus")) {
        return Err(Error::Failed);
    }
    Ok(doc)
}

fn responses<'a, 'input>(doc: &'a Document<'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    doc.root_element()
        .children()
        .filter(|node| node.has_tag_name((DAV, "response")))
}

fn child<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.has_tag_name((namespace, name)))
}

fn property<'a, 'input>(
    response: Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Option<Node<'a, 'input>> {
    response
        .children()
        .filter(|node| node.has_tag_name((DAV, "propstat")))
        .find_map(|status| {
            let code = child(status, DAV, "status")?
                .text()?
                .split_whitespace()
                .nth(1)?;
            if code != "200" {
                return None;
            }
            child(child(status, DAV, "prop")?, namespace, name)
        })
}

fn property_href<'a>(doc: &'a Document<'_>, namespace: &str, name: &str) -> Option<&'a str> {
    responses(doc)
        .find_map(|response| child(property(response, namespace, name)?, DAV, "href")?.text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::storage::LocalStorage;

    #[tokio::test]
    async fn replacing_a_snapshot_preserves_identity_and_removes_deleted_events() {
        let url = Url::parse("https://example.org/calendar/").unwrap();
        let credentials = CalendarCredentials::Basic {
            uri: url.to_string(),
            username: "me".into(),
            password: "test".into(),
        };
        let client = Client::new();
        let session = Session {
            client: &client,
            credentials: &credentials,
            origin: url.clone(),
        };
        let account = Account {
            id: Uuid::new_v4(),
            display_name: "CalDAV".into(),
            email: None,
            provider: "caldav".into(),
        };
        let response = |summary: &str| {
            format!(
                r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
<d:response><d:href>/calendar/one.ics</d:href><d:propstat><d:prop>
<d:getetag>"etag"</d:getetag><c:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:shared-uid
DTSTART:20260101T100000Z
DTEND:20260101T110000Z
SUMMARY:{summary}
END:VEVENT
END:VCALENDAR
</c:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#
            )
        };
        let mut storage = LocalStorage::unavailable("test store".into());
        let first = parse_events(&session, &account, &url, &response("First")).unwrap();
        let id = first[0].id;
        storage.replace_remote_snapshot(
            account.id,
            Snapshot {
                calendars: vec![],
                events: first,
            },
        );

        let updated = parse_events(&session, &account, &url, &response("Updated")).unwrap();
        storage.replace_remote_snapshot(
            account.id,
            Snapshot {
                calendars: vec![],
                events: updated,
            },
        );
        assert_eq!(storage.all_events().len(), 1);
        assert_eq!(storage.get_event(id).unwrap().summary, "Updated");

        let other_origin =
            response("Updated").replace("/calendar/one.ics", "https://other.example/one.ics");
        assert!(matches!(
            parse_events(&session, &account, &url, &other_origin),
            Err(Error::Unsupported)
        ));
        storage.replace_remote_snapshot(
            account.id,
            Snapshot {
                calendars: vec![],
                events: vec![],
            },
        );
        assert!(storage.get_event(id).is_none());
    }
}
