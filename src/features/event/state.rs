use super::recurrence::{RecurrenceEnd, RecurrenceRule};
use crate::features::calendar_management::model::{CalendarKey, Source};
use crate::features::event::model::{Event, EventKind, EventTime, Reminder, TaskDetails};
use crate::fl;
use crate::shared::ids::EventId;
use chrono::{Offset, TimeZone};
use cosmic::widget::{calendar::CalendarModel, combo_box};
use time::{Date, OffsetDateTime, Time};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDateField {
    Start,
    End,
    Until,
}

pub fn clock_label(date: OffsetDateTime) -> String {
    format!("{:02}:{:02}", date.hour(), date.minute())
}

fn clock_presets() -> combo_box::State<String> {
    combo_box::State::new(
        (0..48)
            .map(|slot| format!("{:02}:{:02}", slot / 2, slot % 2 * 30))
            .collect(),
    )
}

fn parse_clock(input: &str) -> Option<Time> {
    let (hour, minute) = input.trim().split_once(':')?;
    if !(1..=2).contains(&hour.len())
        || minute.len() != 2
        || !hour
            .bytes()
            .chain(minute.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Time::from_hms(hour.parse().ok()?, minute.parse().ok()?, 0).ok()
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

#[derive(Debug, Clone)]
struct NewEventDefaults {
    calendar: CalendarKey,
    start: OffsetDateTime,
    end: OffsetDateTime,
    all_day: bool,
    timezone: String,
}

/// State backing the event creation and editing dialog.
#[derive(Debug, Clone)]
pub struct EventEditorState {
    /// `None` while creating a new event; `Some` while editing an existing one.
    pub event_id: Option<EventId>,
    new_event_defaults: Option<NewEventDefaults>,
    pub calendar: CalendarKey,
    pub summary: String,
    pub editing_title: bool,
    pub description: String,
    pub location: String,
    pub all_day: bool,
    pub is_task: bool,
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
    pub start_clock_input: Option<String>,
    pub end_clock_input: Option<String>,
    pub start_clock_presets: combo_box::State<String>,
    pub end_clock_presets: combo_box::State<String>,
    pub date_picker: Option<(EventDateField, CalendarModel)>,
    pub more_options: bool,
    pub adding_reminder: bool,
    pub timezone: String,
    pub floating: bool,
    pub writing: bool,
    pub recurrence: RecurrenceRule,
    /// Minutes-before-start lead times for each reminder attached to this
    /// event.
    pub reminders: Vec<i64>,
    /// Current text in the "custom reminder" minutes input, offered
    /// alongside the fixed presets since those don't cover every lead time
    /// someone might want.
    pub custom_reminder_input: String,
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
    pub fn new(
        calendar: CalendarKey,
        start: OffsetDateTime,
        all_day: bool,
        timezone: String,
    ) -> Self {
        let end = start.saturating_add(time::Duration::hours(1));
        Self {
            event_id: None,
            new_event_defaults: Some(NewEventDefaults {
                calendar: calendar.clone(),
                start,
                end,
                all_day,
                timezone: timezone.clone(),
            }),
            calendar,
            summary: String::new(),
            editing_title: true,
            description: String::new(),
            location: String::new(),
            all_day,
            is_task: false,
            start,
            end,
            start_clock_input: None,
            end_clock_input: None,
            start_clock_presets: clock_presets(),
            end_clock_presets: clock_presets(),
            date_picker: None,
            more_options: false,
            adding_reminder: false,
            timezone,
            floating: false,
            writing: false,
            recurrence: RecurrenceRule::NONE,
            reminders: Vec::new(),
            custom_reminder_input: String::new(),
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
            new_event_defaults: None,
            calendar: event.calendar.clone(),
            summary: event.summary.clone(),
            editing_title: false,
            description: event.description.clone().unwrap_or_default(),
            location: event.location.clone().unwrap_or_default(),
            all_day,
            is_task: matches!(event.kind, EventKind::Task(_)),
            start,
            end,
            start_clock_input: None,
            end_clock_input: None,
            start_clock_presets: clock_presets(),
            end_clock_presets: clock_presets(),
            date_picker: None,
            more_options: event.floating || event.timezone.is_some(),
            adding_reminder: false,
            timezone: event.timezone.clone().unwrap_or_default(),
            floating: event.floating,
            writing: false,
            recurrence: editor_recurrence(event),
            reminders: event
                .reminders
                .iter()
                .map(|reminder| reminder.minutes_before)
                .collect(),
            custom_reminder_input: String::new(),
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
        self.start_clock_input = None;
        self.end_clock_input = None;
        self.reset_clock_search(true);
        self.reset_clock_search(false);
        self.date_picker = None;
    }

    pub fn can_dismiss_with_escape(&self) -> bool {
        let Some(defaults) = &self.new_event_defaults else {
            return false;
        };
        let invalid_clock = [&self.start_clock_input, &self.end_clock_input]
            .into_iter()
            .filter_map(|input| input.as_deref())
            .any(|input| parse_clock(input).is_none());

        !self.writing
            && self.calendar == defaults.calendar
            && self.start == defaults.start
            && self.end == defaults.end
            && self.all_day == defaults.all_day
            && self.timezone == defaults.timezone
            && self.summary.is_empty()
            && self.description.is_empty()
            && self.location.is_empty()
            && !self.is_task
            && !self.floating
            && self.recurrence.freq == super::recurrence::RecurrenceFreq::None
            && self.reminders.is_empty()
            && self.custom_reminder_input.is_empty()
            && !invalid_clock
    }

    pub fn date(&self, field: EventDateField) -> Option<Date> {
        match field {
            EventDateField::Start => Some(self.start.date()),
            EventDateField::End => Some(self.end.date()),
            EventDateField::Until => match self.recurrence.end {
                RecurrenceEnd::Until(date) => Some(date),
                _ => None,
            },
        }
    }

    pub fn select_date(&mut self, field: EventDateField, date: Date) {
        match field {
            EventDateField::Start => self.move_start(self.start.replace_date(date)),
            EventDateField::End => self.end = self.end.replace_date(date),
            EventDateField::Until => self.recurrence.end = RecurrenceEnd::Until(date),
        }
    }

    pub fn input_clock(&mut self, is_start: bool, input: String) {
        if let Some(time) = parse_clock(&input) {
            if is_start {
                self.move_start(self.start.replace_time(time));
            } else {
                self.end = self.end.replace_time(time);
            }
        }
        if is_start {
            self.start_clock_input = Some(input);
        } else {
            self.end_clock_input = Some(input);
        }
    }

    pub fn reset_clock_search(&mut self, is_start: bool) {
        if is_start {
            self.start_clock_presets = clock_presets();
        } else {
            self.end_clock_presets = clock_presets();
        }
    }

    fn move_start(&mut self, start: OffsetDateTime) {
        let duration = self.end - self.start;
        let end = start.saturating_add(duration);
        if end.time() != self.end.time()
            && self
                .end_clock_input
                .as_deref()
                .and_then(parse_clock)
                .is_some()
        {
            self.end_clock_input = None;
            self.reset_clock_search(false);
        }
        self.start = start;
        self.end = end;
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
        if !self.all_day
            && [&self.start_clock_input, &self.end_clock_input]
                .into_iter()
                .filter_map(|input| input.as_deref())
                .any(|input| parse_clock(input).is_none())
        {
            return Some(fl!("event-invalid-clock"));
        }
        if self.recurrence_scope != RecurrenceScope::Occurrence
            && self.recurrence.freq != super::recurrence::RecurrenceFreq::None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::CalendarId;

    fn editor() -> EventEditorState {
        EventEditorState::new(
            CalendarKey::Local(CalendarId::new()),
            Date::from_calendar_date(2026, time::Month::November, 22)
                .unwrap()
                .with_hms(9, 0, 0)
                .unwrap()
                .assume_utc(),
            false,
            String::new(),
        )
    }

    #[test]
    fn escape_only_dismisses_an_unchanged_new_event() {
        let mut state = editor();
        state.more_options = true;
        assert!(state.can_dismiss_with_escape());

        state.summary = "Lunch".into();
        assert!(!state.can_dismiss_with_escape());
        state.summary.clear();
        assert!(state.can_dismiss_with_escape());

        state.input_clock(true, "09:".into());
        assert!(!state.can_dismiss_with_escape());
        state.input_clock(true, "09:00".into());
        assert!(state.can_dismiss_with_escape());

        state.writing = true;
        assert!(!state.can_dismiss_with_escape());
        let event = editor().build_event(None);
        assert!(!EventEditorState::from_event(&event, event.start()).can_dismiss_with_escape());
    }

    #[test]
    fn changing_the_start_date_and_time_preserves_duration() {
        let mut state = editor();
        let next = state.start.date().next_day().unwrap();
        state.select_date(EventDateField::Start, next);
        state.input_clock(false, "10:00".into());
        state.input_clock(true, "23:30".into());

        assert_eq!(state.start.date(), next);
        assert_eq!(state.end.date(), next.next_day().unwrap());
        assert_eq!(clock_label(state.end), "00:30");
        assert_eq!(state.end - state.start, time::Duration::hours(1));
        assert!(state.end_clock_input.is_none());
    }

    #[test]
    fn invalid_clock_text_blocks_saving_timed_events_only() {
        let mut state = editor();
        let original = state.start;
        state.input_clock(true, "24:00".into());
        assert_eq!(state.start, original);
        assert!(state.validation_error().is_some());

        state.all_day = true;
        assert!(state.validation_error().is_none());

        state.all_day = false;
        state.input_clock(true, "9:45".into());
        assert_eq!(clock_label(state.start), "09:45");
        assert!(state.validation_error().is_none());
    }
}
