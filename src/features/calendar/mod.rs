use crate::shared::storage::LocalStorage;
use crate::Result;
use chrono::Offset;
use cosmic::{
    iced::{widget::text::Wrapping, Length},
    theme::spacing,
    widget, Element,
};
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

fn all_day_cell(date: Date, items: &[Occurrence]) -> Element<'static, crate::app::Message> {
    let mut column = widget::column(Vec::new())
        .width(Length::Fill)
        .spacing(spacing().space_xxxs);
    for item in items
        .iter()
        .filter(|item| item.time.is_none() && item.first <= date && date <= item.last)
    {
        column = column.push(
            widget::button::custom(
                widget::container(
                    widget::text::caption(item.title.clone()).wrapping(Wrapping::None),
                )
                .width(Length::Fill)
                .clip(true),
            )
            .width(Length::Fill)
            .padding([4, 6])
            .class(cosmic::style::Button::Text)
            .name(item.title.clone())
            .on_press(item.open()),
        );
    }
    column.into()
}

fn time_slot(
    date: OffsetDateTime,
    hour: u8,
    half_hour: bool,
    items: &[Occurrence],
) -> Element<'static, crate::app::Message> {
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
        return widget::button::custom(widget::text(""))
            .name(crate::fl!(
                "calendar-add-at",
                date = month::agenda_date(date.date()),
                time = format!("{hour:02}:{minute:02}")
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::style::Button::Text)
            .on_press(crate::app::Message::AddEvent(cell_time))
            .into();
    }
    events
        .fold(widget::row(Vec::new()).width(Length::Fill), |row, item| {
            row.push(
                widget::button::custom(
                    widget::container(
                        widget::text::caption(item.title.clone()).wrapping(Wrapping::None),
                    )
                    .clip(true)
                    .width(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::style::Button::Standard)
                .name(item.title.clone())
                .on_press(item.open()),
            )
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalCalendar {
    pub current_date: OffsetDateTime,
    pub selected_date: OffsetDateTime,
    pub display_timezone: Option<chrono_tz::Tz>,
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
        }
    }

    pub fn month_view<'a>(&self, storage: &LocalStorage) -> Element<'a, crate::app::Message> {
        month::month_view(*self, storage)
    }

    pub fn day_agenda<'a>(&self, storage: &LocalStorage) -> Element<'a, crate::app::Message> {
        month::agenda(self.selected_date.date(), storage, self.display_timezone)
    }

    pub fn week_view<'a>(
        &'a self,
        storage: &LocalStorage,
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
        let items = month::occurrences(storage, first, last, self.display_timezone);
        let time_grid = week::week_grid(&week_start, &items);
        let all_day = (0..7).fold(
            widget::row(Vec::new()).push(
                widget::container(widget::text::caption(crate::fl!("all-day")))
                    .width(week::TIME_COLUMN_WIDTH),
            ),
            |row, day| row.push(all_day_cell(first + Duration::days(day), &items)),
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
        &'a self,
        storage: &LocalStorage,
    ) -> impl Into<Element<'a, crate::app::Message>> {
        let date = self.selected_date.date();
        let items = month::occurrences(storage, date, date, self.display_timezone);
        let time_grid = day::single_day_time_grid(&self.selected_date, &items);
        let all_day = widget::row(Vec::new())
            .push(widget::text::caption(crate::fl!("all-day")).width(100))
            .push(all_day_cell(date, &items));

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
