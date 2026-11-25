use cosmic::widget::color_picker::ColorPickerUpdate;

use crate::shared::ids::CalendarId;

#[derive(Debug, Clone)]
pub enum CalendarManagementMessage {
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
}
