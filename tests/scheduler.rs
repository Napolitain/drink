//! Behavior at deadlines, through session changes and under overlapping blockers.
use drink_core::{BREAK, GRACE, INTERVAL, PAUSE, Scheduler};
use proptest::prelude::*;

#[test]
fn default_durations_match_the_desktop_policy() {
    assert_eq!(INTERVAL, 1_800_000);
    assert_eq!(BREAK, 30_000);
    assert_eq!(GRACE, 10_000);
    assert_eq!(PAUSE, 3_600_000);
    assert!(!Scheduler::new(0).paused());
}

#[test]
fn normal_break_uses_actual_start_and_completion_times() {
    let mut s = Scheduler::new(100);
    assert_eq!(s.deadline(), 100 + INTERVAL);
    s.update(99 + INTERVAL, true, false);
    assert!(!s.visible());
    let start = 200 + INTERVAL;
    s.update(start, true, false);
    assert!(s.visible());
    assert_eq!(s.deadline(), start + BREAK);
    s.update(start + BREAK - 1, true, false);
    assert!(s.visible());
    s.update(start + BREAK, true, false);
    assert!(!s.visible());
    assert_eq!(s.deadline(), start + BREAK + INTERVAL);
}

#[test]
fn interruption_restarts_the_full_grace_and_break() {
    let mut s = Scheduler::new(0);
    s.update(INTERVAL, true, false);
    s.update(INTERVAL + 1, true, true);
    assert!(!s.visible());
    assert_eq!(s.deadline(), 0);
    s.update(INTERVAL + 2, true, false);
    assert_eq!(s.deadline(), INTERVAL + 2 + GRACE);
    s.update(INTERVAL + GRACE, true, true);
    assert_eq!(s.deadline(), 0);
    let clear = INTERVAL * 3;
    s.update(clear, true, false);
    s.update(clear + GRACE - 1, true, false);
    assert!(s.pending());
    s.update(clear + GRACE, true, false);
    assert!(s.visible());
    assert_eq!(s.deadline(), clear + GRACE + BREAK);
}

#[test]
fn lock_wake_and_manual_controls() {
    let mut s = Scheduler::new(0);
    s.request_break(1, true, true);
    assert!(s.pending());
    s.update(2, false, false);
    assert!(!s.pending());
    assert_eq!(s.deadline(), 0);
    s.update(INTERVAL, true, false);
    assert_eq!(s.deadline(), INTERVAL * 2);
    s.request_break(INTERVAL, true, false);
    assert!(s.visible());
    s.pause(INTERVAL);
    assert!(!s.visible());
    assert!(s.paused());
    s.update(INTERVAL + PAUSE - 1, false, false);
    assert!(s.paused());
    s.update(INTERVAL + PAUSE, true, true);
    assert!(!s.pending());
    assert!(!s.paused());
    assert_eq!(s.deadline(), INTERVAL * 2 + PAUSE);
    s.request_break(INTERVAL + PAUSE, true, false);
    s.restart(INTERVAL + PAUSE);
    assert_eq!(s.deadline(), INTERVAL * 2 + PAUSE);
    assert!(!s.pending());
    s.request_break(10 * INTERVAL, false, false);
    assert_eq!(s.deadline(), 0);
}

proptest! {
    #[test]
    fn overlapping_blockers_leave_only_one_due_break(
        overlap in 1..GRACE, busy in GRACE..INTERVAL * 5, mic_ends_first in any::<bool>()
    ) {
        let mut s = Scheduler::new(0);
        let inputs = [
            (INTERVAL, true, false),
            (INTERVAL + overlap, true, true),
            (INTERVAL + busy, !mic_ends_first, mic_ends_first),
        ];
        for (now, mic, game) in inputs {
            s.update(now, true, mic || game);
            prop_assert!(s.pending());
            prop_assert!(!s.visible());
            prop_assert_eq!(s.deadline(), 0);
        }
        let clear = INTERVAL + busy + 1;
        s.update(clear, true, false);
        s.update(clear + GRACE, true, false);
        prop_assert!(s.visible());
        let end = clear + GRACE + BREAK;
        s.update(end, true, false);
        prop_assert_eq!(s.deadline(), end + INTERVAL);
        prop_assert!(!s.pending());
    }
}

#[test]
fn pause_keeps_its_deadline_on_early_return_and_respects_a_busy_expiry() {
    let mut s = Scheduler::new(0);
    s.pause(10);
    assert_eq!(s.deadline(), 10 + PAUSE);
    s.update(100, false, false);
    assert_eq!(s.deadline(), 10 + PAUSE);
    s.update(10 + PAUSE - 1, true, true);
    assert!(s.paused());
    s.update(10 + PAUSE, true, true);
    assert!(s.pending());
    assert!(!s.visible());
}
