//! Whether the keyboard and mouse are in use, from Raw Input.
//!
//! Drink listens only while a due break waits on a game in front. For each
//! event it requests just the header, which names the device type. Key codes
//! and button data are never requested, so they never reach Drink.
#![allow(unsafe_code)]

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_HEADER, RIDEV_INPUTSINK,
    RIDEV_REMOVE, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE, RegisterRawInputDevices,
};

/// HID generic desktop page, with its keyboard and mouse usages.
const GENERIC_DESKTOP: u16 = 0x01;
const MOUSE: u16 = 0x02;
const KEYBOARD: u16 = 0x06;

/// The kind of device an input event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// A keyboard.
    Keyboard,
    /// A mouse or touchpad.
    Mouse,
}

/// The device type in a Raw Input header.
pub const fn device_type(kind: u32) -> Option<Device> {
    if kind == RIM_TYPEKEYBOARD.0 {
        Some(Device::Keyboard)
    } else if kind == RIM_TYPEMOUSE.0 {
        Some(Device::Mouse)
    } else {
        None
    }
}

/// The device behind a `WM_INPUT` message, reading only its header.
///
/// # Safety
///
/// `lparam` must be the parameter of a `WM_INPUT` message being handled.
pub unsafe fn device(lparam: LPARAM) -> Option<Device> {
    let mut header = RAWINPUTHEADER::default();
    let expected = u32::try_from(size_of::<RAWINPUTHEADER>()).ok()?;
    let mut size = expected;
    // SAFETY: the caller passes the message's raw input handle. RID_HEADER
    // copies at most `size` bytes of header into the local.
    let copied = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as *mut _),
            RID_HEADER,
            Some((&raw mut header).cast()),
            &raw mut size,
            expected,
        )
    };
    if copied == u32::MAX {
        return None;
    }
    device_type(header.dwType)
}

/// Keyboard and mouse notifications for one window, delivered as `WM_INPUT`
/// even while another app is in front.
pub struct Listener {
    hwnd: HWND,
    active: bool,
}

impl Listener {
    /// A listener for `hwnd`, not yet listening.
    pub const fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            active: false,
        }
    }

    /// Whether `WM_INPUT` messages are being delivered.
    #[cfg(test)]
    pub const fn active(&self) -> bool {
        self.active
    }

    /// Start or stop listening. Returns whether listening just started.
    pub fn listen(&mut self, on: bool) -> bool {
        if on == self.active {
            return false;
        }
        let (flags, target) = if on {
            (RIDEV_INPUTSINK, self.hwnd)
        } else {
            (RIDEV_REMOVE, HWND::default())
        };
        let device = |usage| RAWINPUTDEVICE {
            usUsagePage: GENERIC_DESKTOP,
            usUsage: usage,
            dwFlags: flags,
            hwndTarget: target,
        };
        let devices = [device(KEYBOARD), device(MOUSE)];
        let size = u32::try_from(size_of::<RAWINPUTDEVICE>()).unwrap_or_default();
        // SAFETY: registers or removes this process's keyboard and mouse
        // notifications; the target window outlives the registration, which
        // drop removes.
        let changed = unsafe { RegisterRawInputDevices(&devices, size) }.is_ok();
        if changed {
            self.active = on;
        }
        on && changed
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.listen(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
    };
    use windows::core::w;

    #[test]
    fn device_types_map_to_keyboard_and_mouse_only() {
        assert_eq!(device_type(RIM_TYPEKEYBOARD.0), Some(Device::Keyboard));
        assert_eq!(device_type(RIM_TYPEMOUSE.0), Some(Device::Mouse));
        assert_eq!(device_type(2), None, "other HID devices are ignored");
        // SAFETY: an invalid handle makes GetRawInputData fail, never read.
        assert_eq!(unsafe { device(LPARAM(0)) }, None);
    }

    #[test]
    fn listening_starts_once_and_stops() {
        // SAFETY: a hidden window of a system class, destroyed below.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("listener test"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )
        };
        let Ok(hwnd) = hwnd else {
            unreachable!("a static window can always be created");
        };
        let mut listener = Listener::new(hwnd);
        assert!(!listener.active());
        assert!(listener.listen(true), "starts");
        assert!(listener.active());
        assert!(!listener.listen(true), "already listening");
        assert!(!listener.listen(false));
        assert!(!listener.active());
        assert!(listener.listen(true));
        drop(listener);
        // SAFETY: destroys the window created above.
        let _ = unsafe { DestroyWindow(hwnd) };
    }
}
