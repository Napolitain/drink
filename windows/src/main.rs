//! Drink for Windows: a notification-area water-break reminder.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use drink_windows::{app, flavor::Flavor, instance::Instance};
    use std::time::Duration;

    let flavor = Flavor::compiled();
    // A second launch of the same build leaves the running one in place.
    let Some(_instance) = Instance::acquire(flavor.identifier, Duration::ZERO) else {
        return std::process::ExitCode::SUCCESS;
    };
    std::process::ExitCode::from(app::run(flavor))
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("Drink for Windows runs only on Windows.");
    std::process::ExitCode::FAILURE
}
