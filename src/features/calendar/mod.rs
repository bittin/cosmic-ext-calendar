mod message;
pub use message::{CalendarAction, CalendarMessage};

use crate::app::{AppModel, Message};
use crate::Result;
use crate::shared::storage::LocalStorage;
use chrono::Offset;
use cosmic::style::Button;
use cosmic::widget::{button, menu};
use cosmic::{
    Element,
    iced::{
        Color, Length,
        widget::{mouse_area, text::Wrapping},
    },
    theme::spacing,
    widget,
};
use std::collections::HashMap;
use time::{Date, Duration, Month, OffsetDateTime, Time, UtcOffset};

pub(crate) mod month;
mod time_grid;
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
pub enum CalendarSlot {
    MonthDay(Date),
    AllDay(Date),
    Timed(Date, Time),
}

impl CalendarSlot {
    pub fn date(self) -> Date {
        match self {
            Self::MonthDay(date) | Self::AllDay(date) | Self::Timed(date, _) => date,
        }
    }

    pub fn start(self, offset: UtcOffset) -> OffsetDateTime {
        let time = match self {
            Self::MonthDay(_) => Time::from_hms(9, 0, 0).unwrap(),
            Self::AllDay(_) => Time::MIDNIGHT,
            Self::Timed(_, time) => time,
        };
        self.date().with_time(time).assume_offset(offset)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarAnchor {
    Month(Date, EventSelection),
    Agenda(Date, EventSelection),
    Grid(Date, EventSelection),
    AllDay(Date, EventSelection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EventMenuAction(CalendarAction, Option<EventSelection>, OffsetDateTime);

impl menu::Action for EventMenuAction {
    type Message = Message;

    fn message(&self) -> Message {
        Message::Calendar(CalendarMessage::EventAction(self.0, self.1, self.2))
    }
}

fn context_menu<'a>(
    content: impl Into<Element<'a, Message>>,
    items: Vec<menu::Item<EventMenuAction, String>>,
) -> Element<'a, Message> {
    widget::context_menu(content, Some(menu::items(&HashMap::new(), items))).into()
}

const EVENT_CARD_RADIUS: f32 = 5.0;

fn event_card_style(
    active: impl Fn(bool, &cosmic::Theme) -> button::Style + Clone + 'static,
    disabled: impl Fn(&cosmic::Theme) -> button::Style + 'static,
) -> Button {
    let active = move |focused, theme: &cosmic::Theme| {
        let mut style = active(focused, theme);
        style.border_radius = EVENT_CARD_RADIUS.into();
        style
    };
    Button::Custom {
        active: Box::new(active.clone()),
        hovered: Box::new(active.clone()),
        pressed: Box::new(active),
        disabled: Box::new(move |theme| {
            let mut style = disabled(theme);
            style.border_radius = EVENT_CARD_RADIUS.into();
            style
        }),
    }
}

fn colored_event_card_style(event: &Occurrence, filled: bool) -> Button {
    use button::Catalog;

    let color = event.color;
    let selected = event.selected;
    event_card_style(
        move |focused, theme| {
            let mut style = theme.active(focused, false, &Button::MenuItem);
            style.background = (filled || selected).then(|| {
                let base = theme.current_container().base.into();
                let accent = accent_or_calendar_color(theme, color);
                month::blend(base, accent, if selected { 0.48 } else { 0.22 }).into()
            });
            style
        },
        |theme| theme.disabled(&Button::MenuItem),
    )
}

fn accent_or_calendar_color(theme: &cosmic::Theme, color: Option<Color>) -> Color {
    color.unwrap_or_else(|| theme.cosmic().accent_text_color().into())
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
    let editable = event.editable;
    let mut items = Vec::new();
    if editable {
        items.extend([
            item(crate::fl!("edit-event"), CalendarAction::Edit),
            menu::Item::Divider,
            item(crate::fl!("calendar-cut"), CalendarAction::Cut),
        ]);
    }
    items.push(item(crate::fl!("calendar-copy"), CalendarAction::Copy));
    if app.calendar_can_paste() {
        items.push(item(crate::fl!("calendar-paste"), CalendarAction::Paste));
    }
    items.push(item(crate::fl!("calendar-duplicate"), CalendarAction::Duplicate));
    if editable {
        items.extend([
            menu::Item::Divider,
            item(crate::fl!("delete"), CalendarAction::Delete),
        ]);
    }
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

fn slot_target<'a>(
    app: &'a AppModel,
    content: impl Into<Element<'a, Message>>,
    slot: CalendarSlot,
    offset: UtcOffset,
) -> Element<'a, Message> {
    paste_target(
        app,
        mouse_area(content).on_release(Message::Calendar(
            CalendarMessage::CalendarEmptySlotClicked(slot),
        )),
        slot.start(offset),
    )
}

fn slot_style(selected: bool) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |theme| {
        let background = selected.then(|| {
            month::blend(
                theme.current_container().base.into(),
                theme.cosmic().accent_text_color().into(),
                0.13,
            )
            .into()
        });
        cosmic::iced::widget::container::Style {
            background,
            ..Default::default()
        }
    })
}

fn all_day_cell<'a>(
    date: Date,
    items: &[Occurrence],
    selected: Option<CalendarSlot>,
    app: &'a AppModel,
) -> Element<'a, Message> {
    let slot = CalendarSlot::AllDay(date);
    let mut column = widget::column(Vec::new())
        .width(Length::Fill)
        .spacing(spacing().space_xxxs);
    let mut all_day = items
        .iter()
        .filter(|item| item.time.is_none() && item.first <= date && date <= item.last)
        .peekable();
    if all_day.peek().is_none() {
        column = column.push(widget::space::vertical().height(20));
    }
    for item in all_day {
        let anchor = CalendarAnchor::AllDay(date, item.key());
        let mut content = widget::column(Vec::new())
            .push(widget::text::caption_heading(item.title.clone()).wrapping(Wrapping::None));
        let mut label = item.title.clone();
        if let Some(location) = &item.location {
            content =
                content.push(widget::text::caption(location.clone()).wrapping(Wrapping::None));
            label.push_str(", ");
            label.push_str(location);
        }
        column = column.push(event_target(
            app,
            widget::button::custom(widget::container(content).width(Length::Fill).clip(true))
                .width(Length::Fill)
                .padding([4, 6])
                .class(colored_event_card_style(item, true))
                .name(label)
                .on_press(item.open(anchor)),
            item,
            anchor,
        ));
    }
    let cell = widget::container(column)
        .width(Length::Fill)
        .class(slot_style(selected == Some(slot)));
    mouse_area(cell)
        .on_release(Message::Calendar(
            CalendarMessage::CalendarEmptySlotClicked(slot),
        ))
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalCalendar {
    pub current_date: OffsetDateTime,
    pub selected_date: OffsetDateTime,
    pub display_timezone: Option<chrono_tz::Tz>,
    pub selected_event: Option<EventSelection>,
    pub selected_slot: Option<CalendarSlot>,
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
            selected_slot: None,
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
            selected_slot: None,
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
            app,
            first,
            last,
            self.display_timezone,
            self.selected_event,
        );
        let time_grid = time_grid::view(
            week_start,
            7,
            week::TIME_COLUMN_WIDTH,
            time_grid::WEEK_SLOT_HEIGHT,
            &items,
            self.selected_slot,
            app,
        );
        let all_day = (0..7).fold(
            widget::row(Vec::new()).push(
                widget::container(widget::text::caption(crate::fl!("all-day")))
                    .width(week::TIME_COLUMN_WIDTH),
            ),
            |row, day| {
                row.push(all_day_cell(
                    first + Duration::days(day),
                    &items,
                    self.selected_slot,
                    app,
                ))
            },
        );

        widget::column(Vec::new())
            .push(header_row)
            .push(all_day)
            .push(
                widget::scrollable(time_grid)
                    .id(time_grid::scroll_id(true))
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
            app,
            date,
            date,
            self.display_timezone,
            self.selected_event,
        );
        let time_grid = time_grid::view(
            self.selected_date,
            1,
            100,
            time_grid::DAY_SLOT_HEIGHT,
            &items,
            self.selected_slot,
            app,
        );
        let all_day = widget::row(Vec::new())
            .push(widget::text::caption(crate::fl!("all-day")).width(100))
            .push(all_day_cell(date, &items, self.selected_slot, app))
            .align_y(cosmic::iced::Alignment::Center)
            .padding([spacing().space_xxs, 0]);

        widget::column(Vec::new())
            .push(all_day)
            .push(
                widget::scrollable(time_grid)
                    .id(time_grid::scroll_id(false))
                    .height(Length::Fill),
            )
            .padding([0, 0, spacing().space_xxs, 0])
    }

    pub(crate) fn scroll_to_now<T>(&self, week: bool) -> cosmic::iced::Task<T> {
        let now = display_time(OffsetDateTime::now_utc(), self.display_timezone);
        time_grid::scroll_to_time(now.time(), week)
    }

    pub fn set_today(&mut self) {
        self.set_date(self.current_date);
    }

    pub fn next_day(&mut self) -> Result<()> {
        let new_date = self
            .selected_date
            .checked_add(time::Duration::days(1))
            .ok_or_else(|| crate::Error::DateCalculation("Failed to calculate next day".into()))?;
        self.set_date(new_date);
        Ok(())
    }

    pub fn previous_day(&mut self) -> Result<()> {
        let new_date = self
            .selected_date
            .checked_sub(time::Duration::days(1))
            .ok_or_else(|| {
                crate::Error::DateCalculation("Failed to calculate previous day".into())
            })?;
        self.set_date(new_date);
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
        self.select_month_of_year(year, month)
    }

    pub fn select_month_of_year(&mut self, year: i32, month: Month) -> Result<()> {
        let day = self.selected_date.day().min(month.length(year));
        let date = Date::from_calendar_date(year, month, day)?;
        self.set_date(self.selected_date.replace_date(date));
        Ok(())
    }

    pub fn next_year(&mut self) -> Result<()> {
        self.select_month_of_year(self.selected_date.year() + 1, self.selected_date.month())
    }

    pub fn previous_year(&mut self) -> Result<()> {
        self.select_month_of_year(self.selected_date.year() - 1, self.selected_date.month())
    }

    pub fn today(&self) -> bool {
        self.selected_date.date() == self.current_date.date()
    }

    pub(crate) fn selected_day_events(
        &self,
        storage: &LocalStorage,
        app: &AppModel,
    ) -> Vec<EventSelection> {
        let day = self.selected_date.date();
        month::occurrences(
            storage,
            app,
            day,
            day,
            self.display_timezone,
            self.selected_event,
        )
        .iter()
        .map(month::Occurrence::key)
        .collect()
    }

    pub fn select_slot(&mut self, slot: CalendarSlot) {
        self.set_date(self.selected_date.replace_date(slot.date()));
        self.selected_slot = Some(slot);
        self.selected_event = None;
    }

    pub fn set_date(&mut self, date: OffsetDateTime) {
        self.selected_date = date;
        self.selected_slot = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_an_empty_slot_clears_event_selection_and_tracks_the_clicked_date() {
        let now = Date::from_calendar_date(2024, Month::February, 28)
            .unwrap()
            .midnight()
            .assume_utc();
        let mut calendar = LocalCalendar::new(now);
        calendar.selected_event = Some((crate::shared::ids::EventId::new(), now));
        let date = now.date().next_day().unwrap();
        let slot = CalendarSlot::Timed(date, Time::from_hms(14, 30, 0).unwrap());

        calendar.select_slot(slot);
        assert_eq!(calendar.selected_date.date(), date);
        assert_eq!(calendar.selected_slot, Some(slot));
        assert!(calendar.selected_event.is_none());

        calendar.next_year().unwrap();
        assert_eq!(
            calendar.selected_date.to_calendar_date(),
            (2025, Month::February, 28)
        );
        assert!(calendar.selected_slot.is_none());
    }
}
