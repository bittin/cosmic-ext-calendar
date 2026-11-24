use std::path::PathBuf;

use crate::shared::ids::CalendarId;

#[derive(Debug, Clone)]
pub enum IcsMessage {
    ImportIcs,
    IcsFileChosen(Option<PathBuf>),
    SelectImportCalendar(CalendarId),
    ConfirmIcsImport,
    IcsImported(std::result::Result<(usize, usize), String>),
    ExportIcs,
    IcsExported(std::result::Result<Option<PathBuf>, String>),
}
