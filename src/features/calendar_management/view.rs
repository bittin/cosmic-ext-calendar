use cosmic::{
    iced::{Border, Length},
    theme::{spacing, Container},
    widget::{
        self,
        color_picker::{color_button, ColorPickerUpdate},
    },
    Element,
};

use super::{
    model::calendar_color,
    state::{CalendarForm, COLOR_PICKER_WIDTH},
};
use crate::{app::Message, fl};

const PRESETS: [&str; 6] = [
    "#3584e4", "#219a86", "#4b9f43", "#db8c23", "#d95066", "#9064d5",
];

pub fn view(form: &CalendarForm) -> Element<'_, Message> {
    let mut swatches = Vec::with_capacity(PRESETS.len() + 2);
    let default_swatch = color_button(
        (!form.writing).then_some(Message::SetCalendarColor(None)),
        Some(cosmic::theme::active().cosmic().accent_text_color().into()),
        Length::Fixed(12.0),
    )
    .name(fl!("calendar-color-default"));
    swatches.push(swatch_frame(
        widget::tooltip(
            default_swatch,
            widget::text(fl!("calendar-color-default")),
            widget::tooltip::Position::Bottom,
        ),
        form.color.is_none(),
    ));
    let selected = form.color.as_deref().and_then(calendar_color);
    for hex in PRESETS {
        let color = calendar_color(hex);
        let swatch = color_button(
            (!form.writing).then(|| Message::SetCalendarColor(Some(hex.into()))),
            color,
            Length::Fixed(12.0),
        )
        .name(fl!("calendar-color-swatch", color = hex));
        swatches.push(swatch_frame(swatch, selected == color));
    }
    let custom = selected.filter(|color| {
        !PRESETS
            .iter()
            .any(|hex| calendar_color(hex) == Some(*color))
    });
    let custom_button = color_button(
        (!form.writing).then_some(Message::CalendarColorPicker(
            ColorPickerUpdate::ToggleColorPicker,
        )),
        custom,
        Length::Fixed(12.0),
    )
    .name(fl!("calendar-color-custom"));
    swatches.push(swatch_frame(custom_button, custom.is_some()));

    let content = widget::column(Vec::new())
        .spacing(spacing().space_s)
        .push(widget::text::body(fl!("calendar-name")))
        .push(
            widget::text_input("", &form.name)
                .on_input(Message::CalendarNameChanged)
                .width(Length::Fill),
        )
        .push(widget::text::body(fl!("calendar-color")))
        .push(widget::flex_row(swatches).spacing(spacing().space_xxs));

    if !form.picker.get_is_active() {
        return content.width(Length::Fill).into();
    }

    let picker = form
        .picker
        .builder(Message::CalendarColorPicker)
        .build(
            fl!("calendar-color-recent"),
            fl!("calendar-color-copy"),
            fl!("calendar-color-copied"),
        );

    widget::row(vec![
        content.width(Length::Fill).into(),
        widget::container(picker)
            .width(Length::Fixed(COLOR_PICKER_WIDTH))
            .into(),
    ])
    .width(Length::Fill)
    .align_y(cosmic::iced::Alignment::Start)
    .spacing(spacing().space_m)
    .into()
}

fn swatch_frame<'a>(
    swatch: impl Into<Element<'a, Message>>,
    selected: bool,
) -> Element<'a, Message> {
    widget::container(swatch)
        .padding(2)
        .class(Container::custom(move |theme| widget::container::Style {
            border: Border {
                color: theme.cosmic().accent_text_color().into(),
                width: if selected { 2.0 } else { 0.0 },
                radius: theme.cosmic().radius_xs().into(),
            },
            ..Default::default()
        }))
        .into()
}

/// Filled circles indicate visible calendars and rings indicate hidden calendars.
pub fn sidebar_color_icon(color: Option<cosmic::iced::Color>, visible: bool) -> widget::icon::Icon {
    let paint = color.map_or_else(
        || "#ffffff".to_owned(),
        |color| {
            let [r, g, b, _] = color.into_rgba8();
            format!("#{r:02x}{g:02x}{b:02x}")
        },
    );
    let fill = if visible { paint.as_str() } else { "none" };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="5" fill="{fill}" stroke="{paint}" stroke-width="2"/></svg>"#,
    );

    widget::icon::from_svg_bytes(svg.into_bytes())
        .symbolic(color.is_none())
        .icon()
        .size(16)
}
