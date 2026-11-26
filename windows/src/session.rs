//! Session lock, connection, sleep and display-power notifications.
#![allow(unsafe_code)]

use crate::controller::{Absence, Change, Observations};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::System::Power::{
    HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSActive, WTSFreeMemory,
    WTSINFOEXW, WTSQuerySessionInformationW, WTSRegisterSessionNotification, WTSSessionInfoEx,
    WTSUnRegisterSessionNotification,
};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::UI::WindowsAndMessaging::{
    DEVICE_NOTIFY_WINDOW_HANDLE, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND,
    PBT_POWERSETTINGCHANGE, WTS_CONSOLE_CONNECT, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_CONNECT,
    WTS_REMOTE_DISCONNECT, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};
use windows::core::{Owned, PWSTR};

/// Session and display-state notifications for one window.
pub struct Registration {
    hwnd: HWND,
    sessions: bool,
    _display: Option<Owned<HPOWERNOTIFY>>,
}

impl Registration {
    /// Deliver `WM_WTSSESSION_CHANGE` and display-state `WM_POWERBROADCAST`
    /// messages to `hwnd`. Windows reports the current display state at once.
    pub fn register(hwnd: HWND) -> Self {
        // SAFETY: `hwnd` is a live window on this thread; both registrations
        // are undone on drop, before the window is destroyed.
        unsafe {
            Self {
                hwnd,
                sessions: WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).is_ok(),
                _display: RegisterPowerSettingNotification(
                    HANDLE(hwnd.0),
                    &GUID_CONSOLE_DISPLAY_STATE,
                    DEVICE_NOTIFY_WINDOW_HANDLE,
                )
                .ok()
                .map(|handle| Owned::new(handle)),
            }
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if self.sessions {
            // SAFETY: undoes the registration made for this window.
            let _ = unsafe { WTSUnRegisterSessionNotification(self.hwnd) };
        }
    }
}

/// Record the lock and connection state at startup.
pub fn read_initial(seen: &mut Observations) {
    let mut buffer = PWSTR::null();
    let mut bytes = 0;
    // SAFETY: Windows allocates the buffer, which is read as the documented
    // structure only when it is large enough, then freed.
    unsafe {
        if WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &raw mut buffer,
            &raw mut bytes,
        )
        .is_err()
        {
            return;
        }
        if bytes as usize >= size_of::<WTSINFOEXW>() {
            let info = std::ptr::read_unaligned(buffer.0.cast::<WTSINFOEXW>());
            if info.Level == 1 {
                let level = info.Data.WTSInfoExLevel1;
                let absence = &mut seen.absence;
                absence.set(
                    Absence::LOCKED,
                    level.SessionFlags == WTS_SESSIONSTATE_LOCK.cast_signed(),
                );
                absence.set(Absence::DISCONNECTED, level.SessionState != WTSActive);
            }
        }
        WTSFreeMemory(buffer.0.cast());
    }
}

/// Translate a `WM_WTSSESSION_CHANGE` code.
pub const fn session_change(code: u32) -> Option<Change> {
    match code {
        WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT => Some(Change::Connected),
        WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => Some(Change::Disconnected),
        WTS_SESSION_LOCK => Some(Change::Locked),
        WTS_SESSION_UNLOCK => Some(Change::Unlocked),
        _ => None,
    }
}

/// Translate a `WM_POWERBROADCAST` message.
///
/// # Safety
///
/// For `PBT_POWERSETTINGCHANGE`, `lparam` must point to the
/// `POWERBROADCAST_SETTING` that Windows sent with the message.
pub unsafe fn power_change(event: u32, lparam: LPARAM) -> Option<Change> {
    match event {
        PBT_APMSUSPEND => Some(Change::Suspending),
        PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => Some(Change::Resumed),
        PBT_POWERSETTINGCHANGE if lparam.0 != 0 => {
            // SAFETY: the caller guarantees `lparam` points to the setting.
            let setting = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
            if setting.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || setting.DataLength < 4 {
                return None;
            }
            // SAFETY: Windows sends at least `DataLength` bytes of data.
            let state = unsafe { setting.Data.as_ptr().cast::<u32>().read_unaligned() };
            Some(if state == 0 {
                Change::DisplayOff
            } else {
                Change::DisplayOn
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_codes_map_to_presence_changes() {
        assert_eq!(session_change(WTS_CONSOLE_CONNECT), Some(Change::Connected));
        assert_eq!(session_change(WTS_REMOTE_CONNECT), Some(Change::Connected));
        assert_eq!(
            session_change(WTS_CONSOLE_DISCONNECT),
            Some(Change::Disconnected)
        );
        assert_eq!(
            session_change(WTS_REMOTE_DISCONNECT),
            Some(Change::Disconnected)
        );
        assert_eq!(session_change(WTS_SESSION_LOCK), Some(Change::Locked));
        assert_eq!(session_change(WTS_SESSION_UNLOCK), Some(Change::Unlocked));
        assert_eq!(session_change(0), None);
    }

    #[test]
    fn power_events_map_to_sleep_and_display_changes() {
        let display = |state: u32| {
            let mut setting = [0u8; size_of::<POWERBROADCAST_SETTING>() + 4];
            let header = POWERBROADCAST_SETTING {
                PowerSetting: GUID_CONSOLE_DISPLAY_STATE,
                DataLength: 4,
                Data: [0],
            };
            // SAFETY: the buffer is large enough for the header and data.
            unsafe {
                setting
                    .as_mut_ptr()
                    .cast::<POWERBROADCAST_SETTING>()
                    .write_unaligned(header);
                setting
                    .as_mut_ptr()
                    .add(std::mem::offset_of!(POWERBROADCAST_SETTING, Data))
                    .cast::<u32>()
                    .write_unaligned(state);
                power_change(PBT_POWERSETTINGCHANGE, LPARAM(setting.as_ptr() as isize))
            }
        };
        assert_eq!(display(0), Some(Change::DisplayOff));
        assert_eq!(display(1), Some(Change::DisplayOn));
        assert_eq!(display(2), Some(Change::DisplayOn));
        // SAFETY: these events carry no pointer.
        unsafe {
            assert_eq!(
                power_change(PBT_APMSUSPEND, LPARAM(0)),
                Some(Change::Suspending)
            );
            assert_eq!(
                power_change(PBT_APMRESUMEAUTOMATIC, LPARAM(0)),
                Some(Change::Resumed)
            );
            assert_eq!(
                power_change(PBT_APMRESUMESUSPEND, LPARAM(0)),
                Some(Change::Resumed)
            );
            assert_eq!(power_change(PBT_POWERSETTINGCHANGE, LPARAM(0)), None);
            assert_eq!(power_change(0, LPARAM(0)), None);
        }
        let other = POWERBROADCAST_SETTING::default();
        // SAFETY: points to a live setting for another GUID.
        let ignored = unsafe {
            power_change(
                PBT_POWERSETTINGCHANGE,
                LPARAM(std::ptr::from_ref(&other) as isize),
            )
        };
        assert_eq!(ignored, None);
    }

    #[test]
    fn the_current_session_is_readable() {
        // CI may run tests in a service session, so only stable facts are checked.
        let mut first = Observations {
            absence: Absence::SUSPENDED,
            ..Observations::default()
        };
        read_initial(&mut first);
        let mut second = Observations::default();
        read_initial(&mut second);
        second.absence.set(Absence::SUSPENDED, true);
        assert_eq!(
            first.absence, second.absence,
            "other reasons are kept and the reading is stable"
        );
    }
}
