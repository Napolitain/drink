//! Small Win32 helpers shared by the adapter modules.
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, PostMessageW,
};
use windows::core::{Owned, PWSTR};

/// Milliseconds of unbiased interrupt time. Like the macOS uptime clock, it
/// is monotonic and stops while the system sleeps.
#[must_use]
pub fn clock_ms() -> u64 {
    let mut ticks = 0u64;
    // SAFETY: the pointer refers to a live, writable u64. The call cannot fail
    // with a valid pointer.
    let _ = unsafe { QueryUnbiasedInterruptTime(&raw mut ticks) };
    ticks / 10_000
}

/// A window handle as a plain value, so worker threads can post to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowId(isize);

impl WindowId {
    /// Wrap `hwnd`.
    #[must_use]
    pub fn of(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }

    /// The window handle.
    #[must_use]
    pub const fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }

    /// Post `message` to the window. Fails quietly if it no longer exists.
    pub fn post(self, message: u32, wparam: usize) -> bool {
        self.post_with(message, wparam, 0)
    }

    /// Post `message` with both parameters, which must not be pointers.
    pub fn post_with(self, message: u32, wparam: usize, lparam: isize) -> bool {
        // SAFETY: PostMessageW accepts any handle value and reports invalid
        // windows as an error. Neither parameter points into this process.
        unsafe { PostMessageW(Some(self.hwnd()), message, WPARAM(wparam), LPARAM(lparam)) }.is_ok()
    }
}

/// Process that owns `hwnd`, or zero.
#[must_use]
pub fn window_process(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `pid` is a live, writable u32; invalid windows return zero.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
    pid
}

/// The foreground window, if there is one.
#[must_use]
pub fn foreground_window() -> Option<HWND> {
    // SAFETY: no arguments; returns null when no window is in the foreground.
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then_some(hwnd)
}

/// Whether the foreground window belongs to this process.
#[must_use]
pub fn foreground_is_own() -> bool {
    foreground_window().is_some_and(|hwnd| window_process(hwnd) == std::process::id())
}

/// Full path of a process's executable.
#[must_use]
pub fn process_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: opening with query rights has no side effects; the handle is
    // owned and closed on drop.
    let process =
        unsafe { Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?) };
    let mut buffer = vec![0u16; 32_768];
    let mut len = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` holds `len` u16s and outlives the call, which writes at
    // most `len` characters and stores the written length.
    unsafe {
        QueryFullProcessImageNameW(
            *process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &raw mut len,
        )
    }
    .ok()?;
    buffer.truncate(len as usize);
    Some(PathBuf::from(OsString::from_wide(&buffer)))
}
