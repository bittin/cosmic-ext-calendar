// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod features;
mod shared;

pub use shared::error::{Error, Result};

fn main() -> cosmic::iced::Result {
    app::init_runtime();
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--notification-worker"))
    {
        if let Err(err) = features::event::notifications::run_worker() {
            tracing::error!("could not start event alert worker: {err}");
            std::process::exit(1);
        }
        return Ok(());
    }

    cosmic::app::run::<app::AppModel>(app::settings(), app::flags())
}
