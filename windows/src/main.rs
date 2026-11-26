//! Drink for Windows: a notification-area water-break reminder.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use drink_windows::cli::{Mode, parse};
    use drink_windows::{flavor::Flavor, instance, print_to_console};

    let flavor = Flavor::compiled();
    let code = match parse(std::env::args_os().skip(1)) {
        Mode::Run => run(flavor, false),
        Mode::Relaunch => run(flavor, true),
        Mode::CheckForUpdates => check_for_updates(flavor),
        Mode::RestoreRelease {
            session,
            lock,
            release,
        } => instance::restore_release(session, lock, &release),
        Mode::Usage => {
            print_to_console("Usage: Drink.exe [--check-for-updates]", true);
            2
        }
    };
    std::process::ExitCode::from(code)
}

#[cfg(windows)]
fn run(flavor: drink_windows::flavor::Flavor, relaunch: bool) -> u8 {
    use drink_windows::{app, instance, print_to_console, update};
    use std::time::Duration;

    if flavor.release.is_none() {
        return match instance::begin_development_session() {
            Ok(_session) => app::run(flavor),
            Err(message) => {
                print_to_console(&message, true);
                1
            }
        };
    }
    // After an update, the previous instance needs a moment to exit.
    let wait = if relaunch {
        Duration::from_secs(10)
    } else {
        Duration::ZERO
    };
    let Some(_instance) = instance::Instance::acquire(flavor.identifier, wait) else {
        return 0;
    };
    instance::close_development_sessions();
    if let Ok(exe) = std::env::current_exe() {
        update::clean(&exe);
    }
    app::run(flavor)
}

#[cfg(windows)]
fn check_for_updates(flavor: drink_windows::flavor::Flavor) -> u8 {
    use drink_windows::menu::Outcome;
    use drink_windows::{print_to_console, update};

    let Some(version) = flavor.release else {
        print_to_console("Updates apply only to release builds.", true);
        return 1;
    };
    let outcome = std::env::current_exe().map_or(Outcome::Failed, |exe| {
        update::install_latest(&update::Install::running(&exe, version), &update::Curl)
    });
    let (message, succeeded) = outcome.report();
    print_to_console(message, !succeeded);
    u8::from(!succeeded)
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("Drink for Windows runs only on Windows.");
    std::process::ExitCode::FAILURE
}
