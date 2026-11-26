//! Notification-area icon and its popup menu.
#![allow(unsafe_code)]

use crate::icon::drop_alpha;
use crate::menu::Entry;
use crate::settings::light_taskbar;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, GetSystemMetrics, HICON, HMENU, ICONINFO,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON, SM_MENUDROPALIGNMENT,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_NULL,
};
use windows::core::{HSTRING, Owned, PCWSTR};

/// The icon in the notification area.
pub struct Tray {
    hwnd: HWND,
    message: u32,
    tip: String,
    icon: Option<Owned<HICON>>,
}

impl Tray {
    /// Add an icon that sends `message` to `hwnd`, with `tip` as its tooltip.
    pub fn add(hwnd: HWND, message: u32, tip: &str) -> Self {
        let mut tray = Self {
            hwnd,
            message,
            tip: tip.into(),
            icon: None,
        };
        tray.icon = create_icon(icon_size(hwnd), light_taskbar());
        tray.readd();
        tray
    }

    /// Add the icon again after Explorer restarts.
    pub fn readd(&self) {
        let data = self.data();
        // SAFETY: `data` is fully initialised and refers to a live window.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &raw const data);
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &raw const data);
        }
    }

    /// Redraw the icon for the current taskbar theme and DPI.
    pub fn refresh(&mut self) {
        self.icon = create_icon(icon_size(self.hwnd), light_taskbar());
        let data = self.data();
        // SAFETY: `data` is fully initialised; the old icon is freed only
        // after the shell has switched to the new one.
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &raw const data) };
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or_default(),
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: self.message,
            hIcon: self
                .icon
                .as_ref()
                .map_or_else(HICON::default, |icon| **icon),
            ..Default::default()
        };
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        for (slot, unit) in data.szTip.iter_mut().zip(self.tip.encode_utf16().take(127)) {
            *slot = unit;
        }
        data
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = self.data();
        // SAFETY: removes the icon this value added.
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &raw const data) };
    }
}

fn icon_size(hwnd: HWND) -> u32 {
    // SAFETY: plain metric queries; an invalid window reports 0 DPI.
    let size = unsafe {
        match GetDpiForWindow(hwnd) {
            0 => GetSystemMetrics(SM_CXSMICON),
            dpi => GetSystemMetricsForDpi(SM_CXSMICON, dpi),
        }
    };
    u32::try_from(size).unwrap_or(16).clamp(16, 256)
}

/// A white drop for dark taskbars, or a black one for light taskbars.
fn create_icon(size: u32, dark_ink: bool) -> Option<Owned<HICON>> {
    let ink: u32 = if dark_ink { 0 } else { 0x00FF_FFFF };
    let pixels: Vec<u32> = drop_alpha(size)
        .into_iter()
        .map(|alpha| u32::from(alpha) << 24 | ink)
        .collect();
    let extent = i32::try_from(size).ok()?;
    let header = BITMAPINFOHEADER {
        biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?,
        biWidth: extent,
        biHeight: -extent,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let info = BITMAPINFO {
        bmiHeader: header,
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: the DIB section holds `size * size` 32-bit pixels, which are
    // copied in before use. Bitmaps are owned and freed after the icon copies
    // them.
    unsafe {
        let color: Owned<HBITMAP> = Owned::new(
            CreateDIBSection(
                None,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                None,
                0,
            )
            .ok()?,
        );
        if bits.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
        // An all-zero AND mask lets the alpha channel decide transparency.
        let stride = size.div_ceil(16) as usize * 2;
        let mask_bits = vec![0u8; stride * size as usize];
        let mask: Owned<HBITMAP> = Owned::new(CreateBitmap(
            extent,
            extent,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        ));
        let icon = ICONINFO {
            fIcon: true.into(),
            hbmMask: *mask,
            hbmColor: *color,
            ..Default::default()
        };
        CreateIconIndirect(&raw const icon)
            .ok()
            .map(|icon| Owned::new(icon))
    }
}

/// Build a popup menu from `entries`.
fn build_menu(entries: &[Entry]) -> Option<Owned<HMENU>> {
    // SAFETY: the menu is owned by the returned value; item strings are
    // copied by AppendMenuW.
    unsafe {
        let menu = Owned::new(CreatePopupMenu().ok()?);
        for entry in entries {
            let _ = match entry {
                Entry::Label(text) => AppendMenuW(
                    *menu,
                    MF_STRING | MF_GRAYED,
                    0,
                    &HSTRING::from(text.as_str()),
                ),
                Entry::Action {
                    command,
                    text,
                    enabled,
                } => {
                    let flags = if *enabled {
                        MF_STRING
                    } else {
                        MF_STRING | MF_GRAYED
                    };
                    AppendMenuW(
                        *menu,
                        flags,
                        *command as usize,
                        &HSTRING::from(text.as_str()),
                    )
                }
                Entry::Separator => AppendMenuW(*menu, MF_SEPARATOR, 0, PCWSTR::null()),
            };
        }
        Some(menu)
    }
}

/// Show `entries` at `x`, `y` and return the chosen command identifier, or 0.
pub fn show_menu(hwnd: HWND, entries: &[Entry], x: i32, y: i32) -> u32 {
    let Some(menu) = build_menu(entries) else {
        return 0;
    };
    // SAFETY: `hwnd` is the live owner window. Taking the foreground first and
    // posting WM_NULL afterwards are the documented requirements for menus
    // opened from the notification area.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let align = if GetSystemMetrics(SM_MENUDROPALIGNMENT) == 0 {
            TPM_LEFTALIGN
        } else {
            TPM_RIGHTALIGN
        };
        let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | align;
        let chosen = TrackPopupMenuEx(*menu, flags.0, x, y, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        u32::try_from(chosen.0).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::Status;
    use crate::menu::entries;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetIconInfo, GetMenuItemCount, GetMenuStringW, MF_BYPOSITION,
    };

    #[test]
    fn icons_render_at_every_size_and_theme() {
        for size in [16, 24, 32, 48] {
            for dark in [false, true] {
                let icon = create_icon(size, dark);
                assert!(icon.is_some(), "{size} {dark}");
                let mut info = ICONINFO::default();
                // SAFETY: the icon is live; the returned bitmaps are freed.
                unsafe {
                    assert!(
                        GetIconInfo(
                            **icon.as_ref().unwrap_or_else(|| unreachable!()),
                            &raw mut info
                        )
                        .is_ok()
                    );
                    drop(Owned::new(info.hbmColor));
                    drop(Owned::new(info.hbmMask));
                }
            }
        }
        assert!((16..=256).contains(&icon_size(HWND::default())));
    }

    #[test]
    fn menus_keep_their_order_and_text() {
        let status = Status {
            text: "Next break in 30 min".into(),
            microphone_unavailable: true,
            paused: false,
        };
        let list = entries("Drink", &status);
        let menu = build_menu(&list).unwrap_or_else(|| unreachable!());
        let text = |index: i32| {
            let mut buffer = [0u16; 64];
            // SAFETY: reads an item of a live menu into a local buffer.
            let len = unsafe {
                GetMenuStringW(
                    *menu,
                    index.cast_unsigned(),
                    Some(&mut buffer),
                    MF_BYPOSITION,
                )
            };
            String::from_utf16_lossy(&buffer[..usize::try_from(len).unwrap_or(0)])
        };
        // SAFETY: counts items of a live menu.
        assert_eq!(unsafe { GetMenuItemCount(Some(*menu)) }, 7);
        assert_eq!(text(0), "Next break in 30 min");
        assert_eq!(text(1), "Microphone status unavailable");
        assert_eq!(text(3), "Take break now");
        assert_eq!(text(4), "Pause for 1 hour");
        assert_eq!(text(6), "Quit Drink");
    }
}
