use cosmic::widget::segmented_button;
use time::OffsetDateTime;

use super::{CalendarAnchor, CalendarSlot, EventSelection};
use crate::shared::ids::EventId;

#[derive(Debug, Clone)]
pub enum CalendarMessage {
    TabSelected(segmented_button::Entity),
    NavigateNextDay,
    NavigatePreviousDay,
    NavigateNextMonth,
    NavigatePreviousMonth,
    NavigateNextYear,
    NavigatePreviousYear,
    NavigateToday,
    SelectDate(OffsetDateTime),
    ShowDayAgenda(time::Date),
    CloseDayAgenda,
    SelectMonth(usize),
    SelectYear(usize),
    SelectDay(usize),
    CalendarEventClicked(EventSelection, CalendarAnchor),
    CalendarEmptySlotClicked(CalendarSlot),
    CloseCalendarPopup,
    EventAction(CalendarAction, Option<EventSelection>, OffsetDateTime),
    SearchChanged(String),
    SelectSearchResult(EventId, OffsetDateTime),
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
