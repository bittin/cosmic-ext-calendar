use super::recurrence::{RecurrenceEnd, RecurrenceRule};
use crate::features::calendar_management::model::{CalendarKey, Source};
use crate::features::event::model::{Event, EventKind, EventTime, Reminder, TaskDetails};
use crate::fl;
use crate::shared::ids::EventId;
use chrono::{Offset, TimeZone};
use cosmic::widget;
use time::OffsetDateTime;

/// The year range offered by the event editor's start-date picker.
pub const EVENT_EDITOR_YEARS: [i32; 100] = {
    let mut years = [0; 100];
    let mut i = 0;
    while i < 100 {
        years[i] = 2000 + i as i32;
        i += 1;
    }
    years
};

/// The minute options offered by the event editor's time pickers.
pub const EVENT_EDITOR_MINUTES: usize = 60;

pub fn event_editor_months() -> [time::Month; 12] {
    [
        time::Month::January,
        time::Month::February,
        time::Month::March,
        time::Month::April,
        time::Month::May,
        time::Month::June,
        time::Month::July,
        time::Month::August,
        time::Month::September,
        time::Month::October,
        time::Month::November,
        time::Month::December,
    ]
}

/// Moves `state.start` to `new_start`, keeping the event's duration
/// constant by shifting `state.end` by the same delta.
pub fn shift_event_editor_start(state: &mut EventEditorState, new_start: OffsetDateTime) {
    let duration = state.end - state.start;
    state.start = new_start;
    state.end = new_start + duration;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurrenceScope {
    Occurrence,
    Following,
    Series,
}

impl RecurrenceScope {
    pub const OPTIONS: [Self; 3] = [Self::Occurrence, Self::Following, Self::Series];

    pub fn label(self) -> String {
        match self {
            Self::Occurrence => fl!("recurrence-this-event"),
            Self::Following => fl!("recurrence-following-events"),
            Self::Series => fl!("recurrence-all-events"),
        }
    }
}

/// State backing the event creation/editing form shown in the context drawer.
#[derive(Debug, Clone)]
pub struct EventEditorState {
    /// `None` while creating a new event; `Some` while editing an existing one.
    pub event_id: Option<EventId>,
    pub calendar: CalendarKey,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub all_day: bool,
    pub is_task: bool,
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
    pub timezone: String,
    pub floating: bool,
    pub error: Option<String>,
    pub writing: bool,
    pub recurrence: RecurrenceRule,
    /// Minutes-before-start lead times for each reminder attached to this
    /// event.
    pub reminders: Vec<i64>,
    /// Current text in the "custom reminder" minutes input, offered
    /// alongside the fixed presets since those don't cover every lead time
    /// someone might want.
    pub custom_reminder_input: String,
    /// Whether the custom reminder field (a `widget::editable_input`) is
    /// currently showing its text box rather than its read-only label.
    pub custom_reminder_editing: bool,
    /// The specific occurrence that was clicked to open this editor (equal
    /// to `start` for non-recurring events and new events), used to record
    /// which single occurrence to exclude via EXDATE on "delete this
    /// occurrence".
    pub occurrence_start: OffsetDateTime,
    pub recurring_series: bool,
    pub recurrence_scope: RecurrenceScope,
}

/// The lead-time presets offered when adding a reminder in the event editor.
pub const REMINDER_PRESETS_MINUTES: [i64; 6] = [0, 5, 10, 15, 30, 60];

/// The label shown on a reminder preset's segmented-button entry.
pub fn reminder_label(minutes: i64) -> String {
    if minutes == 0 {
        fl!("reminder-at-start")
    } else if minutes % 60 == 0 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

/// Builds the reminders multi-select model: one entry per preset, plus one
/// for each of `reminders`'s lead times that isn't already covered by a
/// preset (added via the custom-minutes input). All are activated, since a
/// non-preset entry only exists in the model because it's in `reminders`;
/// deactivating one removes it from `reminders` entirely (there's no default
/// state for it to fall back to) and it disappears next rebuild.
pub fn build_reminders_model(
    reminders: &[i64],
) -> widget::segmented_button::Model<widget::segmented_button::MultiSelect> {
    let mut minutes_list: Vec<i64> = REMINDER_PRESETS_MINUTES.to_vec();
    for minutes in reminders {
        if !minutes_list.contains(minutes) {
            minutes_list.push(*minutes);
        }
    }
    minutes_list.sort_unstable();

    let mut builder =
        widget::segmented_button::Model::<widget::segmented_button::MultiSelect>::builder();
    for minutes in minutes_list {
        let is_on = reminders.contains(&minutes);
        builder = builder.insert(|b| {
            let b = b.text(reminder_label(minutes)).data(minutes);
            if is_on {
                b.activate()
            } else {
                b
            }
        });
    }
    builder.build()
}

fn editor_recurrence(event: &Event) -> RecurrenceRule {
    let mut rule = RecurrenceRule::from_rrule_string(event.rrule.as_deref());
    if let Some(until) = event
        .rrule
        .as_deref()
        .and_then(|rule| rule.split(';').find_map(|part| part.strip_prefix("UNTIL=")))
    {
        let date = if matches!(event.time, EventTime::AllDay { .. }) || event.floating {
            until.get(..8).and_then(|text| {
                let date = chrono::NaiveDate::parse_from_str(text, "%Y%m%d").ok()?;
                let noon = date.and_hms_opt(12, 0, 0)?.and_utc();
                OffsetDateTime::from_unix_timestamp(noon.timestamp())
                    .ok()
                    .map(|date| date.date())
            })
        } else if let Some(zone) = event.timezone.as_deref().and_then(|name| name.parse().ok()) {
            chrono::NaiveDateTime::parse_from_str(until, "%Y%m%dT%H%M%SZ")
                .ok()
                .and_then(|until| {
                    OffsetDateTime::from_unix_timestamp(until.and_utc().timestamp()).ok()
                })
                .map(|until| crate::features::calendar::display_time(until, Some(zone)).date())
        } else if until.ends_with("T235959Z") {
            // Older rules encoded the displayed day directly as UTC.
            None
        } else {
            chrono::NaiveDateTime::parse_from_str(until, "%Y%m%dT%H%M%SZ")
                .ok()
                .and_then(|until| {
                    OffsetDateTime::from_unix_timestamp(until.and_utc().timestamp()).ok()
                })
                .map(|until| until.to_offset(event.start().offset()).date())
        };
        if let Some(date) = date {
            rule.end = RecurrenceEnd::Until(date);
        }
    }
    rule
}

impl EventEditorState {
    pub fn new(calendar: CalendarKey, start: OffsetDateTime) -> Self {
        Self {
            event_id: None,
            calendar,
            summary: String::new(),
            description: String::new(),
            location: String::new(),
            all_day: false,
            is_task: false,
            start,
            end: start + time::Duration::hours(1),
            timezone: String::new(),
            floating: false,
            error: None,
            writing: false,
            recurrence: RecurrenceRule::NONE,
            reminders: Vec::new(),
            custom_reminder_input: String::new(),
            custom_reminder_editing: false,
            occurrence_start: start,
            recurring_series: false,
            recurrence_scope: RecurrenceScope::Series,
        }
    }

    pub fn from_event(event: &Event, occurrence_start: OffsetDateTime) -> Self {
        let (start, end, all_day) = match &event.time {
            EventTime::Timed { start, end } => (*start, *end, false),
            EventTime::AllDay { start, end } => (
                start.midnight().assume_offset(time::UtcOffset::UTC),
                end.previous_day().unwrap_or(*end).midnight().assume_utc(),
                true,
            ),
        };

        Self {
            event_id: Some(event.id),
            calendar: event.calendar.clone(),
            summary: event.summary.clone(),
            description: event.description.clone().unwrap_or_default(),
            location: event.location.clone().unwrap_or_default(),
            all_day,
            is_task: matches!(event.kind, EventKind::Task(_)),
            start,
            end,
            timezone: event.timezone.clone().unwrap_or_default(),
            floating: event.floating,
            error: None,
            writing: false,
            recurrence: editor_recurrence(event),
            reminders: event
                .reminders
                .iter()
                .map(|reminder| reminder.minutes_before)
                .collect(),
            custom_reminder_input: String::new(),
            custom_reminder_editing: false,
            occurrence_start,
            recurring_series: event.is_recurring(),
            recurrence_scope: RecurrenceScope::Series,
        }
    }

    pub fn set_recurrence_scope(&mut self, event: &Event, scope: RecurrenceScope) {
        let was_series = self.recurrence_scope == RecurrenceScope::Series;
        let is_series = scope == RecurrenceScope::Series;
        self.recurrence_scope = scope;
        if was_series == is_series {
            return;
        }

        let original = event.start();
        let occurrence = if matches!(event.time, EventTime::AllDay { .. }) {
            self.occurrence_start.to_offset(time::UtcOffset::UTC)
        } else if let Some(zone) = event.timezone.as_deref().and_then(|name| name.parse().ok()) {
            crate::features::calendar::display_time(self.occurrence_start, Some(zone))
        } else {
            self.occurrence_start.to_offset(original.offset())
        };
        let wall_time = |date: OffsetDateTime| date.replace_offset(time::UtcOffset::UTC);
        let shift = wall_time(occurrence) - wall_time(original);
        let shift = if is_series { -shift } else { shift };
        self.start = self.start.saturating_add(shift);
        self.end = self.end.saturating_add(shift);
    }

    fn zoned_time(&self, value: OffsetDateTime) -> Option<OffsetDateTime> {
        if self.floating {
            return Some(value.replace_offset(time::UtcOffset::UTC));
        }
        let name = self.timezone.trim();
        if name.is_empty() {
            return Some(value);
        }

        let zone = name.parse::<chrono_tz::Tz>().ok()?;
        let wall_time = chrono::DateTime::from_timestamp(
            value.unix_timestamp() + i64::from(value.offset().whole_seconds()),
            value.nanosecond(),
        )?
        .naive_utc();
        let candidates = zone.from_local_datetime(&wall_time);
        let first = candidates.earliest()?;
        // Keep the original offset during a repeated hour when possible.
        let zoned = candidates
            .latest()
            .filter(|date| date.offset().fix().local_minus_utc() == value.offset().whole_seconds())
            .unwrap_or(first);
        let offset =
            time::UtcOffset::from_whole_seconds(zoned.offset().fix().local_minus_utc()).ok()?;
        let nanos = i128::from(zoned.timestamp()) * 1_000_000_000
            + i128::from(zoned.timestamp_subsec_nanos());
        Some(
            OffsetDateTime::from_unix_timestamp_nanos(nanos)
                .ok()?
                .to_offset(offset),
        )
    }

    pub fn validation_error(&self) -> Option<String> {
        if self.recurrence_scope != RecurrenceScope::Occurrence
            && matches!(self.recurrence.end, RecurrenceEnd::Until(date) if date < self.start.date())
        {
            return Some(fl!("invalid-recurrence-end"));
        }
        if self.all_day {
            return (self.end.date() < self.start.date() || self.end.date().next_day().is_none())
                .then(|| fl!("invalid-all-day-range"));
        }
        if !self.floating
            && !self.timezone.trim().is_empty()
            && self.timezone.trim().parse::<chrono_tz::Tz>().is_err()
        {
            return Some(fl!("invalid-event-timezone"));
        }
        let (Some(start), Some(end)) = (self.zoned_time(self.start), self.zoned_time(self.end))
        else {
            return Some(fl!("nonexistent-event-time"));
        };
        (end < start).then(|| fl!("invalid-date-range"))
    }

    fn rrule_string(&self) -> Option<String> {
        let rule = self.recurrence.to_rrule_string()?;
        let RecurrenceEnd::Until(date) = self.recurrence.end else {
            return Some(rule);
        };
        let (base, _) = rule.rsplit_once(";UNTIL=")?;
        let civil = format!(
            "{:04}{:02}{:02}",
            date.year(),
            date.month() as u8,
            date.day()
        );
        if self.all_day {
            return Some(format!("{base};UNTIL={civil}"));
        }
        if self.floating {
            return Some(format!("{base};UNTIL={civil}T235959"));
        }
        let last_time = date
            .with_hms(23, 59, 59)
            .ok()?
            .assume_offset(self.start.offset());
        let utc = self.zoned_time(last_time)?.to_offset(time::UtcOffset::UTC);
        Some(format!(
            "{base};UNTIL={:04}{:02}{:02}T{:02}{:02}{:02}Z",
            utc.year(),
            utc.month() as u8,
            utc.day(),
            utc.hour(),
            utc.minute(),
            utc.second()
        ))
    }

    /// Builds the [`Event`] to persist from the current form state, reusing
    /// identity/timestamps/exceptions from `existing` when editing.
    pub fn build_event(&self, existing: Option<&Event>) -> Event {
        let now = OffsetDateTime::now_utc();

        let time = if self.all_day {
            EventTime::AllDay {
                start: self.start.date(),
                end: self.end.date().next_day().unwrap_or(self.end.date()),
            }
        } else {
            EventTime::Timed {
                start: self.zoned_time(self.start).unwrap_or(self.start),
                end: self.zoned_time(self.end).unwrap_or(self.end),
            }
        };

        let kind = if self.is_task {
            match existing.map(|event| &event.kind) {
                Some(EventKind::Task(details)) => EventKind::Task(details.clone()),
                _ => EventKind::Task(TaskDetails {
                    due: None,
                    completed_at: None,
                    priority: None,
                }),
            }
        } else {
            EventKind::Event
        };

        Event {
            id: self.event_id.unwrap_or_default(),
            calendar: self.calendar.clone(),
            kind,
            summary: self.summary.clone(),
            description: (!self.description.is_empty()).then(|| self.description.clone()),
            location: (!self.location.is_empty()).then(|| self.location.clone()),
            time,
            timezone: (!self.all_day && !self.floating && !self.timezone.trim().is_empty())
                .then(|| self.timezone.trim().to_string()),
            floating: !self.all_day && self.floating,
            color: existing.and_then(|event| event.color.clone()),
            rrule: match existing {
                Some(event)
                    if editor_recurrence(event) == self.recurrence
                        && event.floating == (!self.all_day && self.floating)
                        && event.timezone.as_deref().unwrap_or_default()
                            == self.timezone.trim()
                        && matches!(event.time, EventTime::AllDay { .. }) == self.all_day =>
                {
                    event.rrule.clone()
                }
                _ => self.rrule_string(),
            },
            repeat_before: existing.and_then(|event| event.repeat_before),
            predecessor: existing.and_then(|event| event.predecessor),
            split_before: existing.and_then(|event| event.split_before),
            exdates: existing.map_or(Vec::new(), |event| event.exdates.clone()),
            rdates: existing.map_or(Vec::new(), |event| event.rdates.clone()),
            recurrence_id: existing.and_then(|event| event.recurrence_id),
            series_id: existing.and_then(|event| event.series_id),
            ics_uid: existing.and_then(|event| event.ics_uid.clone()),
            attendees: existing.map_or(Vec::new(), |event| event.attendees.clone()),
            reminders: self
                .reminders
                .iter()
                .map(|minutes_before| Reminder {
                    minutes_before: *minutes_before,
                })
                .collect(),
            source: Source::Local,
            external: None,
            created_at: existing.map_or(now, |event| event.created_at),
            updated_at: now,
            sequence: existing.map_or(0, |event| event.sequence + 1),
        }
    }
}
