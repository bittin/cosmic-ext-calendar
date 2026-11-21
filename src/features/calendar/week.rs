use cosmic::{
    iced::{alignment::Horizontal, Length},
    theme::spacing,
    widget, Element,
};
use time::OffsetDateTime;

use super::{time_slot, Occurrence};

pub(super) const TIME_COLUMN_WIDTH: u16 = 56;

pub fn week_grid<'a>(
    week_start: &OffsetDateTime,
    items: &[Occurrence],
) -> impl Into<Element<'a, crate::app::Message>> {
    let mut main_column = widget::column(Vec::new()).spacing(0);

    for hour in 0..24 {
        main_column = main_column.push(widget::divider::horizontal::default());

        let row = hour_row(hour, false, week_start, items);
        main_column = main_column.push(row);

        main_column = main_column.push(widget::divider::horizontal::light());

        let half_hour_row = hour_row(hour, true, week_start, items);
        main_column = main_column.push(half_hour_row);
    }

    main_column
}

fn hour_row<'a>(
    hour: u8,
    is_half_hour: bool,
    week_start: &OffsetDateTime,
    items: &[Occurrence],
) -> impl Into<Element<'a, crate::app::Message>> {
    let mut row = widget::row(Vec::new()).height(60);

    let time_label = if is_half_hour {
        String::new()
    } else {
        format!("{:02}:00", hour)
    };

    let time_container =
        widget::container(widget::text::body(time_label).align_x(Horizontal::Right))
            .width(TIME_COLUMN_WIDTH)
            .height(30)
            .align_top(Length::Fill)
            .align_x(Horizontal::Center)
            .padding([spacing().space_xxxs, 0, 0, 0]);

    row = row.push(time_container);
    row = row.push(widget::divider::vertical::default());

    for day in 0..7 {
        let day_date = week_start.checked_add(time::Duration::days(day)).unwrap();

        let cell = time_slot(day_date, hour, is_half_hour, items);
        row = row.push(cell);
        row = row.push(widget::divider::vertical::default());
    }

    row
}

pub fn day_header<'a>(
    day_name: String,
    day_number: u8,
    is_today: bool,
    date: OffsetDateTime,
) -> impl Into<Element<'a, crate::app::Message>> {
    let header = widget::column(Vec::new())
        .push(
            widget::text::body(day_name)
                .font(if is_today {
                    cosmic::font::bold()
                } else {
                    cosmic::font::default()
                })
                .align_x(Horizontal::Center),
        )
        .push(widget::text::title4(day_number.to_string()).align_x(Horizontal::Center))
        .width(Length::Fill)
        .align_x(Horizontal::Center);

    widget::button::custom(header)
        .width(Length::Fill)
        .class(cosmic::style::Button::Text)
        .on_press(crate::app::Message::SelectDate(date))
        .height(60)
}
