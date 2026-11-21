use crate::Result;
use crate::app::{AppModel, CalendarAction, Message};
use crate::shared::storage::LocalStorage;
use chrono::Offset;
use cosmic::style::Button;
use cosmic::widget::{button, menu};
use cosmic::{
    Element,
    iced::{Length, widget::text::Wrapping},
    theme::spacing,
    widget,
};
use std::collections::HashMap;
use time::{Date, Duration, Month, OffsetDateTime, Time, UtcOffset};

mod day;
pub(crate) mod month;
mod week;

pub(crate) fn display_time(date: OffsetDateTime, zone: Option<chrono_tz::Tz>) -> OffsetDateTime {
    let Some(utc) = chrono::DateTime::from_timestamp(date.unix_timestamp(), date.nanosecond())
    else {
        return date;
    };
    let offset = match zone {
        Some(zone) => utc.with_timezone(&zone).offset().fix(),
        None => *utc.with_timezone(&chrono::Local).offset(),
    };
    UtcOffset::from_whole_seconds(offset.local_minus_utc())
        .ok()
        .and_then(|offset| date.checked_to_offset(offset))
        .unwrap_or(date)
}

use month::Occurrence;

pub(crate) type EventSelection = (crate::shared::ids::EventId, OffsetDateTime);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarAnchor {
    Month(Date, EventSelection),
    Agenda(Date, EventSelection),
    Grid(OffsetDateTime, EventSelection),
    AllDay(Date, EventSelection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EventMenuAction(CalendarAction, Option<EventSelection>, OffsetDateTime);

impl menu::Action for EventMenuAction {
    type Message = Message;

    fn message(&self) -> Message {
        Message::CalendarAction(self.0, self.1, self.2)
    }
}

fn context_menu<'a>(
    content: impl Into<Element<'a, Message>>,
    items: Vec<menu::Item<EventMenuAction, String>>,
) -> Element<'a, Message> {
    widget::context_menu(content, Some(menu::items(&HashMap::new(), items))).into()
}

fn event_card_style(
    active: impl Fn(bool, &cosmic::Theme) -> button::Style + Clone + 'static,
    disabled: impl Fn(&cosmic::Theme) -> button::Style + 'static,
) -> Button {
    Button::Custom {
        active: Box::new(active.clone()),
        hovered: Box::new(active.clone()),
        pressed: Box::new(active),
        disabled: Box::new(disabled),
    }
}

fn event_button_style(selected: bool, normal: Button, selection: Button) -> Button {
    use button::Catalog;

    let style = if selected { selection } else { normal };
    let style = std::rc::Rc::new(style);
    let active = style.clone();
    event_card_style(
        move |focused, theme| theme.active(focused, false, &active),
        move |theme| theme.disabled(&style),
    )
}

fn event_target<'a>(
    app: &'a AppModel,
    button: impl Into<Element<'a, Message>>,
    event: &Occurrence,
    anchor: CalendarAnchor,
) -> Element<'a, Message> {
    let item = |label, action| {
        menu::Item::Button(
            label,
            None,
            EventMenuAction(action, Some(event.key()), event.display_start),
        )
    };
    let mut items = vec![
        item(crate::fl!("edit-event"), CalendarAction::Edit),
        menu::Item::Divider,
        item(crate::fl!("calendar-cut"), CalendarAction::Cut),
        item(crate::fl!("calendar-copy"), CalendarAction::Copy),
    ];
    if app.calendar_can_paste() {
        items.push(item(crate::fl!("calendar-paste"), CalendarAction::Paste));
    }
    items.extend([
        item(crate::fl!("calendar-duplicate"), CalendarAction::Duplicate),
        menu::Item::Divider,
        item(crate::fl!("delete"), CalendarAction::Delete),
    ]);
    app.attach_calendar_popup(context_menu(button, items), Some(anchor))
}

fn paste_target<'a>(
    app: &'a AppModel,
    button: impl Into<Element<'a, Message>>,
    date: OffsetDateTime,
) -> Element<'a, Message> {
    if !app.calendar_can_paste() {
        return button.into();
    }
    let items = vec![menu::Item::Button(
        crate::fl!("calendar-paste"),
        None,
        EventMenuAction(CalendarAction::Paste, None, date),
    )];
    context_menu(button, items)
}

fn all_day_cell<'a>(date: Date, items: &[Occurrence], app: &'a AppModel) -> Element<'a, Message> {
    let mut column = widget::column(Vec::new())
        .width(Length::Fill)
        .spacing(spacing().space_xxxs);
    for item in items
        .iter()
        .filter(|item| item.time.is_none() && item.first <= date && date <= item.last)
    {
        let anchor = CalendarAnchor::AllDay(date, item.key());
        column = column.push(event_target(
            app,
            widget::button::custom(
                widget::container(
                    widget::text::caption(item.title.clone()).wrapping(Wrapping::None),
                )
                .width(Length::Fill)
                .clip(true),
            )
            .width(Length::Fill)
            .padding([4, 6])
            .class(event_button_style(
                item.selected,
                Button::Text,
                Button::Standard,
            ))
            .name(item.title.clone())
            .on_press(item.open(anchor)),
            item,
            anchor,
        ));
    }
    column.into()
}

fn time_slot<'a>(
    date: OffsetDateTime,
    hour: u8,
    half_hour: bool,
    items: &[Occurrence],
    app: &'a AppModel,
) -> Element<'a, Message> {
    let minute = if half_hour { 30 } else { 0 };
    let cell_time = date.replace_time(Time::from_hms(hour, minute, 0).unwrap());
    let start = cell_time.time();
    let end = cell_time
        .checked_add(Duration::minutes(30))
        .filter(|end| end.date() == date.date())
        .map(|end| end.time());
    let mut events = items
        .iter()
        .filter(|item| {
            item.time.is_some()
                && item.first <= date.date()
                && date.date() <= item.last
                && (item.first != date.date()
                    || end.is_none_or(|end| item.display_start.time() < end))
                && (item.last != date.date() || item.display_end.time() >= start)
        })
        .peekable();
    if events.peek().is_none() {
        return paste_target(
            app,
            widget::button::custom(widget::text(""))
                .name(crate::fl!(
                    "calendar-add-at",
                    date = month::agenda_date(date.date()),
                    time = format!("{hour:02}:{minute:02}")
                ))
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::style::Button::Text)
                .on_press(crate::app::Message::AddEvent(cell_time)),
            cell_time,
        );
    }
    events
        .fold(widget::row(Vec::new()).width(Length::Fill), |row, item| {
            let anchor = CalendarAnchor::Grid(cell_time, item.key());
            row.push(event_target(
                app,
                widget::button::custom(
                    widget::container(
                        widget::text::caption(item.title.clone()).wrapping(Wrapping::None),
                    )
                    .clip(true)
                    .width(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .class(event_button_style(
                    item.selected,
                    Button::Standard,
                    Button::Suggested,
                ))
                .name(item.title.clone())
                .on_press(item.open(anchor)),
                item,
                anchor,
            ))
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalCalendar {
    pub current_date: OffsetDateTime,
    pub selected_date: OffsetDateTime,
    pub display_timezone: Option<chrono_tz::Tz>,
    pub selected_event: Option<EventSelection>,
}

impl Default for LocalCalendar {
    fn default() -> Self {
        let current_date = match OffsetDateTime::now_local() {
            Ok(date) => date,
            Err(err) => {
                tracing::error!("Failed to get local time: {}", err);
                panic!("Failed to get local time")
            }
        };
        Self {
            current_date,
            selected_date: current_date,
            display_timezone: None,
            selected_event: None,
        }
    }
}

impl LocalCalendar {
    #[allow(unused)]
    pub fn new(current_date: OffsetDateTime) -> Self {
        Self {
            current_date,
            selected_date: current_date,
            display_timezone: None,
            selected_event: None,
        }
    }

    pub fn month_view<'a>(
        &self,
        storage: &LocalStorage,
        app: &'a AppModel,
    ) -> Element<'a, crate::app::Message> {
        month::month_view(*self, storage, app)
    }

    pub fn day_agenda<'a>(
        &self,
        storage: &LocalStorage,
        app: &'a AppModel,
    ) -> Element<'a, crate::app::Message> {
        month::agenda(
            self.selected_date.date(),
            storage,
            self.display_timezone,
            self.selected_event,
            app,
        )
    }

    pub fn week_view<'a>(
        &self,
        storage: &LocalStorage,
        app: &'a AppModel,
    ) -> impl Into<Element<'a, crate::app::Message>> {
        let selected_date = self.selected_date;
        let days_since_monday = i64::from(selected_date.weekday().number_days_from_monday());
        let week_start = selected_date
            .replace_time(Time::MIDNIGHT)
            .saturating_sub(Duration::days(days_since_monday));

        let mut header_row =
            widget::row(Vec::new()).push(widget::Space::new().width(week::TIME_COLUMN_WIDTH));
        for day in 0..7 {
            let date = week_start + Duration::days(day);
            header_row = header_row.push(week::day_header(
                month::weekday_name(date.weekday()),
                date.day(),
                date.date() == self.current_date.date(),
                date,
            ));
        }

        let first = week_start.date();
        let last = first + Duration::days(6);
        let items = month::occurrences(
            storage,
            first,
            last,
            self.display_timezone,
            self.selected_event,
        );
        let time_grid = week::week_grid(&week_start, &items, app);
        let all_day = (0..7).fold(
            widget::row(Vec::new()).push(
                widget::container(widget::text::caption(crate::fl!("all-day")))
                    .width(week::TIME_COLUMN_WIDTH),
            ),
            |row, day| row.push(all_day_cell(first + Duration::days(day), &items, app)),
        );

        widget::column(Vec::new())
            .push(header_row)
            .push(all_day)
            .push(
                widget::scrollable(time_grid)
                    .height(Length::Fill)
                    .width(Length::Fill),
            )
            .padding([0, 0, spacing().space_xxs, 0])
    }

    pub fn day_view<'a>(
        &self,
        storage: &LocalStorage,
        app: &'a AppModel,
    ) -> impl Into<Element<'a, crate::app::Message>> {
        let date = self.selected_date.date();
        let items = month::occurrences(
            storage,
            date,
            date,
            self.display_timezone,
            self.selected_event,
        );
        let time_grid = day::single_day_time_grid(&self.selected_date, &items, app);
        let all_day = widget::row(Vec::new())
            .push(widget::text::caption(crate::fl!("all-day")).width(100))
            .push(all_day_cell(date, &items, app));

        widget::column(Vec::new())
            .push(all_day)
            .push(widget::scrollable(time_grid).height(Length::Fill))
            .padding([0, 0, spacing().space_xxs, 0])
    }

    pub fn set_today(&mut self) {
        self.selected_date = self.current_date;
    }

    pub fn next_day(&mut self) -> Result<()> {
        let new_date = self
            .selected_date
            .checked_add(time::Duration::days(1))
            .ok_or_else(|| crate::Error::DateCalculation("Failed to calculate next day".into()))?;
        self.selected_date = new_date;
        Ok(())
    }

    pub fn previous_day(&mut self) -> Result<()> {
        let new_date = self
            .selected_date
            .checked_sub(time::Duration::days(1))
            .ok_or_else(|| {
                crate::Error::DateCalculation("Failed to calculate previous day".into())
            })?;
        self.selected_date = new_date;
        Ok(())
    }

    pub fn next_month(&mut self) -> Result<()> {
        self.shift_month(1)
    }

    pub fn previous_month(&mut self) -> Result<()> {
        self.shift_month(-1)
    }

    fn shift_month(&mut self, delta: i32) -> Result<()> {
        let date = self.selected_date.date();
        let month_index = i32::from(u8::from(date.month())) - 1 + delta;
        let year = date.year() + month_index.div_euclid(12);
        let month = Month::try_from((month_index.rem_euclid(12) + 1) as u8)?;
        let day = date.day().min(month.length(year));
        self.selected_date = self
            .selected_date
            .replace_date(time::Date::from_calendar_date(year, month, day)?);
        Ok(())
    }

    pub fn next_year(&mut self) -> Result<()> {
        let next_year = self.selected_date.year();
        let new_date = self.selected_date.replace_year(next_year + 1)?;
        self.selected_date = new_date;
        Ok(())
    }

    pub fn previous_year(&mut self) -> Result<()> {
        let next_year = self.selected_date.year();
        let new_date = self.selected_date.replace_year(next_year - 1)?;
        self.selected_date = new_date;
        Ok(())
    }

    pub fn today(&self) -> bool {
        self.selected_date.date() == self.current_date.date()
    }

    pub fn set_date(&mut self, date: OffsetDateTime) {
        self.selected_date = date;
    }

    pub fn months(&self) -> [Month; 12] {
        [
            Month::January,
            Month::February,
            Month::March,
            Month::April,
            Month::May,
            Month::June,
            Month::July,
            Month::August,
            Month::September,
            Month::October,
            Month::November,
            Month::December,
        ]
    }

    pub fn years(&self) -> Vec<i32> {
        let mut years = vec![];
        for year in 2000..=2099 {
            years.push(year);
        }
        years
    }

    pub fn days(&self) -> Vec<u8> {
        let mut days = vec![];
        let days_in_month = self.selected_date.month().length(self.selected_date.year());
        for day in 1..=days_in_month {
            days.push(day);
        }
        days
    }
}
