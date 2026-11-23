use cosmic::app::Settings;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::i18n;

pub fn init_runtime() {
    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Initialize tracing subscriber.
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "cosmic_ext_calendar=info");
        }
    }
    tracing_subscriber::registry()
        .with(EnvFilter::from_env("RUST_LOG"))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

pub const MIN_WINDOW_SIZE: cosmic::iced::Size = cosmic::iced::Size::new(360.0, 180.0);

pub fn settings(keep_running: bool) -> Settings {
    // Settings for configuring the application window and iced runtime.
    Settings::default()
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(MIN_WINDOW_SIZE.width)
                .min_height(MIN_WINDOW_SIZE.height),
        )
        .exit_on_close(!keep_running)
        .no_main_window(keep_running)
}
