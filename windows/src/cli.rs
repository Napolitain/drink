//! Command-line modes of the Drink executable.

use std::ffi::OsString;

/// Relaunch after an update; waits for the previous instance to exit.
pub const RELAUNCH: &str = "--relaunch";
/// Install the latest release and exit.
pub const CHECK_FOR_UPDATES: &str = "--check-for-updates";

/// What the executable was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Run the reminder.
    Run,
    /// Run the reminder once the instance being replaced has exited.
    Relaunch,
    /// Install the latest release and exit.
    CheckForUpdates,
    /// Unrecognised arguments.
    Usage,
}

/// Parse the arguments after the program name.
#[must_use]
pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Mode {
    let args: Vec<OsString> = args.into_iter().collect();
    let flag = |index: usize| args.get(index).and_then(|arg| arg.to_str());
    match (flag(0), args.len()) {
        (None, 0) => Mode::Run,
        (Some(RELAUNCH), 1) => Mode::Relaunch,
        (Some(CHECK_FOR_UPDATES), 1) => Mode::CheckForUpdates,
        _ => Mode::Usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Mode {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn recognises_each_mode() {
        assert_eq!(parse_strs(&[]), Mode::Run);
        assert_eq!(parse_strs(&[RELAUNCH]), Mode::Relaunch);
        assert_eq!(parse_strs(&[CHECK_FOR_UPDATES]), Mode::CheckForUpdates);
    }

    #[test]
    fn rejects_unknown_or_malformed_arguments() {
        for args in [
            &["--dev"][..],
            &[RELAUNCH, "extra"],
            &[CHECK_FOR_UPDATES, CHECK_FOR_UPDATES],
        ] {
            assert_eq!(parse_strs(args), Mode::Usage, "{args:?}");
        }
    }
}
