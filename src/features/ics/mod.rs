//! iCalendar (`.ics`) import/export.
//!
//! Enabling `icalendar`'s `time` feature lets us hand `time::Date` /
//! `time::OffsetDateTime` straight to its builder API, so the only manual
//! `chrono` <-> `time` conversion needed here is on the *read* path
//! (`icalendar` only exposes parsed date-times as `chrono` types).

mod message;
pub use message::IcsMessage;

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDate, Offset, TimeZone, Utc};
use icalendar::{
    Alarm, Calendar as IcsCalendar, CalendarComponent, CalendarDateTime, Component,
    DatePerhapsTime, Event as IcsEvent, EventLike, EventStatus, Property, Todo as IcsTodo, Trigger,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::features::calendar_management::model::{CalendarKey, Source};
use crate::features::event::model::{Event, EventKind, EventTime, Reminder, TaskDetails};
use crate::features::event::resolve_series_occurrence;
use crate::shared::ids::EventId;
use crate::Result;
mod timezones;

fn from_utc(dt: DateTime<Utc>) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(dt.timestamp()).unwrap_or(OffsetDateTime::UNIX_EPOCH)
        + time::Duration::nanoseconds(i64::from(dt.timestamp_subsec_nanos()))
}

fn to_utc(dt: OffsetDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(dt.unix_timestamp(), dt.nanosecond())
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
}

fn from_naive_date(date: NaiveDate) -> time::Date {
    use chrono::Datelike;
    time::Month::try_from(date.month() as u8)
        .ok()
        .and_then(|month| time::Date::from_calendar_date(date.year(), month, date.day() as u8).ok())
        .unwrap_or(time::Date::MIN)
}

fn resolve_zone(name: &str, aliases: &BTreeMap<String, chrono_tz::Tz>) -> Option<chrono_tz::Tz> {
    aliases.get(name).copied().or_else(|| name.parse().ok())
}

fn calendar_date_time_to_offset(
    dt: &CalendarDateTime,
    aliases: &BTreeMap<String, chrono_tz::Tz>,
) -> Result<OffsetDateTime> {
    let (utc, offset) = match dt {
        CalendarDateTime::Utc(dt) => (*dt, 0),
        CalendarDateTime::Floating(dt) => (dt.and_utc(), 0),
        CalendarDateTime::WithTimezone { date_time, tzid } => {
            let zone = resolve_zone(tzid, aliases).ok_or_else(|| {
                crate::Error::Storage(format!("unsupported calendar time zone: {tzid}"))
            })?;
            let local = zone
                .from_local_datetime(date_time)
                .earliest()
                .or_else(|| {
                    // RFC 5545 interprets an explicitly supplied time in a gap
                    // using the offset immediately before the transition.
                    let (_, previous) = chrono_tz::GapInfo::new(date_time, &zone)?.begin?;
                    let utc = date_time.checked_sub_signed(chrono::Duration::seconds(
                        i64::from(previous.fix().local_minus_utc()),
                    ))?;
                    Some(zone.from_utc_datetime(&utc))
                })
                .ok_or_else(|| {
                    crate::Error::Storage(format!("invalid calendar time in {tzid}: {date_time}"))
                })?;
            (local.to_utc(), local.offset().fix().local_minus_utc())
        }
    };
    let offset = time::UtcOffset::from_whole_seconds(offset).unwrap_or(time::UtcOffset::UTC);
    Ok(from_utc(utc).to_offset(offset))
}

fn date_perhaps_time_to_event_time(
    dt: DatePerhapsTime,
    aliases: &BTreeMap<String, chrono_tz::Tz>,
) -> Result<OffsetDateTime> {
    match dt {
        DatePerhapsTime::DateTime(dt) => calendar_date_time_to_offset(&dt, aliases),
        DatePerhapsTime::Date(date) => Ok(from_naive_date(date).midnight().assume_utc()),
    }
}

fn export_timed(dt: OffsetDateTime, event: &Event) -> DatePerhapsTime {
    if event.floating {
        return CalendarDateTime::Floating(
            to_utc(dt.replace_offset(time::UtcOffset::UTC)).naive_utc(),
        )
        .into();
    }
    let timezone = event.timezone.as_deref();
    if let Some((name, zone)) =
        timezone.and_then(|name| name.parse::<chrono_tz::Tz>().ok().map(|zone| (name, zone)))
    {
        let date_time = to_utc(dt).with_timezone(&zone).naive_local();
        CalendarDateTime::WithTimezone {
            date_time,
            tzid: name.into(),
        }
        .into()
    } else {
        dt.into()
    }
}

fn is_all_day(dt: &DatePerhapsTime) -> bool {
    matches!(dt, DatePerhapsTime::Date(_))
}

fn export_rule(event: &Event) -> Option<String> {
    let rule = event.rrule.as_deref()?;
    let Some(cutoff) = event.recurrence_cutoff() else {
        return Some(
            rule.split(';')
                .map(|field| {
                    let Some(until) = field.strip_prefix("UNTIL=") else {
                        return field.to_string();
                    };
                    if matches!(event.time, EventTime::AllDay { .. }) {
                        if let Some(date) = until.get(..8) {
                            return format!("UNTIL={date}");
                        }
                    } else if event.floating {
                        return format!("UNTIL={}", until.trim_end_matches('Z'));
                    }
                    field.to_string()
                })
                .collect::<Vec<_>>()
                .join(";"),
        );
    };
    let fields = rule
        .split(';')
        .filter(|field| !field.starts_with("COUNT=") && !field.starts_with("UNTIL="))
        .collect::<Vec<_>>()
        .join(";");
    if let Some(slots) = event.recurrence_slots_before(cutoff) {
        return Some(format!("{fields};COUNT={slots}"));
    }
    let until = to_utc(cutoff.saturating_sub(time::Duration::SECOND));
    let until = match event.time {
        EventTime::AllDay { .. } => until.format("%Y%m%d").to_string(),
        EventTime::Timed { .. } if event.floating => until.format("%Y%m%dT%H%M%S").to_string(),
        EventTime::Timed { .. } => until.format("%Y%m%dT%H%M%SZ").to_string(),
    };
    Some(format!("{fields};UNTIL={until}"))
}

fn export_exception_links(events: &[Event]) -> HashMap<EventId, (EventId, OffsetDateTime)> {
    events
        .iter()
        .filter_map(|event| {
            let (series, occurrence) = event.series_id.zip(event.recurrence_id)?;
            let link = resolve_series_occurrence(series, occurrence, events.iter());
            Some((event.id, link))
        })
        .collect()
}

/// Builds an in-memory `.ics` document from `events`.
pub fn export_events(events: &[Event]) -> String {
    let mut calendar = IcsCalendar::new();
    let exception_links = export_exception_links(events);
    for event in events {
        if event
            .recurrence_cutoff()
            .is_some_and(|cutoff| cutoff <= event.first_possible_occurrence())
        {
            continue;
        }
        match &event.kind {
            EventKind::Event => {
                let mut ics_event = IcsEvent::new();
                apply_common(&mut ics_event, event);
                match &event.time {
                    EventTime::Timed { start, end } => {
                        ics_event.starts(export_timed(*start, event));
                        if end > start {
                            ics_event.ends(export_timed(*end, event));
                        }
                    }
                    EventTime::AllDay { start, end } => {
                        ics_event.starts(*start);
                        ics_event.ends(*end);
                    }
                }
                if let Some(rrule) = export_rule(event).filter(|rule| !rule.ends_with(";COUNT=0")) {
                    ics_event.add_property("RRULE", rrule);
                }
                let exception_date = |dt: OffsetDateTime| match event.time {
                    EventTime::AllDay { .. } => DatePerhapsTime::from(dt.date()),
                    EventTime::Timed { .. } => export_timed(dt, event),
                };
                if event
                    .recurrence_cutoff()
                    .is_some_and(|end| event.start() >= end)
                {
                    ics_event
                        .append_multi_property(exception_date(event.start()).to_property("EXDATE"));
                }
                for &exdate in &event.exdates {
                    let replaced = exception_links
                        .values()
                        .any(|link| *link == (event.id, exdate));
                    if event.recurrence_cutoff().is_some_and(|end| exdate >= end) {
                        continue;
                    }
                    if !replaced {
                        ics_event
                            .append_multi_property(exception_date(exdate).to_property("EXDATE"));
                    }
                }
                for &date in &event.rdates {
                    if event.recurrence_cutoff().is_none_or(|end| date < end) {
                        ics_event.append_multi_property(exception_date(date).to_property("RDATE"));
                    }
                }
                if let Some(recurrence_id) = event.recurrence_id {
                    let (parent_id, recurrence_id) = exception_links
                        .get(&event.id)
                        .map_or((event.series_id, recurrence_id), |(series, date)| {
                            (Some(*series), *date)
                        });
                    let parent =
                        parent_id.and_then(|id| events.iter().find(|event| event.id == id));
                    let recurrence_value = match parent {
                        Some(parent) => {
                            let uid = parent.id.0.to_string();
                            ics_event.uid(parent.ics_uid.as_deref().unwrap_or(&uid));
                            match parent.time {
                                EventTime::AllDay { .. } => {
                                    DatePerhapsTime::from(recurrence_id.date())
                                }
                                EventTime::Timed { .. } => export_timed(recurrence_id, parent),
                            }
                        }
                        None => exception_date(recurrence_id),
                    };
                    ics_event.recurrence_id(recurrence_value);
                }
                calendar.push(ics_event.done());
            }
            EventKind::Task(details) => {
                let mut ics_todo = IcsTodo::new();
                apply_common(&mut ics_todo, event);
                match event.time {
                    EventTime::AllDay { start, .. } => ics_todo.starts(start),
                    EventTime::Timed { start, .. } => ics_todo.starts(export_timed(start, event)),
                };
                if let Some(due) = details.due {
                    match event.time {
                        EventTime::AllDay { .. } => ics_todo.due(due.date()),
                        EventTime::Timed { .. } => ics_todo.due(export_timed(due, event)),
                    };
                }
                if let Some(priority) = details.priority {
                    ics_todo.priority(u32::from(priority));
                }
                if let Some(completed) = details.completed_at {
                    ics_todo.completed(to_utc(completed));
                }
                calendar.push(ics_todo.done());
            }
        }
    }
    let serialized = calendar.done().to_string();
    let mut output = String::with_capacity(serialized.len());
    let mut in_alarm = false;
    for line in serialized.split_inclusive("\r\n") {
        match line.trim_end() {
            "BEGIN:VALARM" => in_alarm = true,
            "END:VALARM" => in_alarm = false,
            "END:VCALENDAR" => {
                // The generic serializer adds event-only metadata to
                // auxiliary components. Write time zone definitions directly.
                for zone in timezones::export_zones(events) {
                    output.push_str(&zone);
                }
            }
            _ if in_alarm && (line.starts_with("DTSTAMP:") || line.starts_with("UID:")) => continue,
            _ => {}
        }
        output.push_str(line);
    }
    output
}

fn apply_common(component: &mut impl EventLike, event: &Event) {
    let generated_uid = event.series_id.unwrap_or(event.id).0.to_string();
    component.uid(event.ics_uid.as_deref().unwrap_or(&generated_uid));
    component.summary(&event.summary);
    if let Some(description) = &event.description {
        component.description(description);
    }
    if let Some(location) = &event.location {
        component.location(location);
    }
    component.sequence(event.sequence);
    for reminder in &event.reminders {
        if let Some(duration) = reminder
            .minutes_before
            .checked_neg()
            .filter(|minutes| *minutes <= 0)
            .and_then(chrono::Duration::try_minutes)
        {
            component.alarm(Alarm::display(&event.summary, duration));
        }
    }
}

fn import_recurrence_dates(
    component: &impl Component,
    key: &str,
    aliases: &BTreeMap<String, chrono_tz::Tz>,
) -> Result<Vec<OffsetDateTime>> {
    component
        .multi_properties()
        .get(key)
        .into_iter()
        .flatten()
        .flat_map(|property| {
            property.value().split(',').map(move |value| {
                let mut entry = Property::new(key, value);
                for parameter in property.params().values() {
                    entry.append_parameter(parameter.clone());
                }
                let date = DatePerhapsTime::from_property(&entry).ok_or_else(|| {
                    crate::Error::Storage(format!("invalid {key} value: {value}"))
                })?;
                date_perhaps_time_to_event_time(date, aliases)
            })
        })
        .collect()
}

fn signed_duration_seconds(property: &Property) -> Option<i64> {
    let value = property.value();
    let (sign, magnitude) = if let Some(unsigned) = value.strip_prefix('-') {
        (-1, unsigned)
    } else {
        (1, value.strip_prefix('+').unwrap_or(value))
    };
    // The underlying ISO parser represents a duration as an unsigned
    // magnitude. Retain the iCalendar sign when parsing a relative trigger.
    let mut trigger = Property::new("TRIGGER", magnitude);
    for parameter in property.params().values() {
        trigger.append_parameter(parameter.clone());
    }
    match Trigger::try_from(&trigger).ok()? {
        Trigger::Duration(duration, _) => duration.num_seconds().checked_mul(sign),
        Trigger::DateTime(_) => None,
    }
}

fn import_event_duration(component: &impl Component, all_day: bool) -> Result<time::Duration> {
    let Some(value) = component.property_value("DURATION") else {
        return Ok(if all_day {
            time::Duration::DAY
        } else {
            time::Duration::ZERO
        });
    };
    let duration = signed_duration_seconds(&Property::new("DURATION", value)).unwrap_or_default();
    if duration <= 0 || (all_day && duration % 86_400 != 0) {
        return Err(crate::Error::Storage(format!(
            "invalid event duration: {value}"
        )));
    }
    Ok(time::Duration::seconds(duration))
}

fn import_reminders(component: &impl Component) -> Vec<Reminder> {
    component
        .components()
        .iter()
        .filter(|alarm| alarm.component_kind() == "VALARM")
        .filter_map(|alarm| {
            let trigger = alarm.properties().get("TRIGGER")?;
            if trigger
                .params()
                .get("RELATED")
                .is_some_and(|parameter| parameter.value() != "START")
            {
                return None;
            }
            let seconds = signed_duration_seconds(trigger)?;
            (seconds <= 0 && seconds % 60 == 0).then(|| Reminder {
                minutes_before: -seconds / 60,
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct ParsedImport {
    pub events: Vec<Event>,
    pub skipped: usize,
}

#[cfg(test)]
fn import_ics(text: &str, target: CalendarKey) -> Result<Vec<Event>> {
    Ok(parse_import(text, target)?.events)
}

/// Parses `text` as an iCalendar document and converts its `VEVENT`/`VTODO`
/// components into [`Event`]s attached to `target`.
pub fn parse_import(text: &str, target: CalendarKey) -> Result<ParsedImport> {
    let calendar: IcsCalendar = text
        .parse()
        .map_err(|err: String| crate::Error::Storage(format!("invalid ICS file: {err}")))?;

    let aliases = timezones::import_aliases(&calendar.components);
    let convert_time = |value| date_perhaps_time_to_event_time(value, &aliases);
    let now = OffsetDateTime::now_utc();
    let mut events = Vec::new();
    let mut cancellations = Vec::new();

    let mut skipped = 0;
    for component in &calendar.components {
        if matches!(component, CalendarComponent::Other(zone) if zone.component_kind() == "VTIMEZONE")
        {
            continue;
        }
        if let CalendarComponent::Event(event) = component {
            if event.get_status() == Some(EventStatus::Cancelled) {
                match (
                    event.get_uid(),
                    event.get_recurrence_id().map(convert_time).transpose(),
                ) {
                    (Some(uid), Ok(Some(date))) => cancellations.push((
                        uid.to_owned(),
                        date,
                        event.get_sequence().unwrap_or_default(),
                    )),
                    _ => skipped += 1,
                }
                continue;
            }
        }
        let parsed = (|| -> Result<Option<Event>> {
            match component {
                CalendarComponent::Event(ics_event) => {
                    let Some(start) = ics_event.get_start() else {
                        return Ok(None);
                    };
                    let all_day = is_all_day(&start);
                    let floating = matches!(
                        start,
                        DatePerhapsTime::DateTime(CalendarDateTime::Floating(_))
                    );
                    let timezone = match &start {
                        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone {
                            tzid, ..
                        }) => resolve_zone(tzid, &aliases).map(|zone| zone.name().to_string()),
                        _ => None,
                    };
                    let start = convert_time(start)?;
                    let end = match ics_event.get_end() {
                        Some(end) if is_all_day(&end) == all_day => convert_time(end)?,
                        Some(_) => return Ok(None),
                        None => start
                            .checked_add(import_event_duration(ics_event, all_day)?)
                            .ok_or_else(|| {
                                crate::Error::Storage("event end is out of range".into())
                            })?,
                    };
                    if end < start || (all_day && end == start) {
                        return Ok(None);
                    }

                    let time = if all_day {
                        EventTime::AllDay {
                            start: start.date(),
                            end: end.date(),
                        }
                    } else {
                        EventTime::Timed { start, end }
                    };

                    Ok(Some(Event {
                        id: EventId(
                            ics_event
                                .get_uid()
                                .and_then(|uid| Uuid::parse_str(uid).ok())
                                .unwrap_or_else(Uuid::new_v4),
                        ),
                        calendar: target.clone(),
                        kind: EventKind::Event,
                        summary: ics_event.get_summary().unwrap_or("Untitled").to_string(),
                        description: ics_event.get_description().map(str::to_string),
                        location: ics_event.get_location().map(str::to_string),
                        time,
                        timezone,
                        floating,
                        color: None,
                        rrule: ics_event.property_value("RRULE").map(str::to_string),
                        repeat_before: None,
                        predecessor: None,
                        split_before: None,
                        exdates: import_recurrence_dates(ics_event, "EXDATE", &aliases)?,
                        rdates: import_recurrence_dates(ics_event, "RDATE", &aliases)?,
                        recurrence_id: ics_event
                            .get_recurrence_id()
                            .map(convert_time)
                            .transpose()?,
                        series_id: None,
                        ics_uid: ics_event.get_uid().map(str::to_owned),
                        attendees: Vec::new(),
                        reminders: import_reminders(ics_event),
                        source: Source::Local,
                        external: None,
                        created_at: now,
                        updated_at: now,
                        sequence: ics_event.get_sequence().unwrap_or_default(),
                    }))
                }
                CalendarComponent::Todo(ics_todo) => {
                    let start_value = ics_todo.get_start();
                    let due_value = ics_todo.get_due();
                    let anchor = start_value.as_ref().or(due_value.as_ref());
                    let all_day = anchor.is_some_and(is_all_day);
                    let floating = matches!(
                        anchor,
                        Some(DatePerhapsTime::DateTime(CalendarDateTime::Floating(_)))
                    );
                    let timezone = match anchor {
                        Some(DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone {
                            tzid,
                            ..
                        })) => resolve_zone(tzid, &aliases).map(|zone| zone.name().to_string()),
                        _ => None,
                    };
                    let due = due_value.map(convert_time).transpose()?;
                    let start = start_value
                        .map(convert_time)
                        .transpose()?
                        .or(due)
                        .unwrap_or(now);
                    let time = if all_day {
                        let last = due.map_or(start.date(), |due| start.date().max(due.date()));
                        EventTime::AllDay {
                            start: start.date(),
                            end: last.next_day().unwrap_or(last),
                        }
                    } else {
                        let end = due
                            .filter(|due| *due > start)
                            .unwrap_or(start + time::Duration::hours(1));
                        EventTime::Timed { start, end }
                    };

                    Ok(Some(Event {
                        id: EventId(
                            ics_todo
                                .get_uid()
                                .and_then(|uid| Uuid::parse_str(uid).ok())
                                .unwrap_or_else(Uuid::new_v4),
                        ),
                        calendar: target.clone(),
                        kind: EventKind::Task(TaskDetails {
                            due,
                            completed_at: ics_todo.get_completed().map(from_utc),
                            priority: ics_todo.get_priority().map(|p| p as u8),
                        }),
                        summary: ics_todo.get_summary().unwrap_or("Untitled").to_string(),
                        description: ics_todo.get_description().map(str::to_string),
                        location: ics_todo.get_location().map(str::to_string),
                        time,
                        timezone,
                        floating,
                        color: None,
                        rrule: None,
                        repeat_before: None,
                        predecessor: None,
                        split_before: None,
                        exdates: Vec::new(),
                        rdates: Vec::new(),
                        recurrence_id: None,
                        series_id: None,
                        ics_uid: ics_todo.get_uid().map(str::to_owned),
                        attendees: Vec::new(),
                        reminders: import_reminders(ics_todo),
                        source: Source::Local,
                        external: None,
                        created_at: now,
                        updated_at: now,
                        sequence: ics_todo.get_sequence().unwrap_or_default(),
                    }))
                }
                _ => Ok(None),
            }
        })();
        match parsed {
            Ok(Some(event)) => events.push(event),
            Ok(None) => skipped += 1,
            Err(err) => {
                tracing::warn!("skipping calendar entry: {err}");
                skipped += 1;
            }
        }
    }

    for (uid, occurrence, sequence) in cancellations {
        let is_replacement = |event: &Event| {
            event.ics_uid.as_deref() == Some(uid.as_str())
                && event.recurrence_id == Some(occurrence)
        };
        if events
            .iter()
            .any(|event| is_replacement(event) && event.sequence > sequence)
        {
            continue;
        }
        if let Some(parent) = events.iter_mut().find(|event| {
            event.ics_uid.as_deref() == Some(uid.as_str())
                && event.recurrence_id.is_none()
                && event.is_recurring()
        }) {
            if !parent.exdates.contains(&occurrence) {
                parent.exdates.push(occurrence);
            }
            events.retain(|event| !is_replacement(event));
        } else {
            skipped += 1;
        }
    }

    let parents: HashMap<_, _> = events
        .iter()
        .filter(|event| event.recurrence_id.is_none())
        .filter_map(|event| Some((event.ics_uid.clone()?, event.id)))
        .collect();
    for event in &mut events {
        if event.recurrence_id.is_some() {
            event.series_id = event
                .ics_uid
                .as_ref()
                .and_then(|uid| parents.get(uid).copied());
            event.id = EventId::new();
        }
    }
    // Series must exist before their overrides are indexed.
    events.sort_by_key(|event| event.recurrence_id.is_some());

    Ok(ParsedImport { events, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::CalendarId;

    #[test]
    fn round_trips_a_timed_event() {
        let calendar = CalendarKey::Local(CalendarId::new());
        let now = OffsetDateTime::now_utc().replace_nanosecond(0).unwrap();
        let original = Event {
            id: EventId::new(),
            calendar: calendar.clone(),
            kind: EventKind::Event,
            summary: "Team sync".to_string(),
            description: Some("Weekly check-in".to_string()),
            location: Some("Room 4".to_string()),
            time: EventTime::Timed {
                start: now,
                end: now + time::Duration::hours(1),
            },
            timezone: None,
            floating: false,
            color: None,
            rrule: Some("FREQ=WEEKLY".to_string()),
            repeat_before: None,
            predecessor: None,
            split_before: None,
            exdates: vec![now + time::Duration::days(7)],
            rdates: Vec::new(),
            recurrence_id: None,
            series_id: None,
            ics_uid: None,
            attendees: Vec::new(),
            reminders: Vec::new(),
            source: Source::Local,
            external: None,
            created_at: now,
            updated_at: now,
            sequence: 0,
        };

        let ics_text = export_events(std::slice::from_ref(&original)).to_string();
        let imported = import_ics(&ics_text, calendar).expect("import should succeed");

        assert_eq!(imported.len(), 1);
        let round_tripped = &imported[0];
        assert_eq!(round_tripped.id, original.id);
        assert_eq!(round_tripped.summary, original.summary);
        assert_eq!(round_tripped.description, original.description);
        assert_eq!(round_tripped.location, original.location);
        assert_eq!(round_tripped.rrule, original.rrule);
        assert_eq!(round_tripped.exdates.len(), 1);

        match round_tripped.time {
            EventTime::Timed { start, end } => {
                assert_eq!(start.unix_timestamp(), original.start().unix_timestamp());
                assert_eq!(
                    end.unix_timestamp(),
                    match original.time {
                        EventTime::Timed { end, .. } => end.unix_timestamp(),
                        _ => unreachable!(),
                    }
                );
            }
            _ => panic!("expected a timed event"),
        }
    }

    #[test]
    fn round_trips_an_all_day_event() {
        let calendar = CalendarKey::Local(CalendarId::new());
        let now = OffsetDateTime::now_utc().replace_nanosecond(0).unwrap();
        let original = Event {
            id: EventId::new(),
            calendar: calendar.clone(),
            kind: EventKind::Event,
            summary: "Birthday".to_string(),
            description: None,
            location: None,
            time: EventTime::AllDay {
                start: now.date(),
                end: now.date().next_day().unwrap(),
            },
            timezone: None,
            floating: false,
            color: None,
            rrule: None,
            repeat_before: None,
            predecessor: None,
            split_before: None,
            exdates: Vec::new(),
            rdates: Vec::new(),
            recurrence_id: None,
            series_id: None,
            ics_uid: None,
            attendees: Vec::new(),
            reminders: Vec::new(),
            source: Source::Local,
            external: None,
            created_at: now,
            updated_at: now,
            sequence: 0,
        };

        let ics_text = export_events(std::slice::from_ref(&original)).to_string();
        let imported = import_ics(&ics_text, calendar).expect("import should succeed");

        assert_eq!(imported.len(), 1);
        match imported[0].time {
            EventTime::AllDay { start, end } => {
                assert_eq!(start, original.start().date());
                assert_eq!(
                    end,
                    match original.time {
                        EventTime::AllDay { end, .. } => end,
                        _ => unreachable!(),
                    }
                );
            }
            _ => panic!("expected an all-day event"),
        }
    }
}
