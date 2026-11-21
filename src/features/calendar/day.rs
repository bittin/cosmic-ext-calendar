use crate::app::AppModel;
use cosmic::{
    iced::{alignment::Horizontal, Length},
    theme::spacing,
    widget, Element,
};
use time::OffsetDateTime;

use super::{time_slot, Occurrence};

pub fn single_day_time_grid<'a>(
    selected_date: &OffsetDateTime,
    items: &[Occurrence],
    app: &'a AppModel,
) -> impl Into<Element<'a, crate::app::Message>> {
    let mut main_column = widget::column(Vec::new()).spacing(0);

    for hour in 0..24 {
        main_column = main_column.push(widget::divider::horizontal::default());

        let hour_row = single_day_hour_row(hour, false, selected_date, items, app);
        main_column = main_column.push(hour_row);

        main_column = main_column.push(widget::divider::horizontal::light());

        let half_hour_row = single_day_hour_row(hour, true, selected_date, items, app);
        main_column = main_column.push(half_hour_row);
    }

    main_column
}

fn single_day_hour_row<'a>(
    hour: u8,
    is_half_hour: bool,
    selected_date: &OffsetDateTime,
    items: &[Occurrence],
    app: &'a AppModel,
) -> impl Into<Element<'a, crate::app::Message>> {
    let mut row = widget::row(Vec::new());

    let time_label = if is_half_hour {
        format!("{:02}:30", hour)
    } else {
        format!("{:02}:00", hour)
    };

    let time_container =
        widget::container(widget::text::body(time_label).align_x(Horizontal::Right))
            .width(100)
            .height(40)
            .center_y(Length::Fill)
            .padding([0, spacing().space_s, 0, 0]);

    row = row.push(time_container);
    row = row.push(widget::divider::vertical::default());

    let time_cell = time_slot(*selected_date, hour, is_half_hour, items, app);
    row = row.push(time_cell);

    row.height(80)
}
