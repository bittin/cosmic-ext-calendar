#[derive(Debug, Default)]
pub struct Flags {
    pub keep_running: bool,
}

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
}

pub fn flags() -> Flags {
    Flags {
        keep_running: cfg!(target_os = "linux") && std::path::Path::new("/.flatpak-info").is_file(),
    }
}
