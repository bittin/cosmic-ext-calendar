use std::borrow::Cow;

use cosmic::{Element, iced::Length, theme::spacing, widget};

use crate::app::Message;
use crate::features::calendar::month::{agenda_date, weekday_name};
use crate::features::calendar_management::{
    model::{CalendarKey, LocalCalendarMeta, calendar_color},
    view::calendar_color_icon,
};
use crate::features::event::location::LocationSuggestions;
use crate::features::event::recurrence::{RecurrenceEnd, RecurrenceFreq};
use crate::features::event::state::{
    EventDateField, EventEditorState, REMINDER_PRESETS_MINUTES, RecurrenceScope, clock_label,
    reminder_label,
};
use crate::fl;

pub fn title_id() -> widget::Id {
    widget::Id::new("event-title")
}

fn date_control(
    state: &EventEditorState,
    field: EventDateField,
    date: time::Date,
) -> Element<'_, Message> {
    let button = widget::button::standard(fl!(
        "event-date-label",
        date = agenda_date(date),
        year = date.year().to_string()
    ))
    .leading_icon(widget::icon::from_name("office-calendar-symbolic"))
    .on_press(Message::OpenEventDatePicker(field));
    let Some((_, model)) = state
        .date_picker
        .as_ref()
        .filter(|(open, _)| *open == field)
    else {
        return button.into();
    };

    let picker = widget::calendar(
        model,
        Message::SelectEventDate,
        || Message::BrowseEventDatePicker(-1),
        || Message::BrowseEventDatePicker(1),
        jiff::civil::Weekday::Monday,
    );
    widget::popover::popover(button)
        .position(widget::popover::Position::Bottom)
        .popup(widget::container(picker).class(cosmic::theme::Container::Dialog(false)))
        .on_close(Message::CloseEventDatePicker)
        .into()
}

fn schedule_row(state: &EventEditorState, is_start: bool) -> Element<'_, Message> {
    let spacing = spacing();
    let (field, endpoint, input, presets, label) = if is_start {
        (
            EventDateField::Start,
            state.start,
            &state.start_clock_input,
            &state.start_clock_presets,
            fl!("starts"),
        )
    } else {
        (
            EventDateField::End,
            state.end,
            &state.end_clock_input,
            &state.end_clock_presets,
            fl!("ends"),
        )
    };
    let mut controls = vec![date_control(state, field, endpoint.date())];
    if !state.all_day {
        let value = input.clone().unwrap_or_else(|| clock_label(endpoint));
        let combo_box = widget::combo_box(presets, &value, Some(&value), move |value| {
            Message::EventClockChanged(is_start, value)
        })
        .on_input(move |value| Message::EventClockChanged(is_start, value))
        .on_close(Message::CloseEventClockSearch(is_start))
        .menu_height(240)
        .padding([spacing.space_xxs, spacing.space_xs])
        .font(cosmic::font::default())
        .size(14)
        .width(Length::Fill);
        // The combo box menu inherits the window's transparency unless scoped here.
        let mut menu_theme = cosmic::theme::active();
        menu_theme.transparent = false;
        // The native input has no intrinsic text width. Size it using the same font and padding.
        let measure = if value.chars().count() < 5 {
            "00:00".to_owned()
        } else {
            value
        };
        let measurement = widget::container(
            cosmic::iced::widget::text(measure)
                .font(cosmic::font::default())
                .size(14)
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .class(cosmic::iced::Color::TRANSPARENT),
        )
        .padding([spacing.space_xxs, spacing.space_xs]);
        controls.push(
            cosmic::iced::widget::stack(vec![
                measurement.into(),
                cosmic::iced::widget::themer(Some(menu_theme), combo_box).into(),
            ])
            .width(Length::Shrink)
            .height(Length::Shrink)
            .into(),
        );
    }
    widget::settings::item(
        label,
        widget::flex_row(controls)
            .align_items(cosmic::iced::Alignment::Center)
            .spacing(spacing.space_xs),
    )
    .into()
}

fn text_input_editor_style(
    theme: &cosmic::Theme,
    status: widget::text_editor::Status,
) -> widget::text_editor::Style {
    use widget::text_editor::Status;
    use widget::text_input::{Style, StyleSheet};

    let style = Style::Default;
    let appearance = match status {
        Status::Active => StyleSheet::active(theme, &style),
        Status::Hovered => StyleSheet::hovered(theme, &style),
        Status::Focused { .. } => StyleSheet::focused(theme, &style),
        Status::Disabled => StyleSheet::disabled(theme, &style),
    };
    widget::text_editor::Style {
        background: appearance.background,
        border: cosmic::iced::Border {
            color: appearance.border_color,
            width: appearance.border_width,
            radius: appearance.border_radius,
        },
        placeholder: appearance.placeholder_color,
        value: appearance
            .text_color
            .unwrap_or_else(|| theme.current_container().on.into()),
        selection: appearance.selected_fill,
    }
}

/// The "Repeat" settings section. Only the frequency row is always shown;
/// interval/weekday/end-condition rows are added as the form deepens, so a
/// one-off event never has to look at recurrence controls at all.
fn recurrence_section<'a>(state: &'a EventEditorState) -> widget::settings::Section<'a, Message> {
    let recurrence_labels: Vec<String> = RecurrenceFreq::ALL
        .iter()
        .map(|freq| freq.label().to_string())
        .collect();
    let recurrence_selected = RecurrenceFreq::ALL
        .iter()
        .position(|freq| *freq == state.recurrence.freq);

    let mut section = widget::settings::section().add(widget::settings::item(
        fl!("recurrence"),
        widget::dropdown(
            recurrence_labels,
            recurrence_selected,
            Message::EventEditorRecurrenceChanged,
        ),
    ));

    if state.recurrence.freq == RecurrenceFreq::None {
        return section;
    }

    let interval_labels: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
    let interval_selected = Some((state.recurrence.interval.max(1) - 1) as usize);
    section = section.add(widget::settings::item(
        fl!("every"),
        widget::dropdown(
            interval_labels,
            interval_selected,
            Message::EventEditorRecurrenceIntervalChanged,
        ),
    ));

    if state.recurrence.freq == RecurrenceFreq::Weekly {
        let mut weekdays = widget::row(Vec::new()).spacing(spacing().space_xxs);
        for (idx, day) in [
            time::Weekday::Monday,
            time::Weekday::Tuesday,
            time::Weekday::Wednesday,
            time::Weekday::Thursday,
            time::Weekday::Friday,
            time::Weekday::Saturday,
            time::Weekday::Sunday,
        ]
        .into_iter()
        .enumerate()
        {
            let on = state.recurrence.by_weekday[idx];
            weekdays = weekdays.push(
                widget::button::custom(widget::text::body(weekday_name(day)))
                    .class(if on {
                        cosmic::style::Button::Suggested
                    } else {
                        cosmic::style::Button::Standard
                    })
                    .on_press(Message::EventEditorRecurrenceWeekdayToggled(idx, !on)),
            );
        }
        section = section.add(widget::settings::item(fl!("on-days"), weekdays));
    }

    let end_labels: Vec<String> = vec![fl!("end-never"), fl!("end-until"), fl!("end-count")];
    let end_selected = Some(match state.recurrence.end {
        RecurrenceEnd::Never => 0,
        RecurrenceEnd::Until(_) => 1,
        RecurrenceEnd::Count(_) => 2,
    });
    section = section.add(widget::settings::item(
        fl!("recurrence-ends"),
        widget::dropdown(
            end_labels,
            end_selected,
            Message::EventEditorRecurrenceEndChanged,
        ),
    ));

    match state.recurrence.end {
        RecurrenceEnd::Until(until) => {
            section = section.add(widget::settings::item(
                fl!("end-until"),
                date_control(state, EventDateField::Until, until),
            ));
        }
        RecurrenceEnd::Count(count) => {
            let count_labels: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
            let count_selected = Some((count.max(1) - 1) as usize);
            let count_row = widget::row(Vec::new())
                .spacing(spacing().space_xs)
                .align_y(cosmic::iced::alignment::Vertical::Center)
                .push(widget::dropdown(
                    count_labels,
                    count_selected,
                    Message::EventEditorRecurrenceCountChanged,
                ))
                .push(widget::text::body(fl!("occurrences")));
            section = section.add(widget::settings::item(fl!("end-count"), count_row));
        }
        RecurrenceEnd::Never => {}
    }

    section
}

pub fn view<'a>(
    state: &'a EventEditorState,
    calendars: &[&LocalCalendarMeta],
    notes: &'a widget::text_editor::Content,
    locations: &'a LocationSuggestions,
) -> Element<'a, Message> {
    if state.writing {
        return widget::dialog::dialog()
            .title(fl!("event-writing"))
            .width(Length::Fixed(640.0))
            .into();
    }

    let calendar_labels: Vec<String> = calendars.iter().map(|meta| meta.name.clone()).collect();
    let calendar_selected = calendars
        .iter()
        .position(|meta| matches!(&state.calendar, CalendarKey::Local(id) if *id == meta.id));

    let calendar_icons = calendars
        .iter()
        .map(|meta| calendar_color_icon(meta.color.as_deref().and_then(calendar_color), true))
        .collect();
    let title = widget::settings::section().add(
        widget::editable_input(
            fl!("event-title"),
            &state.summary,
            state.editing_title,
            Message::ToggleEventTitleEditing,
        )
        .id(title_id())
        .on_input(Message::EventEditorSummaryChanged)
        .width(Length::Fill),
    );
    let mut when = widget::settings::section()
        .add(
            widget::settings::item::builder(fl!("all-day"))
                .toggler(state.all_day, Message::EventEditorAllDayToggled),
        )
        .add(schedule_row(state, true))
        .add(schedule_row(state, false))
        .add(widget::settings::item(
            fl!("event-calendar"),
            widget::dropdown(
                calendar_labels,
                calendar_selected,
                Message::EventEditorCalendarChanged,
            )
            .icons(Cow::Owned(calendar_icons)),
        ));
    let validation_error = state.validation_error();
    if let Some(error) = &validation_error {
        when = when.add(widget::text::body(error.clone()));
    }

    let location_field = widget::combo_box(
        &locations.choices,
        &fl!("location"),
        Some(&state.location),
        Message::SelectEventLocation,
    )
    .on_input(Message::EventEditorLocationChanged)
    .on_close(Message::CloseEventLocationSuggestions)
    .menu_height(240)
    .width(Length::Fill);
    let mut location_theme = cosmic::theme::active();
    location_theme.transparent = false;
    let mut details = widget::settings::section().add(cosmic::iced::widget::themer(
        Some(location_theme),
        location_field,
    ));
    if locations
        .online
        .as_ref()
        .is_some_and(|online| online.has_results())
    {
        details = details.add(
            widget::button::link(fl!("online-location-attribution"))
                .padding(0)
                .on_press(Message::LaunchUrl(
                    "https://www.openstreetmap.org/copyright".into(),
                )),
        );
    } else if locations.online_failed {
        details = details.add(widget::text::caption(fl!("online-location-unavailable")));
    }
    let details = details.add(
        widget::text_editor(notes)
            .placeholder(fl!("event-notes"))
            .on_action(Message::EventEditorNotesChanged)
            .height(88)
            .padding(spacing().space_xxs)
            .class(cosmic::theme::iced::TextEditor::Custom(Box::new(
                text_input_editor_style,
            ))),
    );

    let mut reminder_rows = widget::column(Vec::new()).spacing(spacing().space_xxs);
    let mut lead_times = state.reminders.clone();
    lead_times.sort_unstable();
    lead_times.dedup();
    for minutes in lead_times {
        reminder_rows = reminder_rows.push(widget::settings::item(
            reminder_label(minutes),
            widget::button::icon(widget::icon::from_name("list-remove-symbolic"))
                .name(fl!("event-remove-reminder"))
                .on_press(Message::RemoveEventReminder(minutes)),
        ));
    }
    reminder_rows = reminder_rows.push(
        widget::button::text(fl!("event-add-reminder"))
            .on_press(Message::ToggleEventReminderOptions),
    );
    if state.adding_reminder {
        let mut presets = Vec::new();
        for minutes in REMINDER_PRESETS_MINUTES {
            presets.push(
                widget::button::standard(reminder_label(minutes))
                    .on_press_maybe(
                        (!state.reminders.contains(&minutes))
                            .then_some(Message::AddEventReminder(minutes)),
                    )
                    .into(),
            );
        }
        let custom = state
            .custom_reminder_input
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|minutes| *minutes >= 0 && !state.reminders.contains(minutes));
        reminder_rows = reminder_rows.push(widget::flex_row(presets)).push(
            widget::row(vec![
                widget::text_input(
                    fl!("custom-reminder-placeholder"),
                    &state.custom_reminder_input,
                )
                .on_input(Message::EventEditorCustomReminderInputChanged)
                .on_submit(|_| Message::EventEditorAddCustomReminder)
                .width(Length::Fill)
                .into(),
                widget::button::standard(fl!("event-add-custom-reminder"))
                    .on_press_maybe(custom.map(|_| Message::EventEditorAddCustomReminder))
                    .into(),
            ])
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(spacing().space_xs),
        );
    }

    let mut options = widget::settings::section().add(
        widget::settings::item::builder(fl!("event-more-options"))
            .toggler(state.more_options, |_| Message::ToggleEventOptions),
    );
    if state.more_options {
        if !state.all_day {
            options = options.add(
                widget::settings::item::builder(fl!("event-floating-time"))
                    .toggler(state.floating, Message::EventEditorFloatingToggled),
            );
            if !state.floating {
                options = options.add(widget::settings::item(
                    fl!("event-timezone"),
                    widget::text_input(fl!("event-timezone-placeholder"), &state.timezone)
                        .on_input(Message::EventEditorTimezoneChanged)
                        .width(Length::Fixed(240.0)),
                ));
            }
        }
        options = options.add(
            widget::settings::item::builder(fl!("task"))
                .toggler(state.is_task, Message::EventEditorTaskToggled),
        );
    }
    let mut sections = Vec::new();
    if state.recurring_series {
        sections.push(
            widget::settings::section()
                .add(widget::settings::item(
                    fl!("recurrence-edit-scope"),
                    widget::dropdown(
                        RecurrenceScope::OPTIONS
                            .into_iter()
                            .map(RecurrenceScope::label)
                            .collect::<Vec<_>>(),
                        RecurrenceScope::OPTIONS
                            .iter()
                            .position(|scope| *scope == state.recurrence_scope),
                        Message::EventEditorScopeChanged,
                    ),
                ))
                .into(),
        );
    }
    sections.extend([title.into(), when.into()]);
    if state.recurrence_scope != RecurrenceScope::Occurrence {
        sections.push(recurrence_section(state).into());
    }
    sections.push(widget::settings::section().add(reminder_rows).into());
    sections.extend([details.into(), options.into()]);
    let content = widget::settings::view_column(sections);

    let mut dialog = widget::dialog::dialog()
        .title(if state.event_id.is_some() {
            fl!("edit-event")
        } else {
            fl!("new-event")
        })
        .control(
            widget::scrollable(content)
                .width(Length::Fill)
                .height(Length::Fixed(420.0)),
        )
        .primary_action(
            widget::button::suggested(fl!("save"))
                .on_press_maybe(validation_error.is_none().then_some(Message::SaveEvent)),
        )
        .secondary_action(
            widget::button::standard(fl!("cancel")).on_press(Message::CloseEventEditor),
        )
        .width(Length::Fixed(640.0));

    if let Some(event_id) = state.event_id {
        let (label, action) = if state.recurring_series {
            match state.recurrence_scope {
                RecurrenceScope::Occurrence => (
                    fl!("delete-occurrence"),
                    Message::DeleteOccurrence(event_id, state.occurrence_start),
                ),
                RecurrenceScope::Following => (
                    fl!("delete-following"),
                    Message::DeleteFollowing(event_id, state.occurrence_start),
                ),
                RecurrenceScope::Series => (fl!("delete-series"), Message::DeleteEvent(event_id)),
            }
        } else {
            (fl!("delete"), Message::DeleteEvent(event_id))
        };
        dialog = dialog.tertiary_action(widget::button::destructive(label).on_press(action));
    }

    dialog.into()
}
