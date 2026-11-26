//! Foreground-app changes and the shell's full-screen state.
#![allow(unsafe_code)]

use crate::controller::{Shell, is_configured_game};
use crate::win::{foreground_window, process_path, window_process};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, WINEVENTPROC};
use windows::Win32::UI::Shell::{
    QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};
use windows::core::Owned;

/// Receives foreground changes of other processes on the installing thread.
pub struct Hook(#[allow(dead_code)] Owned<HWINEVENTHOOK>);

impl Hook {
    /// Call `callback` from this thread's message loop when another process
    /// takes the foreground.
    pub fn install(callback: WINEVENTPROC) -> Option<Self> {
        // SAFETY: an out-of-context hook calls `callback` on this thread from
        // its message loop; the hook is removed when the owner drops.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                callback,
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        // SAFETY: the hook is valid when not null and is owned from here on.
        (!hook.is_invalid()).then(|| Self(unsafe { Owned::new(hook) }))
    }
}

/// Whether another process owns the foreground and is a configured game.
/// Returns `None` when this process, or nothing, is in the foreground.
pub fn foreground_game(configured: &[String]) -> Option<bool> {
    let pid = foreground_window().map(window_process)?;
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    Some(
        process_path(pid)
            .is_some_and(|path| is_configured_game(&path.to_string_lossy(), configured)),
    )
}

/// Full-screen state that should defer a break.
pub fn shell_state() -> Shell {
    // SAFETY: no arguments; the state is returned by value.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(QUNS_RUNNING_D3D_FULL_SCREEN) => Shell::Direct3D,
        Ok(QUNS_PRESENTATION_MODE) => Shell::Presentation,
        _ => Shell::Clear,
    }
}
