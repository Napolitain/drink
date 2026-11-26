//! Notification-area menu contents.

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
}

impl Command {
    /// The command for a menu identifier, if any.
    #[must_use]
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::TakeBreak),
            2 => Some(Self::TogglePause),
            3 => Some(Self::Quit),
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

/// Build the menu for `status`.
#[must_use]
pub fn entries(name: &str, status: &Status) -> Vec<Entry> {
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
    menu.push(action(Command::Quit, &format!("Quit {name}")));
    menu
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
        let menu = entries("Drink Dev", &status("Next break in 30 min", false, false));
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
    fn menu_shows_microphone_state_and_resume() {
        let menu = entries("Drink", &status("Paused", true, true));
        assert_eq!(
            texts(&menu),
            [
                "(Paused)",
                "(Microphone status unavailable)",
                "-",
                "Take break now",
                "Resume",
                "-",
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
        assert_eq!(commands.len(), 3);
        assert_eq!(Command::from_id(0), None);
    }
}
