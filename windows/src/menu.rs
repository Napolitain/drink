//! Notification-area menu contents and the updater's menu state.

use crate::controller::Status;

/// Actions the menu can send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Command {
    /// Show a break now.
    TakeBreak = 1,
    /// Pause for an hour or resume.
    TogglePause = 2,
    /// Quit Drink.
    Quit = 3,
    /// Check for, or finish installing, a release update.
    Updates = 4,
}

impl Command {
    /// The command for a menu identifier, if any.
    #[must_use]
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::TakeBreak),
            2 => Some(Self::TogglePause),
            3 => Some(Self::Quit),
            4 => Some(Self::Updates),
            _ => None,
        }
    }
}

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Disabled informational text.
    Label(String),
    /// A clickable item, possibly disabled.
    Action {
        /// What the item does.
        command: Command,
        /// Item text.
        text: String,
        /// Whether the item can be chosen.
        enabled: bool,
    },
    /// A divider.
    Separator,
}

/// Build the menu for `status`. Release builds pass their updater state.
#[must_use]
pub fn entries(name: &str, status: &Status, updates: Option<Updates>) -> Vec<Entry> {
    let action = |command, text: &str| Entry::Action {
        command,
        text: text.into(),
        enabled: true,
    };
    let mut menu = vec![Entry::Label(status.text.clone())];
    if status.microphone_unavailable {
        menu.push(Entry::Label("Microphone status unavailable".into()));
    }
    menu.push(Entry::Separator);
    menu.push(action(Command::TakeBreak, "Take break now"));
    menu.push(action(
        Command::TogglePause,
        if status.paused {
            "Resume"
        } else {
            "Pause for 1 hour"
        },
    ));
    menu.push(Entry::Separator);
    if let Some(updates) = updates {
        menu.push(Entry::Action {
            command: Command::Updates,
            text: updates.label().into(),
            enabled: updates != Updates::Checking,
        });
    }
    menu.push(action(Command::Quit, &format!("Quit {name}")));
    menu
}

/// Result of one update attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Outcome {
    /// The installed version is the latest.
    Current = 0,
    /// A newer release replaced the executable.
    Installed = 1,
    /// Another update holds the install lock.
    Busy = 2,
    /// The check, download, verification or install failed.
    Failed = 3,
}

impl Outcome {
    /// Console message for `--check-for-updates`, and whether it succeeded.
    #[must_use]
    pub const fn report(self) -> (&'static str, bool) {
        match self {
            Self::Current => ("Up to date.", true),
            Self::Installed => ("Installed new release.", true),
            Self::Busy => ("Another update is in progress.", false),
            Self::Failed => ("Update failed.", false),
        }
    }

    /// Decode an outcome sent between threads.
    #[must_use]
    pub const fn from_code(code: usize) -> Self {
        match code {
            0 => Self::Current,
            1 => Self::Installed,
            2 => Self::Busy,
            _ => Self::Failed,
        }
    }
}

/// The updater's menu state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Updates {
    /// No check has finished yet.
    #[default]
    Idle,
    /// A check is running.
    Checking,
    /// The last check ended with this outcome.
    Done(Outcome),
}

/// What a menu click or the daily timer should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Nothing; a check is already running or an update waits for a break.
    Ignore,
    /// Start a background check.
    Check,
    /// Relaunch into the installed update.
    Restart,
}

impl Updates {
    /// Menu text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Check for Updates…",
            Self::Checking => "Checking for Updates…",
            Self::Done(Outcome::Installed) => "Restart to Finish Update",
            Self::Done(Outcome::Current) => "Up to Date · Check Again",
            Self::Done(Outcome::Busy) => "Update in Progress · Check Again",
            Self::Done(Outcome::Failed) => "Update Unavailable · Retry",
        }
    }

    /// Whether an installed update waits for a restart.
    #[must_use]
    pub const fn installed(self) -> bool {
        matches!(self, Self::Done(Outcome::Installed))
    }

    /// Handle a check request. Manual requests restart into an installed update.
    pub const fn request(&mut self, manual: bool) -> Request {
        if self.installed() {
            return if manual {
                Request::Restart
            } else {
                Request::Ignore
            };
        }
        if matches!(self, Self::Checking) {
            return Request::Ignore;
        }
        *self = Self::Checking;
        Request::Check
    }

    /// Record a finished check.
    pub const fn finish(&mut self, outcome: Outcome) {
        *self = Self::Done(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(text: &str, microphone_unavailable: bool, paused: bool) -> Status {
        Status {
            text: text.into(),
            microphone_unavailable,
            paused,
        }
    }

    fn texts(menu: &[Entry]) -> Vec<String> {
        menu.iter()
            .map(|entry| match entry {
                Entry::Label(text) => format!("({text})"),
                Entry::Action { text, enabled, .. } => {
                    if *enabled {
                        text.clone()
                    } else {
                        format!("[{text}]")
                    }
                }
                Entry::Separator => "-".into(),
            })
            .collect()
    }

    #[test]
    fn development_menu_matches_the_mac_layout() {
        let menu = entries(
            "Drink Dev",
            &status("Next break in 30 min", false, false),
            None,
        );
        assert_eq!(
            texts(&menu),
            [
                "(Next break in 30 min)",
                "-",
                "Take break now",
                "Pause for 1 hour",
                "-",
                "Quit Drink Dev"
            ]
        );
    }

    #[test]
    fn release_menu_adds_updates_and_microphone_state() {
        let menu = entries(
            "Drink",
            &status("Paused", true, true),
            Some(Updates::Checking),
        );
        assert_eq!(
            texts(&menu),
            [
                "(Paused)",
                "(Microphone status unavailable)",
                "-",
                "Take break now",
                "Resume",
                "-",
                "[Checking for Updates…]",
                "Quit Drink"
            ]
        );
        let commands: Vec<_> = menu
            .iter()
            .filter_map(|entry| match entry {
                Entry::Action { command, .. } => Some(*command),
                _ => None,
            })
            .collect();
        for command in &commands {
            assert_eq!(Command::from_id(*command as u32), Some(*command));
        }
        assert_eq!(commands.len(), 4);
        assert_eq!(Command::from_id(0), None);
    }

    #[test]
    fn updater_labels_and_requests_follow_each_outcome() {
        let mut updates = Updates::default();
        assert_eq!(updates.label(), "Check for Updates…");
        assert_eq!(updates.request(false), Request::Check);
        assert_eq!(updates.request(true), Request::Ignore);
        for (outcome, label) in [
            (Outcome::Current, "Up to Date · Check Again"),
            (Outcome::Busy, "Update in Progress · Check Again"),
            (Outcome::Failed, "Update Unavailable · Retry"),
        ] {
            assert_eq!(Outcome::from_code(outcome as usize), outcome);
            updates.finish(outcome);
            assert_eq!(updates.label(), label);
            assert!(!updates.installed());
            assert_eq!(updates.request(true), Request::Check);
        }
    }

    #[test]
    fn an_installed_update_waits_for_a_manual_restart() {
        assert_eq!(Outcome::Current.report(), ("Up to date.", true));
        assert_eq!(
            Outcome::Installed.report(),
            ("Installed new release.", true)
        );
        assert_eq!(
            Outcome::Busy.report(),
            ("Another update is in progress.", false)
        );
        assert_eq!(Outcome::Failed.report(), ("Update failed.", false));
        let mut updates = Updates::Checking;
        updates.finish(Outcome::Installed);
        assert_eq!(
            Outcome::from_code(Outcome::Installed as usize),
            Outcome::Installed
        );
        assert_eq!(Outcome::from_code(99), Outcome::Failed);
        assert_eq!(updates.label(), "Restart to Finish Update");
        assert!(updates.installed());
        assert_eq!(updates.request(false), Request::Ignore);
        assert_eq!(updates.request(true), Request::Restart);
    }
}
