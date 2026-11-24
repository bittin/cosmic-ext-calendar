use super::CalendarMessage;
use cosmic::{
    Element,
    iced::{Length, alignment::Horizontal},
    widget,
};
use time::OffsetDateTime;

pub(super) const TIME_COLUMN_WIDTH: u16 = 56;

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
        .on_press(crate::app::Message::Calendar(CalendarMessage::SelectDate(
            date,
        )))
        .height(60)
}
