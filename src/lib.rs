//! Monotonic water-break scheduling shared by the desktop adapters.

/// Default time between completed breaks, in milliseconds.
pub const INTERVAL: u64 = 30 * 60 * 1000;
/// Duration of a visible break, in milliseconds.
pub const BREAK: u64 = 30_000;
/// Continuous quiet time required after a deferral, in milliseconds.
pub const GRACE: u64 = 10_000;
/// Duration of a manual pause, in milliseconds.
pub const PAUSE: u64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Counting(u64),
    Waiting,
    Grace(u64),
    Visible(u64),
    Paused(u64),
    PausedAway(u64),
    Away,
}

/// Consumes complete activity snapshots on changes and at its next deadline.
/// All timestamps must come from the same monotonic millisecond clock.
#[derive(Debug)]
pub struct Scheduler {
    state: State,
}

impl Scheduler {
    /// Start a fresh interval.
    #[must_use]
    pub const fn new(now: u64) -> Self {
        Self {
            state: State::Counting(now.saturating_add(INTERVAL)),
        }
    }

    /// Apply session presence and the union of all currently active blockers.
    pub const fn update(&mut self, now: u64, present: bool, blocked: bool) {
        self.state = match self.state {
            State::Paused(end) | State::PausedAway(end) if now < end => {
                if present {
                    State::Paused(end)
                } else {
                    State::PausedAway(end)
                }
            }
            _ if !present => State::Away,
            State::Away | State::PausedAway(_) => Self::new(now).state,
            State::Visible(_) if blocked => State::Waiting,
            State::Visible(end) if now >= end => Self::new(now).state,
            State::Counting(end) | State::Grace(end) if now >= end => Self::due(now, blocked),
            State::Grace(_) if blocked => State::Waiting,
            State::Waiting if !blocked => State::Grace(now.saturating_add(GRACE)),
            State::Paused(_) => Self::due(now, blocked),
            _ => self.state,
        };
    }

    const fn due(now: u64, blocked: bool) -> State {
        if blocked {
            State::Waiting
        } else {
            State::Visible(now.saturating_add(BREAK))
        }
    }

    /// Ask for a break immediately, respecting session and activity protection.
    pub const fn request_break(&mut self, now: u64, present: bool, blocked: bool) {
        self.state = if present {
            Self::due(now, blocked)
        } else {
            State::Away
        };
    }

    /// Suppress breaks for one hour, including if the session locks.
    pub const fn pause(&mut self, now: u64) {
        self.state = State::Paused(now.saturating_add(PAUSE));
    }

    /// Start a new interval after explicit dismissal or manual resume.
    pub const fn restart(&mut self, now: u64) {
        *self = Self::new(now);
    }

    /// Absolute time for the next update, or zero while waiting for an event.
    #[must_use]
    pub const fn deadline(&self) -> u64 {
        match self.state {
            State::Counting(end)
            | State::Grace(end)
            | State::Paused(end)
            | State::PausedAway(end)
            | State::Visible(end) => end,
            State::Waiting | State::Away => 0,
        }
    }

    /// Whether windows should currently cover the displays.
    #[must_use]
    pub const fn visible(&self) -> bool {
        matches!(self.state, State::Visible(_))
    }

    /// Whether a due break is waiting for activity to end or a grace period to pass.
    #[must_use]
    pub const fn pending(&self) -> bool {
        matches!(self.state, State::Waiting | State::Grace(_))
    }

    /// Whether an explicit one-hour pause is in effect.
    #[must_use]
    pub const fn paused(&self) -> bool {
        matches!(self.state, State::Paused(_) | State::PausedAway(_))
    }
}
