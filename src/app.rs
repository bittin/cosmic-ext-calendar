// SPDX-License-Identifier: GPL-3.0-only

use crate::features::calendar::{CalendarAnchor, CalendarSlot, EventSelection, LocalCalendar};
use crate::features::calendar_management::model::{CalendarKey, LocalCalendarMeta, calendar_color};
use crate::features::calendar_management::{state::CalendarForm, view as calendar_form};
use crate::features::event::model::{Event, EventTime};
use crate::features::event::recurrence::{RecurrenceEnd, RecurrenceFreq};
use crate::features::event::state::{
    EVENT_EDITOR_MINUTES, EVENT_EDITOR_YEARS, EventEditorState, RecurrenceScope,
    build_reminders_model, event_editor_months, shift_event_editor_start,
};
use crate::features::ics;
use crate::fl;
use crate::shared::config::{AppTheme, Config};
use crate::shared::ids::{CalendarId, EventId};
use crate::shared::storage::LocalStorage;
use cosmic::app::context_drawer;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::dialog::file_chooser;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Alignment, Length, Subscription};
use cosmic::prelude::*;
use cosmic::theme::spacing;
use cosmic::widget::{color_picker::ColorPickerUpdate, segmented_button::SingleSelect};
use cosmic::widget::{self, menu, nav_bar};
use cosmic::{cosmic_theme, theme};
use std::collections::HashMap;
use std::ops::Index;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

mod flags;
pub use flags::*;
mod settings;
pub use settings::*;

use crate::features::event::notifications::{ReminderKey, check_reminders};

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

#[derive(Default)]
struct IcsImportState {
    path: Option<PathBuf>,
    calendar: Option<CalendarId>,
    busy: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CalendarClick {
    Event(EventSelection),
    EmptySlot(CalendarSlot),
}

enum CalendarPopup {
    Details(EventSelection, Box<Event>, Option<String>),
}

struct OpenCalendarPopup {
    content: CalendarPopup,
    anchor: Option<CalendarAnchor>,
}

#[derive(Clone)]
enum CalendarClipboard {
    Copy(Event),
    Cut(EventSelection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarAction {
    Edit,
    Delete,
    Copy,
    Cut,
    Duplicate,
    Paste,
}

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// Display a context drawer with the designated page if defined.
    context_page: ContextPage,
    /// Main window width reported by COSMIC.
    window_width: f32,
    /// Contains items assigned to the nav bar panel.
    nav: nav_bar::Model,
    /// Contains items assigned to the tab bar.
    tabs: widget::segmented_button::Model<SingleSelect>,
    /// Key bindings for the application's menu bar.
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    // Configuration data that persists between application runs.
    config: Config,
    display_timezone_options: Vec<String>,
    ics_import: IcsImportState,
    // Calendar data that persists between application runs.
    calendar: LocalCalendar,
    calendar_popup: Option<OpenCalendarPopup>,
    calendar_clipboard: Option<CalendarClipboard>,
    calendar_click: Option<(CalendarClick, std::time::Instant)>,
    // Local calendar/event storage, persisted to disk as JSON.
    storage: Arc<Mutex<LocalStorage>>,
    toasts: widget::Toasts<Message>,
    storage_read_only: bool,
    calendar_to_archive: Option<(CalendarId, String)>,
    calendar_archive_busy: bool,
    // State of the event editor drawer, when open.
    editing_event: Option<EventEditorState>,
    // Multi-select segmented button backing the reminder-preset picker in
    // the event editor. Rebuilt from `editing_event.reminders` whenever the
    // editor opens or a preset is toggled; `Vec<i64>` remains the source of
    // truth that's actually persisted, since `segmented_button::Model` isn't
    // `Clone` and can't live inside `EventEditorState`.
    reminders_model: widget::segmented_button::Model<widget::segmented_button::MultiSelect>,
    calendar_form: Option<CalendarForm>,
    // Nav bar entity ids for the "Calendars" header + per-calendar rows,
    // tracked so they can be torn down and rebuilt on every CRUD change.
    calendar_nav_ids: Vec<nav_bar::Id>,
    calendars_collapsed: bool,
    // Current text in the header search bar; non-empty replaces the main
    // view with search results.
    search_query: String,
    // Reminders already notified this session, so the periodic check
    // doesn't re-fire the same notification every tick.
    fired_reminders: std::collections::HashSet<ReminderKey>,
    checking_reminders: bool,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    MenuSurface(cosmic::surface::Action),
    OpenRepositoryUrl,
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
    SetTheme(AppTheme),
    SetDisplayTimezone(usize),
    TabSelected(widget::segmented_button::Entity),
    LaunchUrl(String),
    NavigateNextDay,
    NavigatePreviousDay,
    NavigateNextMonth,
    NavigatePreviousMonth,
    NavigateNextYear,
    NavigatePreviousYear,
    NavigateToday,
    AddEvent(OffsetDateTime),
    SelectDate(OffsetDateTime),
    ShowDayAgenda(time::Date),
    CloseDayAgenda,
    SelectMonth(usize),
    SelectYear(usize),
    SelectDay(usize),
    OpenEventEditor(Option<EventId>, OffsetDateTime, bool),
    CalendarEventClicked(EventSelection, CalendarAnchor),
    CalendarEmptySlotClicked(CalendarSlot),
    CloseCalendarPopup,
    CalendarAction(CalendarAction, Option<EventSelection>, OffsetDateTime),
    CloseEventEditor,
    EventEditorSummaryChanged(String),
    EventEditorDescriptionChanged(String),
    EventEditorLocationChanged(String),
    EventEditorAllDayToggled(bool),
    EventEditorTimezoneChanged(String),
    EventEditorFloatingToggled(bool),
    EventEditorTaskToggled(bool),
    EventEditorStartYearChanged(usize),
    EventEditorStartMonthChanged(usize),
    EventEditorStartDayChanged(usize),
    EventEditorStartHourChanged(usize),
    EventEditorStartMinuteChanged(usize),
    EventEditorEndYearChanged(usize),
    EventEditorEndMonthChanged(usize),
    EventEditorEndDayChanged(usize),
    EventEditorEndHourChanged(usize),
    EventEditorEndMinuteChanged(usize),
    EventEditorRecurrenceChanged(usize),
    EventEditorRecurrenceIntervalChanged(usize),
    EventEditorRecurrenceWeekdayToggled(usize, bool),
    EventEditorRecurrenceEndChanged(usize),
    EventEditorRecurrenceUntilYearChanged(usize),
    EventEditorRecurrenceUntilMonthChanged(usize),
    EventEditorRecurrenceUntilDayChanged(usize),
    EventEditorRecurrenceCountChanged(usize),
    EventEditorCalendarChanged(usize),
    SaveEvent,
    EventEditorScopeChanged(usize),
    EventSaved(std::result::Result<(), String>),
    DeleteEvent(EventId),
    DeleteOccurrence(EventId, OffsetDateTime),
    DeleteFollowing(EventId, OffsetDateTime),
    EventDeleted(std::result::Result<EventId, String>),
    ImportIcs,
    IcsFileChosen(Option<PathBuf>),
    SelectImportCalendar(CalendarId),
    ConfirmIcsImport,
    IcsImported(std::result::Result<(usize, usize), String>),
    ExportIcs,
    IcsExported(std::result::Result<Option<PathBuf>, String>),
    OpenNewCalendarForm,
    StartEditCalendar(CalendarId),
    CalendarNameChanged(String),
    SetCalendarColor(Option<String>),
    CalendarColorPicker(ColorPickerUpdate),
    SaveCalendarForm,
    CalendarFormSaved(std::result::Result<(), String>),
    CloseCalendarForm,
    ToggleCalendarVisibility(CalendarId, bool),
    CalendarVisibilityToggled(std::result::Result<(), String>),
    DeleteCalendar(CalendarId),
    CancelArchiveCalendar,
    ConfirmArchiveCalendar,
    RestoreCalendar(CalendarId),
    CalendarArchiveChanged(std::result::Result<(), String>),
    CloseToast(widget::ToastId),
    SearchChanged(String),
    SelectSearchResult(EventId, OffsetDateTime),
    EventEditorReminderToggled(widget::segmented_button::Entity),
    EventEditorCustomReminderInputChanged(String),
    EventEditorCustomReminderEditToggled(bool),
    EventEditorAddCustomReminder,
    CheckReminders,
    RemindersFired(Vec<ReminderKey>),
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = Flags;

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "dev.edfloreshz.Calendar";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Create a nav bar with three page items.
        let nav = nav_bar::Model::default();

        // Create a tab bar with three page items.
        let tabs = widget::segmented_button::Model::builder()
            .insert(|b| {
                b.text(fl!("month"))
                    .icon(widget::icon::from_name("office-calendar-symbolic"))
                    .data(Tab::Month)
                    .activate()
            })
            .insert(|b| {
                b.text(fl!("week"))
                    .icon(widget::icon::from_name("x-office-spreadsheet-symbolic"))
                    .data(Tab::Week)
            })
            .insert(|b| {
                b.text(fl!("day"))
                    .icon(widget::icon::from_name("calendar-go-today-symbolic"))
                    .data(Tab::Day)
            })
            .build();

        let mut display_timezone_options = chrono_tz::TZ_VARIANTS
            .iter()
            .map(|zone| zone.name().to_owned())
            .collect::<Vec<_>>();
        display_timezone_options.sort_unstable();
        display_timezone_options.insert(0, fl!("display-timezone-system"));

        // Construct the app model with the runtime's core.
        let mut app = AppModel {
            core,
            context_page: ContextPage::default(),
            window_width: 0.0,
            nav,
            tabs,
            key_binds: HashMap::new(),
            // Optional configuration file for an application.
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => {
                        // for why in errors {
                        //     tracing::error!(%why, "error loading app config");
                        // }

                        config
                    }
                })
                .unwrap_or_default(),
            display_timezone_options,
            ics_import: IcsImportState::default(),
            calendar: LocalCalendar::default(),
            calendar_popup: None,
            calendar_clipboard: None,
            calendar_click: None,
            storage: Arc::new(Mutex::new(LocalStorage::load().unwrap_or_else(|err| {
                tracing::error!("failed to load local storage: {err}");
                LocalStorage::unavailable(err.to_string())
            }))),
            toasts: widget::Toasts::new(Message::CloseToast),
            storage_read_only: false,
            calendar_to_archive: None,
            calendar_archive_busy: false,
            editing_event: None,
            reminders_model: build_reminders_model(&[]),
            calendar_form: None,
            calendar_nav_ids: Vec::new(),
            calendars_collapsed: false,
            search_query: String::new(),
            fired_reminders: std::collections::HashSet::new(),
            checking_reminders: false,
        };

        let mut tasks = Vec::new();
        // Seed a default calendar only if the store has no calendar records.
        {
            let mut storage = app.storage.lock().unwrap();
            if let Some(error) = storage.load_error() {
                app.storage_read_only = true;
                let toast = widget::Toast::new(fl!("calendar-storage-load-error", error = error))
                    .duration(widget::toaster::Duration::Long);
                tasks.push(app.toasts.push(toast).map(cosmic::action::app));
            } else if !storage.has_calendars() {
                let personal = LocalCalendarMeta {
                    id: crate::shared::ids::CalendarId::new(),
                    name: fl!("personal-calendar"),
                    description: None,
                    color: None,
                    visible: true,
                    archived: false,
                };
                if let Err(err) = storage.create_calendar(personal) {
                    let toast =
                        widget::Toast::new(fl!("default-calendar-error", error = err.to_string()));
                    tasks.push(app.toasts.push(toast).map(cosmic::action::app));
                    app.calendar_form = Some(CalendarForm::new(None, String::new(), None));
                }
            }
        }

        app.refresh_calendar_nav();

        app.core.nav_bar_set_toggled(false);

        tasks.push(app.update_title());
        tasks.push(cosmic::command::set_theme(app.config.app_theme.theme()));

        app.refresh_display_timezone();
        app.calendar.set_today();

        (app, Task::batch(tasks))
    }

    /// Elements to pack at the start of the header bar.
    fn header_start<'a>(&'a self) -> Vec<Element<'a, Self::Message>> {
        let menu_bar = menu::bar(vec![menu::Tree::with_children(
            widget::RcElementWrapper::new(
                widget::button::icon(widget::icon::from_name("open-menu-symbolic"))
                    .padding([4, 12])
                    .class(theme::Button::MenuRoot)
                    .name(fl!("app-menu"))
                    .into(),
            ),
            menu::items(
                &self.key_binds,
                vec![
                    menu::Item::Button(fl!("new-calendar"), None, MenuAction::NewCalendar),
                    menu::Item::Button(fl!("import-ics"), None, MenuAction::ImportIcs),
                    menu::Item::Button(fl!("export-ics"), None, MenuAction::ExportIcs),
                    menu::Item::Button(
                        fl!("archived-calendars"),
                        None,
                        MenuAction::ArchivedCalendars,
                    ),
                    menu::Item::Button(fl!("settings"), None, MenuAction::Settings),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("about"), None, MenuAction::About),
                ],
            ),
        )])
        .item_height(menu::ItemHeight::Dynamic(40))
        .item_width(menu::ItemWidth::Uniform(320))
        .spacing(spacing().space_xxxs.into())
        .window_id_maybe(self.core.main_window_id())
        .on_surface_action(Message::MenuSurface);

        vec![menu_bar.into()]
    }

    fn header_center<'a>(&'a self) -> Vec<Element<'a, Self::Message>> {
        let width = if self.window_width > 0.0 {
            (self.window_width / self.core.scale_factor() - 260.0).clamp(100.0, 240.0)
        } else {
            240.0
        };
        vec![
            widget::search_input(fl!("search"), &self.search_query)
                .on_input(Message::SearchChanged)
                .on_clear(Message::SearchChanged(String::new()))
                .width(Length::Fixed(width))
                .into(),
        ]
    }

    fn header_end<'a>(&'a self) -> Vec<Element<'a, Self::Message>> {
        let selected_slot = self.calendar.selected_slot.or_else(|| {
            self.show_month()
                .then(|| CalendarSlot::MonthDay(self.calendar.selected_date.date()))
        });
        let create_message = selected_slot
            .map_or(Message::AddEvent(self.calendar.current_date), |slot| {
                self.create_event_at_slot(slot)
            });
        let today = widget::button::icon(widget::icon::from_name("calendar-go-today-symbolic"))
            .name(fl!("today"))
            .class(cosmic::style::Button::NavToggle)
            .on_press_maybe((!self.calendar.today()).then_some(Message::NavigateToday));
        let create_event = widget::button::icon(widget::icon::from_name("list-add-symbolic"))
            .name(fl!("create-event"))
            .on_press(create_message);

        vec![
            widget::tooltip(
                today,
                widget::text(fl!("today")),
                widget::tooltip::Position::Bottom,
            )
            .into(),
            widget::tooltip(
                create_event,
                widget::text(fl!("create-event")),
                widget::tooltip::Position::Bottom,
            )
            .into(),
        ]
    }

    fn footer<'a>(&'a self) -> Option<Element<'a, Self::Message>> {
        let days = widget::row(vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .on_press(Message::NavigatePreviousDay)
                .into(),
            widget::dropdown(
                self.calendar
                    .days()
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<String>>(),
                self.calendar
                    .days()
                    .iter()
                    .position(|m| *m == self.calendar.selected_date.day()),
                Message::SelectDay,
            )
            .into(),
            widget::button::icon(widget::icon::from_name("go-next-symbolic"))
                .on_press(Message::NavigateNextDay)
                .into(),
        ])
        .align_y(Vertical::Center)
        .spacing(spacing().space_xxs);

        let months = widget::row(vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .tooltip(fl!("previous-month"))
                .name(fl!("previous-month"))
                .on_press(Message::NavigatePreviousMonth)
                .into(),
            widget::dropdown(
                self.calendar
                    .months()
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<String>>(),
                self.calendar
                    .months()
                    .iter()
                    .position(|m| *m == self.calendar.selected_date.month()),
                Message::SelectMonth,
            )
            .into(),
            widget::button::icon(widget::icon::from_name("go-next-symbolic"))
                .tooltip(fl!("next-month"))
                .name(fl!("next-month"))
                .on_press(Message::NavigateNextMonth)
                .into(),
        ])
        .align_y(Vertical::Center)
        .spacing(spacing().space_xxs);

        let years = widget::row(vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .on_press(Message::NavigatePreviousYear)
                .into(),
            widget::dropdown(
                self.calendar
                    .years()
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<String>>(),
                self.calendar
                    .years()
                    .iter()
                    .position(|y| *y == self.calendar.selected_date.year()),
                Message::SelectYear,
            )
            .into(),
            widget::button::icon(widget::icon::from_name("go-next-symbolic"))
                .on_press(Message::NavigateNextYear)
                .into(),
        ])
        .align_y(Vertical::Center)
        .spacing(spacing().space_xxs);

        Some(
            widget::container(
                widget::flex_row(vec![days.into(), months.into(), years.into()])
                    .width(Length::Fill)
                    .justify_content(widget::JustifyContent::Center)
                    .justify_items(Alignment::Center)
                    .spacing(spacing().space_xs),
            )
            .class(cosmic::theme::Container::Card)
            .padding(spacing().space_xxxs)
            .width(Length::Fill)
            .into(),
        )
    }

    /// Enables the COSMIC application to create a nav bar with this model.
    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    /// The nav bar's per-item right-click menu: "New calendar" on the
    /// "Calendars" header row, "Rename"/"Delete" on each calendar row.
    /// `nav_bar`'s context menu implementation asserts that this yields
    /// exactly one entry (possibly empty) per nav item, in iteration order.
    fn nav_context_menu(&self) -> Option<Vec<menu::Tree<cosmic::Action<Self::Message>>>> {
        let can_delete = !self.storage_read_only;
        let header_id = self.calendar_nav_ids.first().copied();

        Some(menu::nav_context(
            &HashMap::new(),
            self.nav
                .iter()
                .map(|id| {
                    if let Some(calendar_id) = self.nav.data::<CalendarId>(id).copied() {
                        let mut items = vec![menu::Item::Button(
                            fl!("rename"),
                            None,
                            NavMenuAction::RenameCalendar(calendar_id),
                        )];
                        if can_delete {
                            items.push(menu::Item::Button(
                                fl!("delete-calendar"),
                                None,
                                NavMenuAction::DeleteCalendar(calendar_id),
                            ));
                        }
                        items
                    } else if Some(id) == header_id {
                        vec![menu::Item::Button(
                            fl!("new-calendar"),
                            None,
                            NavMenuAction::NewCalendar,
                        )]
                    } else {
                        Vec::new()
                    }
                })
                .collect(),
        ))
    }

    /// Display a context drawer if the context page is requested.
    fn context_drawer<'a>(&'a self) -> Option<context_drawer::ContextDrawer<'a, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }

        Some(match self.context_page {
            ContextPage::DayAgenda => {
                let storage = self.storage.lock().unwrap();
                context_drawer::context_drawer(
                    self.calendar.day_agenda(&storage, self),
                    Message::CloseDayAgenda,
                )
                .title(fl!("month-day-agenda"))
            }
            ContextPage::Settings => context_drawer::context_drawer(
                self.settings_view(),
                Message::ToggleContextPage(ContextPage::Settings),
            )
            .title(fl!("settings")),
            ContextPage::IcsImport => context_drawer::context_drawer(
                self.ics_import_view(),
                Message::ToggleContextPage(ContextPage::IcsImport),
            )
            .title(fl!("import-ics")),
            ContextPage::About => context_drawer::context_drawer(
                self.about(),
                Message::ToggleContextPage(ContextPage::About),
            )
            .title(fl!("about")),
            ContextPage::ArchivedCalendars => context_drawer::context_drawer(
                self.archived_calendars_view(),
                Message::ToggleContextPage(ContextPage::ArchivedCalendars),
            )
            .title(fl!("archived-calendars")),
        })
    }

    /// Displays calendar creation, calendar deletion confirmation, or the event editor.
    fn dialog<'a>(&'a self) -> Option<Element<'a, Self::Message>> {
        if let Some((_, name)) = &self.calendar_to_archive {
            return Some(
                widget::dialog::dialog()
                    .title(fl!("delete-calendar"))
                    .body(fl!("archive-calendar-confirmation", name = name.as_str()))
                    .primary_action(widget::button::destructive(fl!("delete")).on_press_maybe(
                        (!self.calendar_archive_busy).then_some(Message::ConfirmArchiveCalendar),
                    ))
                    .secondary_action(widget::button::standard(fl!("cancel")).on_press_maybe(
                        (!self.calendar_archive_busy).then_some(Message::CancelArchiveCalendar),
                    ))
                    .width(Length::Fixed(420.0))
                    .into(),
            );
        }
        if let Some(form) = &self.calendar_form {
            let (title, action) = if form.id.is_some() {
                (fl!("edit-calendar"), fl!("save"))
            } else {
                (fl!("new-calendar"), fl!("create"))
            };
            return Some(widget::toaster(
                &self.toasts,
                widget::dialog::dialog()
                    .title(title)
                    .control(widget::scrollable(calendar_form::view(form)).width(Length::Fill))
                    .primary_action(widget::button::suggested(action)
                        .on_press_maybe(form.can_save().then_some(Message::SaveCalendarForm)))
                    .secondary_action(widget::button::standard(fl!("cancel"))
                        .on_press_maybe((!form.writing).then_some(Message::CloseCalendarForm)))
                    .width(Length::Fixed(if form.picker.get_is_active() { 620.0 } else { 380.0 })),
            ));
        }
        self.editing_event
            .as_ref()
            .map(|_| widget::toaster(&self.toasts, self.event_editor_view()))
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view<'a>(&'a self) -> Element<'a, Self::Message> {
        let tabs = widget::segmented_button::horizontal(&self.tabs)
            .padding(2)
            .button_spacing(spacing().space_xxs)
            .button_alignment(cosmic::iced::Alignment::Center)
            .on_activate(Message::TabSelected)
            .into();

        let active_tab = if !self.search_query.is_empty() {
            self.search_results_view()
        } else {
            match self.tabs.active_data::<Tab>() {
                Some(Tab::Month) => self
                    .calendar
                    .month_view(&self.storage.lock().unwrap(), self),
                Some(tab @ (Tab::Week | Tab::Day)) => {
                    let storage = self.storage.lock().unwrap();
                    if matches!(tab, Tab::Week) {
                        self.calendar.week_view(&storage, self).into()
                    } else {
                        self.calendar.day_view(&storage, self).into()
                    }
                }
                None => widget::text::title1("Welcome")
                    .apply(widget::container)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .into(),
            }
        };

        let content = widget::column(vec![tabs, active_tab]).spacing(spacing().space_xxs);
        let calendar = self.attach_calendar_popup(content, None);
        if self.editing_event.is_some() || self.calendar_form.is_some() {
            calendar
        } else {
            widget::toaster(&self.toasts, calendar)
        }
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They are started at the
    /// beginning of the application, and persist through its lifetime.
    fn subscription(&self) -> Subscription<Self::Message> {
        let reminders_tick = cosmic::iced::time::every(std::time::Duration::from_secs(30))
            .map(|_| Message::CheckReminders);

        let config_updates = self
            .core()
            .watch_config::<Config>(Self::APP_ID)
            .map(|update| Message::UpdateConfig(update.config));

        let month_shortcuts = if self.show_month() && self.editing_event.is_none() {
            cosmic::iced::event::listen_with(|event, status, _| {
                use cosmic::iced::{
                    Event, event,
                    keyboard::{self, key::Named},
                };

                let Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    repeat: false,
                    ..
                }) = event
                else {
                    return None;
                };
                if status != event::Status::Ignored || modifiers != keyboard::Modifiers::CTRL {
                    return None;
                }
                match key.as_ref() {
                    keyboard::Key::Named(Named::PageUp) => Some(Message::NavigatePreviousMonth),
                    keyboard::Key::Named(Named::PageDown) => Some(Message::NavigateNextMonth),
                    keyboard::Key::Named(Named::Home) => Some(Message::NavigateToday),
                    _ => None,
                }
            })
        } else {
            Subscription::none()
        };
        Subscription::batch([reminders_tick, config_updates, month_shortcuts])
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        let mut tasks = vec![];
        match message {
            Message::MenuSurface(action) => return cosmic::surface::surface_task(action),
            Message::CalendarEventClicked(key, anchor) => {
                self.calendar.selected_event = Some(key);
                self.calendar.selected_slot = None;
                if self.calendar_double_click(CalendarClick::Event(key)) {
                    self.open_event_details(key, Some(anchor));
                } else {
                    self.calendar_popup = None;
                }
            }
            Message::CalendarEmptySlotClicked(slot) => {
                self.calendar.select_slot(slot);
                self.calendar_popup = None;
                if self.calendar_double_click(CalendarClick::EmptySlot(slot)) {
                    return self.update(self.create_event_at_slot(slot));
                }
                if matches!(slot, CalendarSlot::MonthDay(_)) {
                    self.open_day_agenda();
                }
            }
            Message::CloseCalendarPopup => self.calendar_popup = None,
            Message::CalendarAction(action, selection, date) => {
                self.calendar_click = None;
                self.calendar_popup = None;
                match action {
                    CalendarAction::Edit => {
                        if let Some((id, start)) = selection {
                            return self.update(Message::OpenEventEditor(Some(id), start, false));
                        }
                    }
                    CalendarAction::Delete => {
                        if self.editing_event.is_some() {
                            return Task::none();
                        }
                        if let Some((id, start)) = selection {
                            let action = {
                                let storage = self.storage.lock().unwrap();
                                storage.get_event(id).map(|event| {
                                    self.editing_event =
                                        Some(EventEditorState::from_event(event, start));
                                    if event.is_recurring() {
                                        Message::DeleteOccurrence(id, start)
                                    } else {
                                        Message::DeleteEvent(id)
                                    }
                                })
                            };
                            if let Some(action) = action {
                                return self.update(action);
                            }
                        }
                    }
                    CalendarAction::Copy | CalendarAction::Cut => {
                        if let Some(key) = selection {
                            self.calendar_clipboard = if matches!(action, CalendarAction::Cut) {
                                Some(CalendarClipboard::Cut(key))
                            } else {
                                self.storage
                                    .lock()
                                    .unwrap()
                                    .get_event(key.0)
                                    .cloned()
                                    .map(CalendarClipboard::Copy)
                            };
                        }
                    }
                    CalendarAction::Duplicate | CalendarAction::Paste => {
                        let source = if matches!(action, CalendarAction::Duplicate) {
                            selection.and_then(|key| {
                                self.storage
                                    .lock()
                                    .unwrap()
                                    .get_event(key.0)
                                    .cloned()
                                    .map(CalendarClipboard::Copy)
                            })
                        } else {
                            self.calendar_clipboard.clone()
                        };
                        if let Some(source) = source {
                            let cut = matches!(source, CalendarClipboard::Cut(_));
                            match self.paste_occurrence(source, date) {
                                Ok(key) => {
                                    self.calendar.selected_event = Some(key);
                                    self.calendar.selected_slot = None;
                                    if cut {
                                        self.calendar_clipboard = None;
                                    }
                                }
                                Err(err) => {
                                    tracing::error!("failed to paste calendar event: {err}");
                                    tasks.push(self.toast(fl!("calendar-paste-error")));
                                }
                            }
                        }
                    }
                }
            }
            Message::TabSelected(tab) => {
                self.calendar_popup = None;
                self.calendar_click = None;
                let changed = self.tabs.active() != tab;
                self.tabs.activate(tab);
                if changed {
                    self.calendar.selected_slot = None;
                    tasks.push(self.scroll_calendar_to_now());
                }
                if self.context_page == ContextPage::DayAgenda
                    && !matches!(self.tabs.active_data::<Tab>(), Some(Tab::Month))
                {
                    self.core.set_show_context(false);
                }
            }
            Message::OpenRepositoryUrl => {
                _ = open::that_detached(REPOSITORY);
            }
            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    // Close the context drawer if the toggled context page is the same.
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    // Open the context drawer to display the requested context page.
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
            }
            Message::UpdateConfig(config) => {
                let theme_changed = self.config.app_theme != config.app_theme;
                self.config = config;
                self.refresh_display_timezone();
                if theme_changed {
                    tasks.push(cosmic::command::set_theme(self.config.app_theme.theme()));
                }
            }
            Message::SetTheme(app_theme) => {
                if app_theme != self.config.app_theme {
                    let mut config = self.config.clone();
                    match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                        .and_then(|context| config.set_app_theme(&context, app_theme))
                    {
                        Ok(_) => {
                            self.config = config;
                            tasks.push(cosmic::command::set_theme(app_theme.theme()));
                        }
                        Err(err) => {
                            tasks
                                .push(self.toast(fl!("theme-save-error", error = err.to_string())));
                        }
                    }
                }
            }
            Message::SetDisplayTimezone(index) => {
                if let Some(option) = self.display_timezone_options.get(index) {
                    let value = if index == 0 { String::new() } else { option.clone() };
                    if value != self.config.display_timezone {
                        let mut config = self.config.clone();
                        match cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                            .and_then(|context| config.set_display_timezone(&context, value))
                        {
                            Ok(_) => {
                                self.config = config;
                                self.refresh_display_timezone();
                            }
                            Err(err) => tasks.push(
                                self.toast(fl!("display-timezone-save-error", error = err.to_string())),
                            ),
                        }
                    }
                }
            }
            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("failed to open {url:?}: {err}");
                }
            },
            Message::AddEvent(date) => {
                self.calendar_click = None;
                tasks.push(cosmic::task::message(Message::OpenEventEditor(
                    None, date, false,
                )));
            }
            Message::OpenEventEditor(event_id, date, all_day) => {
                if self.editing_event.is_some() {
                    return Task::none();
                }
                let state = if let Some(id) = event_id {
                    let storage = self.storage.lock().unwrap();
                    let Some(event) = storage.get_event(id) else {
                        return Task::none();
                    };
                    let mut state = EventEditorState::from_event(event, date);
                    if state.recurring_series {
                        state.set_recurrence_scope(event, RecurrenceScope::Occurrence);
                    }
                    state
                } else {
                    let calendar = {
                        let storage = self.storage.lock().unwrap();
                        storage
                            .list_calendars()
                            .into_iter()
                            .min_by(|a, b| {
                                (!a.visible, &a.name, a.id).cmp(&(!b.visible, &b.name, b.id))
                            })
                            .map(|meta| CalendarKey::Local(meta.id))
                    };
                    let Some(calendar) = calendar else {
                        return cosmic::task::message(Message::OpenNewCalendarForm);
                    };
                    let mut state = EventEditorState::new(calendar, date);
                    state.all_day = all_day;
                    state.timezone = self
                        .calendar
                        .display_timezone
                        .map(|zone| zone.name().to_string())
                        .unwrap_or_default();
                    state
                };
                self.calendar_click = None;
                self.reminders_model = build_reminders_model(&state.reminders);
                self.editing_event = Some(state);
            }
            Message::CloseEventEditor => {
                if !self
                    .editing_event
                    .as_ref()
                    .is_some_and(|state| state.writing)
                {
                    self.editing_event = None;
                }
            }
            Message::EventEditorSummaryChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.summary = value;
                }
            }
            Message::EventEditorDescriptionChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.description = value;
                }
            }
            Message::EventEditorLocationChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.location = value;
                }
            }
            Message::EventEditorFloatingToggled(floating) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.floating = floating;
                }
            }
            Message::EventEditorTimezoneChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.timezone = value;
                }
            }
            Message::EventEditorAllDayToggled(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.all_day = value;
                }
            }
            Message::EventEditorTaskToggled(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.is_task = value;
                }
            }
            Message::EventEditorStartYearChanged(idx) | Message::EventEditorEndYearChanged(idx) => {
                let is_start = matches!(message, Message::EventEditorStartYearChanged(_));
                if let Some(state) = self.editing_event.as_mut() {
                    let date = if is_start { state.start } else { state.end };
                    let year = EVENT_EDITOR_YEARS[0] + idx as i32;
                    let day = date.day().min(date.month().length(year));
                    if let Ok(new_date) = date
                        .replace_day(day)
                        .and_then(|date| date.replace_year(year))
                    {
                        if is_start {
                            shift_event_editor_start(state, new_date);
                        } else {
                            state.end = new_date;
                        }
                    }
                }
            }
            Message::EventEditorStartMonthChanged(idx)
            | Message::EventEditorEndMonthChanged(idx) => {
                let is_start = matches!(message, Message::EventEditorStartMonthChanged(_));
                if let Some(state) = self.editing_event.as_mut() {
                    let date = if is_start { state.start } else { state.end };
                    if let Some(&month) = event_editor_months().get(idx) {
                        let day = date.day().min(month.length(date.year()));
                        if let Ok(new_date) = date
                            .replace_day(day)
                            .and_then(|date| date.replace_month(month))
                        {
                            if is_start {
                                shift_event_editor_start(state, new_date);
                            } else {
                                state.end = new_date;
                            }
                        }
                    }
                }
            }
            Message::EventEditorStartDayChanged(idx) | Message::EventEditorEndDayChanged(idx) => {
                let is_start = matches!(message, Message::EventEditorStartDayChanged(_));
                if let Some(state) = self.editing_event.as_mut() {
                    let date = if is_start { state.start } else { state.end };
                    if let Ok(new_date) = date.replace_day(idx as u8 + 1) {
                        if is_start {
                            shift_event_editor_start(state, new_date);
                        } else {
                            state.end = new_date;
                        }
                    }
                }
            }
            Message::EventEditorStartHourChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let Ok(new_start) = state.start.replace_hour(idx as u8) {
                        shift_event_editor_start(state, new_start);
                    }
                }
            }
            Message::EventEditorStartMinuteChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if idx < EVENT_EDITOR_MINUTES {
                        if let Ok(new_start) = state.start.replace_minute(idx as u8) {
                            shift_event_editor_start(state, new_start);
                        }
                    }
                }
            }
            Message::EventEditorEndHourChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let Ok(new_end) = state.end.replace_hour(idx as u8) {
                        state.end = new_end;
                    }
                }
            }
            Message::EventEditorEndMinuteChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if idx < EVENT_EDITOR_MINUTES {
                        if let Ok(new_end) = state.end.replace_minute(idx as u8) {
                            state.end = new_end;
                        }
                    }
                }
            }
            Message::EventEditorRecurrenceChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let Some(freq) = RecurrenceFreq::ALL.get(idx) {
                        state.recurrence.freq = *freq;
                    }
                }
            }
            Message::EventEditorRecurrenceIntervalChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.recurrence.interval = idx as u16 + 1;
                }
            }
            Message::EventEditorRecurrenceWeekdayToggled(idx, on) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if idx < 7 {
                        state.recurrence.by_weekday[idx] = on;
                    }
                }
            }
            Message::EventEditorRecurrenceEndChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.recurrence.end = match idx {
                        1 => RecurrenceEnd::Until(state.start.date() + time::Duration::days(30)),
                        2 => RecurrenceEnd::Count(10),
                        _ => RecurrenceEnd::Never,
                    };
                }
            }
            Message::EventEditorRecurrenceUntilYearChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let RecurrenceEnd::Until(until) = &mut state.recurrence.end {
                        let year = EVENT_EDITOR_YEARS[0] + idx as i32;
                        if let Ok(new_date) = until.replace_year(year) {
                            *until = new_date;
                        }
                    }
                }
            }
            Message::EventEditorRecurrenceUntilMonthChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let RecurrenceEnd::Until(until) = &mut state.recurrence.end {
                        if let Some(month) = event_editor_months().get(idx) {
                            let day = until.day().min(month.length(until.year()));
                            if let Ok(new_date) =
                                until.replace_month(*month).and_then(|d| d.replace_day(day))
                            {
                                *until = new_date;
                            }
                        }
                    }
                }
            }
            Message::EventEditorRecurrenceUntilDayChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let RecurrenceEnd::Until(until) = &mut state.recurrence.end {
                        if let Ok(new_date) = until.replace_day(idx as u8 + 1) {
                            *until = new_date;
                        }
                    }
                }
            }
            Message::EventEditorRecurrenceCountChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let RecurrenceEnd::Count(count) = &mut state.recurrence.end {
                        *count = idx as u32 + 1;
                    }
                }
            }
            Message::EventEditorCalendarChanged(idx) => {
                if let Some(state) = self.editing_event.as_mut() {
                    let storage = self.storage.lock().unwrap();
                    let mut calendars = storage.list_calendars();
                    calendars.sort_by(|a, b| a.name.cmp(&b.name));
                    if let Some(meta) = calendars.get(idx) {
                        state.calendar = CalendarKey::Local(meta.id);
                    }
                }
            }
            Message::EventEditorScopeChanged(scope) => {
                if let Some(state) = self.editing_event.as_mut() {
                    let storage = self.storage.lock().unwrap();
                    if let Some(parent) = state.event_id.and_then(|id| storage.get_event(id)) {
                        if let Some(&scope) = RecurrenceScope::OPTIONS.get(scope) {
                            state.set_recurrence_scope(parent, scope);
                        }
                    }
                }
            }
            Message::SaveEvent => {
                if let Some(state) = self
                    .editing_event
                    .as_mut()
                    .filter(|state| !state.writing && state.validation_error().is_none())
                {
                    state.writing = true;
                    let state = state.clone();
                    let storage = self.storage.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let mut storage = storage.lock().unwrap();
                                let existing = state
                                    .event_id
                                    .map(|id| {
                                        storage
                                            .get_event(id)
                                            .ok_or_else(|| "event not found".to_string())
                                    })
                                    .transpose()?;
                                let mut event = state.build_event(existing);
                                if state.recurrence_scope == RecurrenceScope::Occurrence {
                                    let parent =
                                        existing.ok_or_else(|| "series not found".to_string())?;
                                    if parent.exdates.contains(&state.occurrence_start)
                                        || parent
                                            .recurrence_cutoff()
                                            .is_some_and(|end| state.occurrence_start >= end)
                                    {
                                        return Err("occurrence was already changed".to_string());
                                    }
                                    event.id = EventId::new();
                                    event.series_id = Some(parent.id);
                                    event.recurrence_id = Some(state.occurrence_start);
                                    event.rrule = None;
                                    event.exdates.clear();
                                    event.rdates.clear();
                                    event.predecessor = None;
                                    event.repeat_before = None;
                                    event.split_before = None;
                                } else if state.recurrence_scope == RecurrenceScope::Following {
                                    let parent =
                                        existing.ok_or_else(|| "series not found".to_string())?;
                                    event
                                        .start_following_series(parent, state.occurrence_start)
                                        .map_err(|err| err.to_string())?;
                                }
                                storage.create_event(event).map_err(|err| err.to_string())
                            })
                            .await
                            .map_err(|err| err.to_string())?
                        },
                        |result| cosmic::action::app(Message::EventSaved(result)),
                    ));
                }
            }
            Message::EventSaved(result) => match result {
                Ok(()) => {
                    self.editing_event = None;
                }
                Err(err) => {
                    tracing::error!("failed to save event: {err}");
                    tasks.push(self.toast(fl!("event-save-error")));
                    if let Some(state) = self.editing_event.as_mut() {
                        state.writing = false;
                    }
                }
            },
            Message::DeleteEvent(id) => {
                if !self.begin_event_deletion(id) {
                    return Task::none();
                }
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut storage = storage.lock().unwrap();
                            storage.delete_event(id).map(|()| id)
                        })
                        .await
                        .map_err(|err| err.to_string())?
                        .map_err(|err| err.to_string())
                    },
                    |result| cosmic::action::app(Message::EventDeleted(result)),
                ));
            }
            Message::DeleteOccurrence(id, occurrence_start)
            | Message::DeleteFollowing(id, occurrence_start) => {
                if !self.begin_event_deletion(id) {
                    return Task::none();
                }
                let following = matches!(message, Message::DeleteFollowing(..));
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut storage = storage.lock().unwrap();
                            let Some(mut event) = storage.get_event(id).cloned() else {
                                return Err("event not found".to_string());
                            };
                            if following {
                                event.repeat_before = Some(
                                    event
                                        .repeat_before
                                        .map_or(occurrence_start, |end| end.min(occurrence_start)),
                                );
                            } else if !event.exdates.contains(&occurrence_start) {
                                event.exdates.push(occurrence_start);
                            }
                            event.updated_at = OffsetDateTime::now_utc();
                            event.sequence += 1;
                            storage
                                .update_event(event)
                                .map(|()| id)
                                .map_err(|err| err.to_string())
                        })
                        .await
                        .map_err(|err| err.to_string())?
                    },
                    |result| cosmic::action::app(Message::EventDeleted(result)),
                ));
            }
            Message::EventDeleted(result) => match result {
                Ok(_id) => {
                    self.editing_event = None;
                }
                Err(err) => {
                    tracing::error!("failed to delete event: {err}");
                    tasks.push(self.toast(fl!("event-delete-error", error = err.as_str())));
                    if let Some(state) = self.editing_event.as_mut() {
                        state.writing = false;
                    }
                }
            },
            Message::ImportIcs if self.ics_import.busy => {
                self.context_page = ContextPage::IcsImport;
                self.core.set_show_context(true);
            }
            Message::ImportIcs => {
                tasks.push(Task::perform(
                    async {
                        let dialog = file_chooser::open::Dialog::new().title(fl!("import-ics"));
                        match dialog.open_file().await {
                            Ok(response) => response.url().to_file_path().ok(),
                            Err(_) => None,
                        }
                    },
                    |path| cosmic::action::app(Message::IcsFileChosen(path)),
                ));
            }
            Message::IcsFileChosen(Some(path)) if !self.ics_import.busy => {
                let storage = self.storage.lock().unwrap();
                let calendar = storage
                    .list_calendars()
                    .into_iter()
                    .min_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)))
                    .map(|calendar| calendar.id);
                self.ics_import = IcsImportState {
                    path: Some(path),
                    calendar,
                    ..IcsImportState::default()
                };
                self.context_page = ContextPage::IcsImport;
                self.core.set_show_context(true);
            }
            Message::IcsFileChosen(_) => {}
            Message::SelectImportCalendar(id) => self.ics_import.calendar = Some(id),
            Message::ConfirmIcsImport => {
                if let (Some(path), Some(calendar_id)) = (
                    self.ics_import
                        .path
                        .take_if(|_| !self.ics_import.busy && self.ics_import.calendar.is_some()),
                    self.ics_import.calendar,
                ) {
                    self.ics_import.busy = true;
                    let storage = self.storage.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let text = std::fs::read_to_string(&path)
                                    .map_err(|err| err.to_string())?;
                                let parsed =
                                    ics::parse_import(&text, CalendarKey::Local(calendar_id))
                                        .map_err(|err| err.to_string())?;
                                let mut storage = storage.lock().unwrap();
                                if storage.get_calendar(calendar_id).is_none() {
                                    return Err("destination calendar no longer exists".to_string());
                                }
                                let mut count = 0;
                                let mut skipped = parsed.skipped;
                                let mut resolved_series = HashMap::new();
                                for mut event in parsed.events {
                                    if let Some(series) = &mut event.series_id {
                                        if let Some(existing) = resolved_series.get(series) {
                                            *series = *existing;
                                        }
                                    }
                                    let duplicate =
                                        storage.all_events().into_iter().find(|existing| {
                                            existing.calendar == event.calendar
                                                && existing.recurrence_id == event.recurrence_id
                                                && (existing.id == event.id
                                                    || event.ics_uid.as_deref().is_some_and(
                                                        |uid| {
                                                            existing.ics_uid.as_deref() == Some(uid)
                                                                || existing.id.0.to_string() == uid
                                                        },
                                                    ))
                                        });
                                    if let Some(duplicate) = duplicate {
                                        if event.recurrence_id.is_none() {
                                            resolved_series.insert(event.id, duplicate.id);
                                        }
                                        skipped += 1;
                                        continue;
                                    }
                                    if storage.contains_event_id(event.id) {
                                        let original = event.id;
                                        event.id = EventId::new();
                                        resolved_series.insert(original, event.id);
                                    }
                                    storage.create_event(event).map_err(|err| err.to_string())?;
                                    count += 1;
                                }
                                Ok((count, skipped))
                            })
                            .await
                            .map_err(|err| err.to_string())?
                        },
                        |result| cosmic::action::app(Message::IcsImported(result)),
                    ));
                }
            }
            Message::IcsImported(result) => {
                self.ics_import.busy = false;
                if self.context_page == ContextPage::IcsImport {
                    self.core.set_show_context(false);
                }
                let notice = match result {
                    Ok((imported, skipped)) => fl!(
                        "ics-import-result",
                        imported = imported.to_string(),
                        skipped = skipped.to_string()
                    ),
                    Err(error) => fl!("ics-import-error", error = error.as_str()),
                };
                tasks.push(self.toast(notice));
            }
            Message::ExportIcs => {
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        let dialog = file_chooser::save::Dialog::new()
                            .title(fl!("export-ics"))
                            .file_name("calendar.ics".to_string());
                        let path = match dialog.save_file().await {
                            Ok(response) => response.url().and_then(|url| url.to_file_path().ok()),
                            Err(_) => None,
                        };
                        let Some(path) = path else {
                            return Ok(None);
                        };

                        tokio::task::spawn_blocking(move || {
                            let storage = storage.lock().unwrap();
                            let events: Vec<Event> = storage.events_for_export().cloned().collect();
                            let calendar = ics::export_events(&events);
                            std::fs::write(&path, calendar)
                                .map(|()| Some(path))
                                .map_err(|err| err.to_string())
                        })
                        .await
                        .map_err(|err| err.to_string())?
                    },
                    |result| cosmic::action::app(Message::IcsExported(result)),
                ));
            }
            Message::IcsExported(result) => match result {
                Ok(Some(path)) => tracing::info!("exported calendar to {}", path.display()),
                Ok(None) => {}
                Err(err) => {
                    tasks.push(self.toast(fl!("calendar-export-error", error = err.as_str())))
                }
            },
            Message::OpenNewCalendarForm => {
                if self.calendar_form.as_ref().is_some_and(|form| form.writing) {
                    return Task::none();
                }
                self.calendar_form = Some(CalendarForm::new(None, String::new(), None));
            }
            Message::StartEditCalendar(id) => {
                if self.calendar_form.as_ref().is_some_and(|form| form.writing) {
                    return Task::none();
                }
                if let Some(meta) = self.storage.lock().unwrap().get_calendar(id) {
                    self.calendar_form =
                        Some(CalendarForm::new(Some(id), meta.name.clone(), meta.color.clone()));
                }
            }
            Message::CalendarNameChanged(name) => {
                if let Some(form) = self.calendar_form.as_mut().filter(|form| !form.writing) {
                    form.name = name;
                }
            }
            Message::SetCalendarColor(color) => {
                if let Some(form) = self.calendar_form.as_mut().filter(|form| !form.writing) {
                    form.picker = CalendarForm::color_picker(color.as_deref());
                    form.color = color;
                }
            }
            Message::CalendarColorPicker(update) => {
                if let Some(form) = self.calendar_form.as_mut().filter(|form| !form.writing) {
                    match update {
                        // Keep the picker open during slider and text input interactions.
                        ColorPickerUpdate::ActionFinished => {}
                        ColorPickerUpdate::Cancel => {
                            form.picker = CalendarForm::color_picker(form.color.as_deref());
                        }
                        ColorPickerUpdate::ToggleColorPicker if form.picker.get_is_active() => {
                            form.picker = CalendarForm::color_picker(form.color.as_deref());
                        }
                        update => {
                            use cosmic::cosmic_theme::palette::{FromColor, Srgb};
                            let color = match &update {
                                ColorPickerUpdate::ActiveColor(hsv) => {
                                    Some(cosmic::iced::Color::from(Srgb::from_color(*hsv)))
                                }
                                ColorPickerUpdate::Input(value) => {
                                    value.parse::<css_color::Srgb>().ok().map(|rgb| {
                                        cosmic::iced::Color::from_rgb(rgb.red, rgb.green, rgb.blue)
                                    })
                                }
                                _ => None,
                            };
                            if let Some(color) = color {
                                let [r, g, b, _] = color.into_rgba8();
                                form.color = Some(format!("#{r:02x}{g:02x}{b:02x}"));
                            }
                            tasks.push(
                                form.picker.update::<Message>(update).map(cosmic::action::app),
                            );
                        }
                    }
                }
            }
            Message::SaveCalendarForm => {
                if let Some(form) = self.calendar_form.as_mut().filter(|form| form.can_save()) {
                    form.writing = true;
                    let id = form.id;
                    let name = form.name.trim().to_owned();
                    let color = form.color.clone();
                    let storage = self.storage.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let mut storage = storage.lock().unwrap();
                                let mut meta = if let Some(id) = id {
                                    storage.get_calendar(id).cloned().ok_or("calendar not found")?
                                } else {
                                    LocalCalendarMeta {
                                        id: CalendarId::new(),
                                        name: String::new(),
                                        description: None,
                                        color: None,
                                        visible: true,
                                        archived: false,
                                    }
                                };
                                meta.name = name;
                                meta.color = color;
                                storage.update_calendar(meta).map_err(|err| err.to_string())
                            })
                            .await
                            .map_err(|err| err.to_string())?
                        },
                        |result| cosmic::action::app(Message::CalendarFormSaved(result)),
                    ));
                }
            }
            Message::CalendarFormSaved(result) => {
                if let Some(form) = self.calendar_form.as_mut() {
                    form.writing = false;
                    let editing = form.id.is_some();
                    match result {
                        Ok(()) => {
                            self.calendar_form = None;
                            self.refresh_calendar_nav();
                        }
                        Err(err) => {
                            let notice = if editing {
                                fl!("calendar-save-error", error = err.as_str())
                            } else {
                                fl!("calendar-create-error", error = err.as_str())
                            };
                            tasks.push(self.toast(notice));
                        }
                    }
                }
            }
            Message::CloseCalendarForm => {
                if self.calendar_form.as_ref().is_some_and(|form| !form.writing) {
                    self.calendar_form = None;
                }
            }
            Message::ToggleCalendarVisibility(id, visible) => {
                let storage = self.storage.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let mut storage = storage.lock().unwrap();
                            let Some(mut meta) = storage.get_calendar(id).cloned() else {
                                return Err("calendar not found".to_string());
                            };
                            meta.visible = visible;
                            storage.update_calendar(meta).map_err(|err| err.to_string())
                        })
                        .await
                        .map_err(|err| err.to_string())?
                    },
                    |result| cosmic::action::app(Message::CalendarVisibilityToggled(result)),
                ));
            }
            Message::CalendarVisibilityToggled(result) => match result {
                Ok(()) => self.refresh_calendar_nav(),
                Err(err) => {
                    tasks.push(self.toast(fl!("calendar-visibility-error", error = err.as_str())))
                }
            },
            Message::DeleteCalendar(id) => {
                if self.editing_event.is_none() && !self.calendar_archive_busy {
                    let storage = self.storage.lock().unwrap();
                    self.calendar_to_archive =
                        storage.get_calendar(id).map(|meta| (id, meta.name.clone()));
                    self.calendar_popup = None;
                }
            }
            Message::CancelArchiveCalendar => {
                if !self.calendar_archive_busy {
                    self.calendar_to_archive = None;
                }
            }
            Message::ConfirmArchiveCalendar | Message::RestoreCalendar(_) => {
                let archive = matches!(message, Message::ConfirmArchiveCalendar);
                let id = match message {
                    Message::RestoreCalendar(id) if self.calendar_to_archive.is_none() => Some(id),
                    Message::ConfirmArchiveCalendar => {
                        self.calendar_to_archive.as_ref().map(|(id, _)| *id)
                    }
                    _ => None,
                };
                if let Some(id) = id.filter(|_| !self.calendar_archive_busy) {
                    self.calendar_archive_busy = true;
                    let storage = self.storage.clone();
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let mut storage = storage.lock().unwrap();
                                if archive {
                                    storage.archive_calendar(id)
                                } else {
                                    storage.restore_calendar(id)
                                }
                            })
                            .await
                            .map_err(|err| err.to_string())?
                            .map_err(|err| err.to_string())
                        },
                        |result| cosmic::action::app(Message::CalendarArchiveChanged(result)),
                    ));
                }
            }
            Message::CalendarArchiveChanged(result) => {
                self.calendar_archive_busy = false;
                let archived = self.calendar_to_archive.take().is_some();
                match result {
                    Ok(()) => {
                        self.calendar.selected_event = None;
                        self.calendar_popup = None;
                        self.refresh_calendar_nav();
                        if archived {
                            tasks.push(self.toast(fl!("calendar-archived")));
                        }
                    }
                    Err(err) => {
                        tasks.push(self.toast(fl!("calendar-archive-error", error = err.as_str())))
                    }
                }
            }
            Message::CloseToast(id) => self.toasts.remove(id),
            Message::SearchChanged(query) => {
                let returning_to_calendar = !self.search_query.is_empty() && query.is_empty();
                self.search_query = query;
                if returning_to_calendar {
                    tasks.push(self.scroll_calendar_to_now());
                }
                if self.context_page == ContextPage::DayAgenda && !self.search_query.is_empty() {
                    self.core.set_show_context(false);
                }
            }
            Message::SelectSearchResult(id, start) => {
                self.search_query.clear();
                self.calendar.set_date(start);
                self.calendar.selected_event = Some((id, start));
                self.open_event_details((id, start), None);
            }
            Message::EventEditorReminderToggled(entity) => {
                let Some(minutes) = self.reminders_model.data::<i64>(entity).copied() else {
                    return Task::batch(tasks);
                };
                let now_on = !self.reminders_model.is_active(entity);
                if let Some(state) = self.editing_event.as_mut() {
                    if now_on {
                        if !state.reminders.contains(&minutes) {
                            state.reminders.push(minutes);
                        }
                    } else {
                        state.reminders.retain(|m| *m != minutes);
                    }
                    // Rebuilt rather than toggled in place: a custom (non-preset)
                    // entry has no "off" state to fall back to, so turning it off
                    // must drop it from the model entirely, not just deactivate it.
                    self.reminders_model = build_reminders_model(&state.reminders);
                }
            }
            Message::EventEditorCustomReminderInputChanged(value) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.custom_reminder_input = value;
                }
            }
            Message::EventEditorCustomReminderEditToggled(editing) => {
                if let Some(state) = self.editing_event.as_mut() {
                    state.custom_reminder_editing = editing;
                }
            }
            Message::EventEditorAddCustomReminder => {
                if let Some(state) = self.editing_event.as_mut() {
                    if let Ok(minutes) = state.custom_reminder_input.trim().parse::<i64>() {
                        if minutes >= 0 && !state.reminders.contains(&minutes) {
                            state.reminders.push(minutes);
                            state.custom_reminder_input.clear();
                            state.custom_reminder_editing = false;
                            self.reminders_model = build_reminders_model(&state.reminders);
                        }
                    }
                }
            }
            Message::CheckReminders => {
                self.refresh_display_timezone();
                if self.checking_reminders {
                    return Task::none();
                }
                self.checking_reminders = true;
                let oldest = OffsetDateTime::now_utc() - time::Duration::days(1);
                self.fired_reminders
                    .retain(|(_, occurrence, _)| *occurrence >= oldest);
                let storage = self.storage.clone();
                let already_fired = self.fired_reminders.clone();
                tasks.push(Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            check_reminders(&storage, &already_fired)
                        })
                        .await
                        .unwrap_or_default()
                    },
                    |fired| cosmic::action::app(Message::RemindersFired(fired)),
                ));
            }
            Message::RemindersFired(fired) => {
                self.checking_reminders = false;
                self.fired_reminders.extend(fired);
            }
            Message::ShowDayAgenda(date) => {
                self.calendar
                    .set_date(self.calendar.selected_date.replace_date(date));
                self.open_day_agenda();
            }
            Message::CloseDayAgenda => {
                if self.context_page == ContextPage::DayAgenda && self.core.window.show_context {
                    self.core.set_show_context(false);
                    tasks.push(widget::button::focus(
                        crate::features::calendar::month::day_id(
                            self.calendar.selected_date.date(),
                        ),
                    ));
                }
            }
            Message::SelectDate(date) => {
                self.calendar.set_date(date);
            }
            Message::SelectDay(idx) => {
                let days = self.calendar.days();
                let day = days.index(idx);
                if let Ok(date) = self.calendar.selected_date.replace_day(*day) {
                    self.calendar.set_date(date);
                } else {
                    tracing::error!("failed to select date");
                }
            }
            Message::SelectMonth(idx) => {
                let months = self.calendar.months();
                let month = months.index(idx);
                if let Err(err) = self
                    .calendar
                    .select_month_of_year(self.calendar.selected_date.year(), *month)
                {
                    tracing::error!("failed to select month: {err}");
                }
            }
            Message::SelectYear(idx) => {
                let years = self.calendar.years();
                let year = years.index(idx);
                if let Err(err) = self
                    .calendar
                    .select_month_of_year(*year, self.calendar.selected_date.month())
                {
                    tracing::error!("failed to select year: {err}");
                }
            }
            Message::NavigateToday => {
                self.calendar.set_today();
                tasks.push(self.scroll_calendar_to_now());
            }
            Message::NavigateNextDay => {
                if let Err(err) = self.calendar.next_day() {
                    tracing::error!("failed to navigate to next day: {err}");
                }
            }
            Message::NavigatePreviousDay => {
                if let Err(err) = self.calendar.previous_day() {
                    tracing::error!("failed to navigate to previous day: {err}");
                }
            }
            Message::NavigateNextMonth => {
                if let Err(err) = self.calendar.next_month() {
                    tracing::error!("failed to navigate to next month: {err}");
                }
            }
            Message::NavigatePreviousMonth => {
                if let Err(err) = self.calendar.previous_month() {
                    tracing::error!("failed to navigate to previous month: {err}");
                }
            }
            Message::NavigateNextYear => {
                if let Err(err) = self.calendar.next_year() {
                    tracing::error!("failed to navigate to next year: {err}");
                }
            }
            Message::NavigatePreviousYear => {
                if let Err(err) = self.calendar.previous_year() {
                    tracing::error!("failed to navigate to previous year: {err}");
                }
            }
        }
        self.update_drawer_layout();
        Task::batch(tasks)
    }

    fn system_theme_update(
        &mut self,
        _keys: &[&'static str],
        _new_theme: &cosmic_theme::Theme,
    ) -> Task<cosmic::Action<Message>> {
        cosmic::command::set_theme(self.config.app_theme.theme())
    }

    fn on_window_resize(&mut self, id: cosmic::iced::window::Id, width: f32, _height: f32) {
        if self.core.main_window_is(id) {
            self.window_width = width;
            self.update_drawer_layout();
        }
    }

    fn on_escape(&mut self) -> Task<cosmic::Action<Self::Message>> {
        if self.calendar_to_archive.is_some() {
            return cosmic::task::message(Message::CancelArchiveCalendar);
        }
        if let Some(form) = &self.calendar_form {
            let message = if form.picker.get_is_active() {
                Message::CalendarColorPicker(ColorPickerUpdate::Cancel)
            } else {
                Message::CloseCalendarForm
            };
            return cosmic::task::message(message);
        }
        if self.calendar_popup.take().is_some() {
            return Task::none();
        }
        if !self.core.window.show_context || self.editing_event.is_some() {
            return Task::none();
        }
        match self.context_page {
            ContextPage::DayAgenda => cosmic::task::message(Message::CloseDayAgenda),
            ContextPage::Settings => {
                cosmic::task::message(Message::ToggleContextPage(ContextPage::Settings))
            }
            _ => Task::none(),
        }
    }

    /// Called when a nav item is selected.
    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<cosmic::Action<Self::Message>> {
        // Clicking a calendar entry toggles its visibility rather than
        // navigating to a page.
        if let Some(calendar_id) = self.nav.data::<CalendarId>(id).copied() {
            let visible = self
                .storage
                .lock()
                .unwrap()
                .get_calendar(calendar_id)
                .map(|meta| !meta.visible)
                .unwrap_or(true);
            return cosmic::task::message(Message::ToggleCalendarVisibility(calendar_id, visible));
        }

        if self.calendar_nav_ids.first() == Some(&id) {
            self.calendars_collapsed = !self.calendars_collapsed;
            self.refresh_calendar_nav();
        }
        Task::none()
    }
}

impl AppModel {
    fn toast(&mut self, message: String) -> Task<cosmic::Action<Message>> {
        self.toasts
            .push(widget::Toast::new(message))
            .map(cosmic::action::app)
    }

    fn create_event_at_slot(&self, slot: CalendarSlot) -> Message {
        Message::OpenEventEditor(
            None,
            slot.start(self.calendar.selected_date.offset()),
            matches!(slot, CalendarSlot::AllDay(_)),
        )
    }

    fn scroll_calendar_to_now(&self) -> Task<cosmic::Action<Message>> {
        if self.search_query.is_empty() {
            match self.tabs.active_data::<Tab>() {
                Some(Tab::Week) => return self.calendar.scroll_to_now(true),
                Some(Tab::Day) => return self.calendar.scroll_to_now(false),
                _ => {}
            }
        }
        Task::none()
    }

    fn calendar_double_click(&mut self, target: CalendarClick) -> bool {
        let now = std::time::Instant::now();
        let double_click = self.calendar_click.is_some_and(|(previous, at)| {
            target == previous && now.duration_since(at) <= std::time::Duration::from_millis(400)
        });
        self.calendar_click = (!double_click).then_some((target, now));
        double_click
    }

    fn open_event_details(&mut self, key: EventSelection, anchor: Option<CalendarAnchor>) {
        let storage = self.storage.lock().unwrap();
        self.calendar_popup = storage.get_event(key.0).map(|event| {
            let calendar = match event.calendar {
                CalendarKey::Local(id) => storage
                    .get_calendar(id)
                    .map(|calendar| calendar.name.clone()),
                CalendarKey::Remote { .. } => None,
            };
            OpenCalendarPopup {
                content: CalendarPopup::Details(key, Box::new(event.clone()), calendar),
                anchor,
            }
        });
    }

    pub(crate) fn attach_calendar_popup<'a>(
        &'a self,
        content: impl Into<Element<'a, Message>>,
        anchor: Option<CalendarAnchor>,
    ) -> Element<'a, Message> {
        let Some(popup) = self
            .calendar_popup
            .as_ref()
            .filter(|popup| popup.anchor == anchor)
        else {
            return content.into();
        };
        let position = if anchor.is_some() {
            widget::popover::Position::Bottom
        } else {
            widget::popover::Position::Center
        };
        widget::popover::popover(content)
            .position(position)
            .popup(self.calendar_popup_view(&popup.content))
            .on_close(Message::CloseCalendarPopup)
            .into()
    }

    pub(crate) fn calendar_can_paste(&self) -> bool {
        self.calendar_clipboard.is_some()
    }

    pub(crate) fn compact_month_anchor(
        &self,
        date: time::Date,
        compact: bool,
    ) -> Option<CalendarAnchor> {
        if compact {
            if let Some(CalendarAnchor::Month(day, key)) =
                self.calendar_popup.as_ref().and_then(|popup| popup.anchor)
            {
                if date == day {
                    return Some(CalendarAnchor::Month(date, key));
                }
            }
        }
        None
    }

    fn calendar_popup_view(&self, popup: &CalendarPopup) -> Element<'_, Message> {
        let mut content = widget::column(Vec::new()).spacing(spacing().space_xxs);
        match popup {
            CalendarPopup::Details(key, event, calendar) => {
                let start = key.1;
                let when = match event.time {
                    EventTime::AllDay {
                        start: original,
                        end,
                    } => {
                        let first = start.date();
                        let last = first
                            .saturating_add((end - original).saturating_sub(time::Duration::DAY));
                        if last > first {
                            format!("{first} – {last} · {}", fl!("all-day"))
                        } else {
                            format!("{first} · {}", fl!("all-day"))
                        }
                    }
                    EventTime::Timed {
                        start: original,
                        end,
                    } => {
                        let display = |instant| {
                            if event.floating {
                                instant
                            } else {
                                crate::features::calendar::display_time(
                                    instant,
                                    self.calendar.display_timezone,
                                )
                            }
                        };
                        let first = display(start);
                        let last = display(start.saturating_add(end - original));
                        let clock = |date: OffsetDateTime| {
                            format!("{:02}:{:02}", date.hour(), date.minute())
                        };
                        if first == last {
                            format!("{} · {}", first.date(), clock(first))
                        } else if first.date() == last.date() {
                            format!("{} · {}–{}", first.date(), clock(first), clock(last))
                        } else {
                            format!(
                                "{} {} – {} {}",
                                first.date(),
                                clock(first),
                                last.date(),
                                clock(last)
                            )
                        }
                    }
                };
                content = content
                    .push(widget::text::title4(event.summary.clone()))
                    .push(widget::text::body(when));
                for value in [&event.location, &event.description].into_iter().flatten() {
                    content = content.push(widget::text::body(value.clone()));
                }
                if let Some(calendar) = calendar {
                    content = content.push(widget::text::caption(calendar.clone()));
                }
                content = content.push(widget::button::suggested(fl!("edit-event")).on_press(
                    Message::CalendarAction(CalendarAction::Edit, Some(*key), start),
                ));
            }
        }
        widget::container(content)
            .width(Length::Fixed(240.0))
            .padding(spacing().space_s)
            .class(cosmic::theme::Container::Dialog(false))
            .into()
    }

    fn paste_occurrence(
        &self,
        source: CalendarClipboard,
        date: OffsetDateTime,
    ) -> crate::Result<EventSelection> {
        use chrono::TimeZone;
        let mut storage = self.storage.lock().unwrap();
        let (mut event, cut) = match source {
            CalendarClipboard::Copy(event) => (event, None),
            CalendarClipboard::Cut(key) => {
                let event = storage
                    .get_event(key.0)
                    .cloned()
                    .ok_or_else(|| crate::Error::Storage("event no longer exists".into()))?;
                if event.is_recurring()
                    && event
                        .occurrences(key.1..key.1.saturating_add(time::Duration::SECOND))
                        .first()
                        != Some(&key.1)
                {
                    return Err(crate::Error::Storage("occurrence no longer exists".into()));
                }
                (event, Some(key))
            }
        };
        let recurring = event.is_recurring();
        let wall = date.replace_offset(time::UtcOffset::UTC);
        let start = match event.time {
            EventTime::AllDay { start, end } => {
                let date = date.date();
                event.time = EventTime::AllDay {
                    start: date,
                    end: date.saturating_add(end - start),
                };
                date.midnight().assume_utc()
            }
            EventTime::Timed { start, end } => {
                let destination = if event.floating {
                    wall
                } else {
                    let naive =
                        chrono::DateTime::from_timestamp(wall.unix_timestamp(), wall.nanosecond())
                            .ok_or_else(|| {
                                crate::Error::Storage("invalid destination date".into())
                            })?
                            .naive_utc();
                    let offset = match self.calendar.display_timezone {
                        Some(zone) => zone.from_local_datetime(&naive).earliest().map(|date| {
                            use chrono::Offset;
                            date.offset().fix().local_minus_utc()
                        }),
                        None => chrono::Local
                            .from_local_datetime(&naive)
                            .earliest()
                            .map(|date| date.offset().local_minus_utc()),
                    }
                    .and_then(|offset| time::UtcOffset::from_whole_seconds(offset).ok())
                    .ok_or_else(|| crate::Error::Storage("invalid destination time".into()))?;
                    wall.replace_offset(offset)
                };
                event.time = EventTime::Timed {
                    start: destination,
                    end: destination.saturating_add(end - start),
                };
                destination
            }
        };
        let now = OffsetDateTime::now_utc();
        event.updated_at = now;
        if cut.is_none() || recurring {
            event.id = EventId::new();
            event.created_at = now;
            event.sequence = 0;
            event.rrule = None;
            event.exdates.clear();
            event.rdates.clear();
            event.repeat_before = None;
            event.split_before = None;
            event.predecessor = None;
            event.series_id = cut.map(|key| key.0);
            event.recurrence_id = cut.map(|key| key.1);
            if cut.is_none() {
                event.ics_uid = None;
                event.external = None;
            }
        } else {
            event.sequence = event.sequence.saturating_add(1);
        }
        let key = (event.id, start);
        storage.create_event(event)?;
        Ok(key)
    }

    fn begin_event_deletion(&mut self, id: EventId) -> bool {
        let Some(state) = self
            .editing_event
            .as_mut()
            .filter(|state| !state.writing && state.event_id == Some(id))
        else {
            return false;
        };
        state.writing = true;
        true
    }

    fn open_day_agenda(&mut self) {
        self.context_page = ContextPage::DayAgenda;
        self.update_drawer_layout();
        self.core.set_show_context(true);
    }

    fn update_drawer_layout(&mut self) {
        // Below this width the native drawer would squeeze the seven day columns.
        let overlay = self.context_page != ContextPage::DayAgenda
            || self.window_width / self.core.scale_factor() < 1_000.0;
        if self.core.window.context_is_overlay != overlay {
            self.core.window.context_is_overlay = overlay;
            self.core.set_show_context(self.core.window.show_context);
        }
    }

    fn show_month(&self) -> bool {
        matches!(self.tabs.active_data::<Tab>(), Some(Tab::Month)) && self.search_query.is_empty()
    }

    fn ics_import_view(&self) -> Element<'_, Message> {
        if self.ics_import.busy {
            return widget::text::body(fl!("ics-import-working")).into();
        }
        let storage = self.storage.lock().unwrap();
        let mut calendars = storage.list_calendars();
        calendars.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
        if calendars.is_empty() {
            return widget::text::body(fl!("ics-import-no-calendar")).into();
        }
        let selected = calendars
            .iter()
            .position(|calendar| Some(calendar.id) == self.ics_import.calendar);
        let labels = calendars
            .iter()
            .map(|calendar| calendar.name.clone())
            .collect::<Vec<_>>();
        let ids = calendars
            .iter()
            .map(|calendar| calendar.id)
            .collect::<Vec<_>>();
        let file = self
            .ics_import
            .path
            .as_deref()
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        widget::column(Vec::new())
            .spacing(spacing().space_s)
            .push(widget::text::body(file))
            .push(widget::settings::item(
                fl!("ics-import-destination"),
                widget::dropdown(labels, selected, move |index| {
                    Message::SelectImportCalendar(ids[index])
                }),
            ))
            .push(
                widget::button::suggested(fl!("ics-import-confirm"))
                    .on_press_maybe(selected.map(|_| Message::ConfirmIcsImport)),
            )
            .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let selected_zone = self.display_timezone_options[1..]
            .iter()
            .position(|zone| zone == &self.config.display_timezone)
            .map_or(0, |index| index + 1);
        let zone_dropdown = widget::dropdown(
            self.display_timezone_options.as_slice(),
            Some(selected_zone),
            Message::SetDisplayTimezone,
        );
        let section = widget::settings::section()
            .title(fl!("settings-date-and-time"))
            .add(
                widget::settings::item::builder(fl!("display-timezone"))
                    .control(widget::container(zone_dropdown).max_width(210)),
            );

        let appearance = widget::settings::section().title(fl!("appearance")).add(
            widget::settings::item::builder(fl!("theme")).control(widget::dropdown(
                vec![
                    fl!("theme-match-desktop"),
                    fl!("theme-dark"),
                    fl!("theme-light"),
                ],
                AppTheme::OPTIONS
                    .iter()
                    .position(|&theme| theme == self.config.app_theme),
                |index| Message::SetTheme(AppTheme::OPTIONS[index]),
            )),
        );
        widget::settings::view_column(vec![appearance.into(), section.into()]).into()
    }

    fn refresh_display_timezone(&mut self) {
        self.calendar.display_timezone = self
            .config
            .display_timezone
            .parse()
            .ok()
            .or_else(|| iana_time_zone::get_timezone().ok()?.parse().ok());
        self.calendar.current_date = crate::features::calendar::display_time(
            OffsetDateTime::now_utc(),
            self.calendar.display_timezone,
        );
    }

    /// The about page for this app.
    pub fn about<'a>(&'a self) -> Element<'a, Message> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        let icon = widget::svg(widget::svg::Handle::from_memory(APP_ICON));

        let title = widget::text::title3(fl!("app-title"));

        let hash = env!("VERGEN_GIT_SHA");
        let short_hash: String = hash.chars().take(7).collect();
        let date = env!("VERGEN_GIT_COMMIT_DATE");

        let link = widget::button::link(REPOSITORY)
            .on_press(Message::OpenRepositoryUrl)
            .padding(0);

        widget::column(Vec::new())
            .push(icon)
            .push(title)
            .push(link)
            .push(
                widget::button::link(fl!(
                    "git-description",
                    hash = short_hash.as_str(),
                    date = date
                ))
                .on_press(Message::LaunchUrl(format!("{REPOSITORY}/commits/{hash}")))
                .padding(0),
            )
            .align_x(Alignment::Center)
            .spacing(space_xxs)
            .into()
    }

    /// The event creation/editing form for the context drawer.
    pub fn event_editor_view<'a>(&'a self) -> Element<'a, Message> {
        match &self.editing_event {
            Some(state) => {
                let storage = self.storage.lock().unwrap();
                let mut calendars = storage.list_calendars();
                calendars.sort_by(|a, b| a.name.cmp(&b.name));
                crate::features::event::view::view(state, &calendars, &self.reminders_model)
            }
            None => widget::text::body("").into(),
        }
    }

    /// Tears down and rebuilds the "Calendars" section of the nav bar from
    /// current storage state. `segmented_button::Model` has no in-place
    /// update API, so every calendar CRUD change calls this to resync.
    pub fn refresh_calendar_nav(&mut self) {
        for id in self.calendar_nav_ids.drain(..) {
            self.nav.remove(id);
        }

        let calendars = {
            let storage = self.storage.lock().unwrap();
            let mut metas = storage.list_calendars();
            metas.sort_by(|a, b| a.name.cmp(&b.name));
            metas
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        let mut new_ids = Vec::with_capacity(calendars.len() + 1);

        self.nav
            .insert()
            .text(fl!("calendars"))
            .icon(widget::icon::from_name(if self.calendars_collapsed {
                "pan-end-symbolic"
            } else {
                "pan-down-symbolic"
            }))
            .with_id(|id| new_ids.push(id));

        if self.calendars_collapsed {
            self.calendar_nav_ids = new_ids;
            return;
        }

        for calendar in calendars {
            let color = calendar.color.as_deref().and_then(calendar_color);
            let icon = calendar_form::sidebar_color_icon(color, calendar.visible);
            self.nav
                .insert()
                .indent(1)
                .text(calendar.name)
                .icon(icon)
                .data(calendar.id)
                .with_id(|entity| new_ids.push(entity));
        }

        self.calendar_nav_ids = new_ids;
    }

    fn archived_calendars_view(&self) -> Element<'_, Message> {
        let storage = self.storage.lock().unwrap();
        let mut calendars = storage.archived_calendars();
        calendars.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
        if calendars.is_empty() {
            return widget::text::body(fl!("no-archived-calendars")).into();
        }
        let mut content = widget::column(Vec::new()).spacing(spacing().space_s);
        for calendar in calendars {
            content = content.push(
                widget::row(Vec::new())
                    .spacing(spacing().space_s)
                    .push(widget::text::body(calendar.name.clone()).width(Length::Fill))
                    .push(
                        widget::button::standard(fl!("restore-calendar")).on_press_maybe(
                            (!self.calendar_archive_busy && !self.storage_read_only)
                                .then_some(Message::RestoreCalendar(calendar.id)),
                        ),
                    ),
            );
        }
        widget::scrollable(content).into()
    }

    /// The list of events matching `self.search_query`, shown in place of
    /// the month/week/day grids while a search is active.
    fn search_results_view<'a>(&'a self) -> Element<'a, Message> {
        let storage = self.storage.lock().unwrap();
        let results = storage.search(&self.search_query);

        if results.is_empty() {
            return widget::text::body(fl!("no-results"))
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .into();
        }

        let mut list = widget::column(Vec::new())
            .spacing(spacing().space_xs)
            .width(Length::Fill);

        for event in results {
            let id = event.id;
            let start = event.start();
            let label = format!("{} — {}", start.date(), event.summary);
            list = list.push(
                widget::button::text(label)
                    .width(Length::Fill)
                    .class(cosmic::style::Button::Standard)
                    .on_press(Message::SelectSearchResult(id, start)),
            );
        }

        widget::scrollable(list)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Updates the header and window titles.
    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let mut window_title = fl!("app-title");

        if let Some(page) = self.nav.text(self.nav.active()) {
            window_title.push_str(" — ");
            window_title.push_str(page);
        }

        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(window_title, id)
        } else {
            Task::none()
        }
    }
}

/// The tab to display in the application.
pub enum Tab {
    Month,
    Week,
    Day,
}

/// The context page to display in the context drawer.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
    ArchivedCalendars,
    DayAgenda,
    Settings,
    IcsImport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    NewCalendar,
    About,
    Settings,
    ImportIcs,
    ExportIcs,
    ArchivedCalendars,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::NewCalendar => Message::OpenNewCalendarForm,
            MenuAction::Settings => Message::ToggleContextPage(ContextPage::Settings),
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
            MenuAction::ImportIcs => Message::ImportIcs,
            MenuAction::ExportIcs => Message::ExportIcs,
            MenuAction::ArchivedCalendars => {
                Message::ToggleContextPage(ContextPage::ArchivedCalendars)
            }
        }
    }
}

/// Actions offered by the nav bar's per-item context menu (right-click),
/// covering calendar creation/rename/delete now that calendar management is
/// merged into the nav bar instead of a separate list drawer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavMenuAction {
    NewCalendar,
    RenameCalendar(CalendarId),
    DeleteCalendar(CalendarId),
}

impl menu::action::MenuAction for NavMenuAction {
    type Message = cosmic::Action<Message>;

    fn message(&self) -> Self::Message {
        match self {
            NavMenuAction::NewCalendar => cosmic::Action::App(Message::OpenNewCalendarForm),
            NavMenuAction::RenameCalendar(id) => {
                cosmic::Action::App(Message::StartEditCalendar(*id))
            }
            NavMenuAction::DeleteCalendar(id) => cosmic::Action::App(Message::DeleteCalendar(*id)),
        }
    }
}
