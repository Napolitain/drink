//! Desktop policy between the shared scheduler and the Win32 message loop.
//!
//! The adapter records what Windows reports in [`Observations`], asks the
//! [`Controller`] for a [`Plan`], and applies it. Everything here is
//! deterministic, so it is tested without windows or audio devices.

use drink_core::Scheduler;

/// Microphone detector reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Input {
    /// No capture session is active.
    #[default]
    Clear,
    /// At least one app has an active capture session.
    Active,
    /// Windows did not report capture sessions.
    Unavailable,
}

/// Full-screen state reported by the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shell {
    /// Nothing that should defer a break.
    #[default]
    Clear,
    /// An exclusive full-screen Direct3D app, usually a game.
    Direct3D,
    /// Presentation settings are on.
    Presentation,
}

/// A session or power transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The session reached the console or a remote client.
    Connected,
    /// The session left the console or remote client, e.g. a user switch.
    Disconnected,
    /// The workstation locked.
    Locked,
    /// The workstation unlocked.
    Unlocked,
    /// The system is about to sleep or hibernate.
    Suspending,
    /// The system resumed.
    Resumed,
    /// The console display turned off.
    DisplayOff,
    /// The console display turned on or dimmed.
    DisplayOn,
}

/// Reasons nobody could see a break, as a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Absence(u8);

impl Absence {
    /// The system is suspending.
    pub const SUSPENDED: Self = Self(1);
    /// The console display is off.
    pub const DISPLAY_OFF: Self = Self(1 << 1);
    /// The workstation is locked.
    pub const LOCKED: Self = Self(1 << 2);
    /// The session is not attached to the console or a remote client.
    pub const DISCONNECTED: Self = Self(1 << 3);

    /// Whether no reason applies.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether `reason` applies.
    #[must_use]
    pub const fn contains(self, reason: Self) -> bool {
        self.0 & reason.0 == reason.0
    }

    /// Add or remove `reason`.
    pub const fn set(&mut self, reason: Self, applies: bool) {
        if applies {
            self.0 |= reason.0;
        } else {
            self.0 &= !reason.0;
        }
    }
}

/// The latest desktop state known to the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observations {
    /// Session and power reasons nobody could see a break.
    pub absence: Absence,
    /// Number of attached displays.
    pub displays: usize,
    /// Latest microphone reading.
    pub microphone: Input,
    /// The microphone detector missed a subscription and should be sampled again.
    pub microphone_retry: bool,
    /// A recognised game is in the foreground.
    pub game: bool,
    /// Last keyboard use on the scheduler clock, recorded only while Drink
    /// listens. `None` means Drink has not listened yet.
    pub keyboard: Option<u64>,
    /// Last mouse use on the scheduler clock, recorded only while Drink listens.
    pub mouse: Option<u64>,
    /// Shell full-screen state.
    pub shell: Shell,
}

impl Default for Observations {
    fn default() -> Self {
        Self {
            absence: Absence(0),
            displays: 1,
            microphone: Input::Clear,
            microphone_retry: false,
            game: false,
            keyboard: None,
            mouse: None,
            shell: Shell::Clear,
        }
    }
}

impl Observations {
    /// Whether someone could see a break right now.
    #[must_use]
    pub const fn present(&self) -> bool {
        self.absence.is_empty() && self.displays > 0
    }

    /// Whether any known activity should defer a break at `now`.
    #[must_use]
    pub const fn blocked(&self, now: u64) -> bool {
        matches!(self.microphone, Input::Active)
            || self.playing(now)
            || !matches!(self.shell, Shell::Clear)
    }

    /// Whether a game is in front and in play: both the keyboard and the
    /// mouse were used within [`PLAYING_WINDOW`]. Typing alone, as when
    /// chatting after a match, or idling on a score screen is not play.
    /// Before Drink has listened, a game in front counts as in play.
    #[must_use]
    pub const fn playing(&self, now: u64) -> bool {
        if !self.game {
            return false;
        }
        match (self.keyboard, self.mouse) {
            (Some(keyboard), Some(mouse)) => {
                now.saturating_sub(keyboard) < PLAYING_WINDOW
                    && now.saturating_sub(mouse) < PLAYING_WINDOW
            }
            _ => true,
        }
    }

    /// When play lapses unless the keyboard and mouse are used again.
    #[must_use]
    pub const fn playing_ends(&self) -> Option<u64> {
        match (self.game, self.keyboard, self.mouse) {
            (true, Some(keyboard), Some(mouse)) => {
                let older = if keyboard < mouse { keyboard } else { mouse };
                Some(older.saturating_add(PLAYING_WINDOW))
            }
            _ => None,
        }
    }

    /// Record a session or power transition.
    pub const fn apply(&mut self, change: Change) {
        let (reason, applies) = match change {
            Change::Connected => (Absence::DISCONNECTED, false),
            Change::Disconnected => (Absence::DISCONNECTED, true),
            Change::Locked => (Absence::LOCKED, true),
            Change::Unlocked => (Absence::LOCKED, false),
            Change::Suspending => (Absence::SUSPENDED, true),
            Change::Resumed => (Absence::SUSPENDED, false),
            Change::DisplayOff => (Absence::DISPLAY_OFF, true),
            Change::DisplayOn => (Absence::DISPLAY_OFF, false),
        };
        self.absence.set(reason, applies);
    }
}

/// Whether `exe` names one of the configured game executables. Entries are
/// file names such as `game.exe`, or full paths, compared case-insensitively.
#[must_use]
pub fn is_configured_game(exe: &str, configured: &[String]) -> bool {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    configured.iter().any(|entry| {
        let entry = entry.trim();
        !entry.is_empty()
            && (entry.eq_ignore_ascii_case(name)
                || (entry.contains(['\\', '/']) && entry.eq_ignore_ascii_case(exe)))
    })
}

/// Retry delay for a microphone subscription while a break is due or visible.
pub const URGENT_RETRY: u64 = 1_000;
/// Retry delay for a microphone subscription otherwise.
pub const IDLE_RETRY: u64 = 60_000;
/// The shell sends no full-screen notifications, so a break it defers polls.
pub const SHELL_POLL: u64 = 1_000;
/// Keyboard and mouse use within this window, with a game in front, counts
/// as play. With the scheduler's grace period, a due break shows once 30
/// seconds pass without both.
pub const PLAYING_WINDOW: u64 = 20_000;

/// What the adapter should do after a scheduler step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Whether break windows should cover the displays.
    pub overlay: bool,
    /// Milliseconds until the next update, if one is scheduled.
    pub wake: Option<u64>,
    /// A displayed break just ended, so a staged update may restart Drink.
    pub break_finished: bool,
}

/// Menu status line and labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The first, informational menu line.
    pub text: String,
    /// Whether to show that microphone status is unavailable.
    pub microphone_unavailable: bool,
    /// Whether a manual pause is in effect.
    pub paused: bool,
}

/// Owns the scheduler and turns observations into plans.
#[derive(Debug)]
pub struct Controller {
    scheduler: Scheduler,
}

impl Controller {
    /// Start a fresh interval at `now`.
    #[must_use]
    pub const fn new(now: u64) -> Self {
        Self {
            scheduler: Scheduler::new(now),
        }
    }

    /// Advance the scheduler. `overlay_shown` is whether break windows were
    /// on screen before this step.
    pub fn update(&mut self, now: u64, seen: &Observations, overlay_shown: bool) -> Plan {
        let present = seen.present();
        let blocked = seen.blocked(now);
        self.scheduler.update(now, present, blocked);
        let s = &self.scheduler;
        Plan {
            overlay: s.visible(),
            wake: self.wake(now, seen),
            break_finished: overlay_shown
                && !s.visible()
                && !s.pending()
                && !s.paused()
                && present
                && !blocked,
        }
    }

    fn wake(&self, now: u64, seen: &Observations) -> Option<u64> {
        let s = &self.scheduler;
        let mut deadline = s.deadline();
        let mut sooner = |at: u64| {
            if deadline == 0 || at < deadline {
                deadline = at;
            }
        };
        if seen.microphone_retry && seen.present() {
            let urgent = s.pending() || s.visible();
            sooner(now.saturating_add(if urgent { URGENT_RETRY } else { IDLE_RETRY }));
        }
        if s.pending() && seen.shell != Shell::Clear {
            sooner(now.saturating_add(SHELL_POLL));
        }
        if s.pending()
            && let Some(end) = seen.playing_ends()
            && end > now
        {
            sooner(end);
        }
        (deadline != 0).then(|| deadline.saturating_sub(now))
    }

    /// Ask for a break now, respecting presence and blockers.
    pub const fn request_break(&mut self, now: u64, seen: &Observations) {
        self.scheduler
            .request_break(now, seen.present(), seen.blocked(now));
    }

    /// Pause for an hour, or resume with a fresh interval when paused.
    pub const fn toggle_pause(&mut self, now: u64) {
        if self.scheduler.paused() {
            self.scheduler.restart(now);
        } else {
            self.scheduler.pause(now);
        }
    }

    /// Start a fresh interval, e.g. after an early dismissal.
    pub const fn restart(&mut self, now: u64) {
        self.scheduler.restart(now);
    }

    /// Whether break windows should be on screen.
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.scheduler.visible()
    }

    /// Whether a due break is waiting.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.scheduler.pending()
    }

    /// Whether a manual pause is in effect.
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.scheduler.paused()
    }

    /// Menu status for the current state.
    #[must_use]
    pub fn status(&self, now: u64, seen: &Observations) -> Status {
        let s = &self.scheduler;
        let minutes = s.deadline().saturating_sub(now).div_ceil(60_000);
        let text = if s.paused() {
            format!("Paused · {minutes} min left")
        } else if !seen.present() {
            "Waiting for your return".into()
        } else if s.visible() {
            "Water break · hold Esc to end".into()
        } else if s.pending() {
            pending_reason(seen, now).into()
        } else {
            format!("Next break in {minutes} min")
        };
        Status {
            text,
            microphone_unavailable: seen.microphone == Input::Unavailable,
            paused: s.paused(),
        }
    }
}

const fn pending_reason(seen: &Observations, now: u64) -> &'static str {
    if matches!(seen.microphone, Input::Active) {
        "Break due · microphone in use"
    } else if seen.playing(now) {
        "Break due · game in progress"
    } else {
        match seen.shell {
            Shell::Direct3D => "Break due · full-screen game",
            Shell::Presentation => "Break due · presentation mode",
            Shell::Clear => "Break due · waiting for ten quiet seconds",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drink_core::{BREAK, GRACE, INTERVAL, PAUSE};

    const SEEN: Observations = Observations {
        absence: Absence(0),
        displays: 1,
        microphone: Input::Clear,
        microphone_retry: false,
        game: false,
        keyboard: None,
        mouse: None,
        shell: Shell::Clear,
    };

    /// A game in front, last used with the keyboard and mouse at these times.
    const fn game(keyboard: u64, mouse: u64) -> Observations {
        Observations {
            game: true,
            keyboard: Some(keyboard),
            mouse: Some(mouse),
            ..SEEN
        }
    }

    #[test]
    fn play_needs_both_keyboard_and_mouse_recently() {
        let w = PLAYING_WINDOW;
        assert!(!SEEN.playing(0), "no game in front");
        assert!(
            Observations { game: true, ..SEEN }.playing(0),
            "not yet listened"
        );
        assert!(game(0, 0).playing(w - 1));
        assert!(!game(0, 0).playing(w), "both idle for the window");
        assert!(
            !game(w, 0).playing(w),
            "typing only, e.g. chat after a match"
        );
        assert!(
            !game(0, w).playing(w),
            "mouse only, e.g. reading the score screen"
        );
        assert!(game(5, 9).playing(w + 4));
        assert_eq!(game(5, 9).playing_ends(), Some(5 + w));
        assert_eq!(game(9, 5).playing_ends(), Some(5 + w));
        assert_eq!(Observations { game: true, ..SEEN }.playing_ends(), None);
        assert_eq!(
            Observations {
                game: false,
                ..game(1, 1)
            }
            .playing_ends(),
            None
        );
        assert!(game(0, 0).blocked(1) && !game(0, 0).blocked(w));
    }

    #[test]
    fn a_break_waits_for_play_to_stop_then_shows() {
        let w = PLAYING_WINDOW;
        let mut c = Controller::new(0);
        // Due while playing: wait, and wake when play would lapse.
        let start = INTERVAL;
        let plan = c.update(start, &game(start, start), false);
        assert!(c.pending() && !plan.overlay);
        assert_eq!(plan.wake, Some(w));
        assert_eq!(
            c.status(start, &game(start, start)).text,
            "Break due · game in progress"
        );
        // Both still in use when the wake fires: wait again.
        let busy = start + w;
        assert_eq!(
            c.update(busy, &game(busy - 1, busy - 2), false).wake,
            Some(w - 2)
        );
        // Chat only: the mouse lapses, the grace period runs, then the break shows.
        let typing = busy + w;
        assert_eq!(
            c.update(typing, &game(typing, busy - 2), false).wake,
            Some(GRACE)
        );
        assert!(
            c.update(typing + GRACE, &game(typing + GRACE, busy - 2), false)
                .overlay
        );
    }

    #[test]
    fn play_resuming_during_the_grace_period_keeps_the_break_waiting() {
        let mut c = Controller::new(0);
        let idle = INTERVAL + PLAYING_WINDOW;
        c.update(INTERVAL, &game(INTERVAL, INTERVAL), false);
        c.update(idle, &game(INTERVAL, INTERVAL), false);
        let resumed = idle + GRACE;
        let plan = c.update(resumed, &game(resumed - 1, resumed - 1), false);
        assert!(c.pending() && !plan.overlay);
        assert_eq!(plan.wake, Some(PLAYING_WINDOW - 1));
    }

    #[test]
    fn every_session_change_controls_presence() {
        let mut seen = Observations::default();
        assert_eq!(seen, SEEN);
        assert!(seen.present());
        for (away, back) in [
            (Change::Disconnected, Change::Connected),
            (Change::Locked, Change::Unlocked),
            (Change::Suspending, Change::Resumed),
            (Change::DisplayOff, Change::DisplayOn),
        ] {
            seen.apply(away);
            assert!(!seen.present(), "{away:?}");
            seen.apply(back);
            assert!(seen.present(), "{back:?}");
        }
        seen.apply(Change::Locked);
        seen.apply(Change::DisplayOff);
        assert!(
            seen.absence.contains(Absence::LOCKED) && seen.absence.contains(Absence::DISPLAY_OFF)
        );
        seen.apply(Change::Unlocked);
        assert!(!seen.absence.contains(Absence::LOCKED) && !seen.present());
        seen.apply(Change::DisplayOn);
        seen.displays = 0;
        assert!(seen.absence.is_empty() && !seen.present());
    }

    #[test]
    fn microphone_games_and_shell_state_each_block() {
        assert!(!SEEN.blocked(0));
        for seen in [
            Observations {
                microphone: Input::Active,
                ..SEEN
            },
            Observations { game: true, ..SEEN },
            Observations {
                shell: Shell::Direct3D,
                ..SEEN
            },
            Observations {
                shell: Shell::Presentation,
                ..SEEN
            },
        ] {
            assert!(seen.blocked(0), "{seen:?}");
        }
        assert!(
            !Observations {
                microphone: Input::Unavailable,
                ..SEEN
            }
            .blocked(0)
        );
    }

    #[test]
    fn configured_games_match_names_or_paths_without_case() {
        let configured = vec![
            "Game.exe".to_owned(),
            " ".to_owned(),
            "D:\\Games\\Other\\play.exe".to_owned(),
        ];
        assert!(is_configured_game("C:\\Steam\\game.EXE", &configured));
        assert!(is_configured_game("game.exe", &configured));
        assert!(is_configured_game(
            "d:\\games\\other\\PLAY.exe",
            &configured
        ));
        assert!(!is_configured_game("C:\\Elsewhere\\play.exe", &configured));
        assert!(!is_configured_game("C:\\Steam\\steam.exe", &configured));
        assert!(!is_configured_game("C:\\Steam\\steam.exe", &[]));
    }

    #[test]
    fn a_timed_break_shows_then_finishes() {
        let mut c = Controller::new(0);
        let plan = c.update(0, &SEEN, false);
        assert_eq!(
            plan,
            Plan {
                overlay: false,
                wake: Some(INTERVAL),
                break_finished: false
            }
        );
        let plan = c.update(INTERVAL, &SEEN, false);
        assert_eq!(
            plan,
            Plan {
                overlay: true,
                wake: Some(BREAK),
                break_finished: false
            }
        );
        let end = INTERVAL + BREAK;
        let plan = c.update(end, &SEEN, true);
        assert_eq!(
            plan,
            Plan {
                overlay: false,
                wake: Some(INTERVAL),
                break_finished: true
            }
        );
        assert!(!c.update(end, &SEEN, false).break_finished);
    }

    #[test]
    fn a_break_ending_while_away_or_blocked_is_not_finished() {
        for seen in [
            Observations {
                absence: Absence::LOCKED,
                ..SEEN
            },
            Observations { game: true, ..SEEN },
        ] {
            let mut c = Controller::new(0);
            c.update(INTERVAL, &SEEN, false);
            let plan = c.update(INTERVAL + 1, &seen, true);
            assert!(!plan.overlay && !plan.break_finished, "{seen:?}");
        }
    }

    #[test]
    fn retries_and_shell_polls_wake_sooner() {
        let mut c = Controller::new(0);
        let retry = Observations {
            microphone_retry: true,
            ..SEEN
        };
        assert_eq!(c.update(0, &retry, false).wake, Some(IDLE_RETRY));
        let away = Observations {
            absence: Absence::LOCKED,
            ..retry
        };
        assert_eq!(c.update(1, &away, false).wake, None);

        let mut c = Controller::new(0);
        let full_screen = Observations {
            shell: Shell::Direct3D,
            ..SEEN
        };
        assert_eq!(
            c.update(INTERVAL, &full_screen, false).wake,
            Some(SHELL_POLL)
        );
        let both = Observations {
            microphone_retry: true,
            ..full_screen
        };
        assert_eq!(
            c.update(INTERVAL + 5, &both, false).wake,
            Some(URGENT_RETRY)
        );
        let mic = Observations {
            microphone: Input::Active,
            ..SEEN
        };
        assert_eq!(c.update(INTERVAL + 6, &mic, false).wake, None);
        assert_eq!(c.update(INTERVAL + 7, &SEEN, false).wake, Some(GRACE));
        assert_eq!(
            c.update(INTERVAL + 8, &retry, false).wake,
            Some(URGENT_RETRY)
        );
    }

    #[test]
    fn manual_controls_follow_the_scheduler() {
        let mut c = Controller::new(0);
        c.request_break(10, &SEEN);
        assert!(c.update(10, &SEEN, false).overlay);
        assert!(c.visible());
        c.toggle_pause(20);
        assert!(c.paused() && !c.visible());
        assert_eq!(c.update(20, &SEEN, true).wake, Some(PAUSE));
        c.toggle_pause(30);
        assert!(!c.paused());
        assert_eq!(c.update(30, &SEEN, false).wake, Some(INTERVAL));
        c.request_break(40, &Observations { game: true, ..SEEN });
        assert!(c.pending());
        c.restart(50);
        assert!(!c.pending());
        assert_eq!(c.update(50, &SEEN, false).wake, Some(INTERVAL));
    }

    #[test]
    fn status_explains_the_current_state() {
        let mut c = Controller::new(0);
        assert_eq!(c.status(0, &SEEN).text, "Next break in 30 min");
        assert_eq!(
            c.status(INTERVAL - 60_001, &SEEN).text,
            "Next break in 2 min"
        );
        assert_eq!(c.status(INTERVAL + 5, &SEEN).text, "Next break in 0 min");
        let away = Observations {
            absence: Absence::DISPLAY_OFF,
            ..SEEN
        };
        assert_eq!(c.status(0, &away).text, "Waiting for your return");
        let unavailable = Observations {
            microphone: Input::Unavailable,
            ..SEEN
        };
        assert!(c.status(0, &unavailable).microphone_unavailable);
        assert!(!c.status(0, &SEEN).microphone_unavailable);

        c.update(INTERVAL, &SEEN, false);
        assert_eq!(
            c.status(INTERVAL, &SEEN).text,
            "Water break · hold Esc to end"
        );

        let mut c = Controller::new(0);
        let reasons = [
            (
                Observations {
                    microphone: Input::Active,
                    game: true,
                    ..SEEN
                },
                "microphone in use",
            ),
            (
                Observations {
                    game: true,
                    shell: Shell::Direct3D,
                    ..SEEN
                },
                "game in progress",
            ),
            (
                Observations {
                    shell: Shell::Direct3D,
                    ..SEEN
                },
                "full-screen game",
            ),
            (
                Observations {
                    shell: Shell::Presentation,
                    ..SEEN
                },
                "presentation mode",
            ),
        ];
        for (seen, reason) in reasons {
            c.update(INTERVAL, &seen, false);
            assert_eq!(
                c.status(INTERVAL, &seen).text,
                format!("Break due · {reason}")
            );
        }
        c.update(INTERVAL, &SEEN, false);
        assert_eq!(
            c.status(INTERVAL, &SEEN).text,
            "Break due · waiting for ten quiet seconds"
        );

        c.toggle_pause(0);
        let status = c.status(1, &SEEN);
        assert_eq!(status.text, "Paused · 60 min left");
        assert!(status.paused);
    }
}
