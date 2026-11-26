//! Native Windows 11 adapter for Drink.
//!
//! The portable modules hold the desktop policy, menu text, icon artwork and
//! release naming, so they are tested on every platform. The Win32 modules own
//! windows, the notification area, Core Audio sessions and the updater.

pub mod cli;
pub mod controller;
pub mod flavor;
pub mod icon;
pub mod menu;
pub mod release;

#[cfg(windows)]
pub mod app;
#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod foreground;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod overlay;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
pub mod update;
#[cfg(windows)]
mod win;

/// Print a line to the console that started this windowed executable, or to
/// redirected output.
#[cfg(windows)]
pub fn print_to_console(message: &str, error: bool) {
    use std::io::Write;
    win::attach_parent_console();
    let line = format!("{message}\n");
    let _ = if error {
        std::io::stderr().write_all(line.as_bytes())
    } else {
        std::io::stdout().write_all(line.as_bytes())
    };
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn console_output_tolerates_an_existing_console() {
        super::print_to_console("console check (stdout)", false);
        super::print_to_console("console check (stderr)", true);
    }
}
