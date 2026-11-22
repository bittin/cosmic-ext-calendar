use cosmic::{iced::Length, widget::ColorPickerModel};

use super::model::calendar_color;
use crate::shared::ids::CalendarId;

pub(super) const COLOR_PICKER_WIDTH: f32 = 224.0;

pub struct CalendarForm {
    pub id: Option<CalendarId>,
    pub name: String,
    pub color: Option<String>,
    pub picker: ColorPickerModel,
    pub writing: bool,
}

impl CalendarForm {
    pub fn new(id: Option<CalendarId>, name: String, color: Option<String>) -> Self {
        let picker = Self::color_picker(color.as_deref());
        Self {
            id,
            name,
            color,
            picker,
            writing: false,
        }
    }

    pub fn color_picker(color: Option<&str>) -> ColorPickerModel {
        ColorPickerModel::new("Hex", "RGB", None, color.and_then(calendar_color))
            .width(Length::Fixed(COLOR_PICKER_WIDTH))
            .height(Length::Fixed(128.0))
    }

    pub fn can_save(&self) -> bool {
        !self.writing && !self.name.trim().is_empty()
    }
}
