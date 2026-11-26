//! Opaque break windows, one per display.
//!
//! A break takes keyboard focus before any window appears, so typing never
//! lands in a hidden app. Holding Escape for two seconds, or closing a window
//! with Alt+F4, asks the owner to end the break.
#![allow(unsafe_code)]

use crate::win::{foreground_is_own, foreground_window, window_process};
use std::sync::Once;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
    DEFAULT_CHARSET, DT_CALCRECT, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DrawTextW, EndPaint,
    EnumDisplayMonitors, FillRect, GetMonitorInfoW, HDC, HFONT, HGDIOBJ, HMONITOR, MONITORINFO,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_MOUSE, SendInput, SetFocus, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GW_OWNER, GetClientRect, GetWindow, IDC_ARROW,
    IsWindow, KillTimer, LoadCursorW, MONITORINFOF_PRIMARY, RegisterClassW, SW_SHOW, SW_SHOWNA,
    SetForegroundWindow, SetTimer, ShowWindow, WM_APP, WM_CLOSE, WM_KEYDOWN, WM_KEYUP,
    WM_KILLFOCUS, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, HSTRING, Owned, PCWSTR, w};

/// Posted to the owner window when a break window asks to end the break.
pub const WM_DISMISS: u32 = WM_APP + 1;
/// How long Escape must be held to end a break.
pub const HOLD_MS: u32 = 2_000;

const CLASS: PCWSTR = w!("io.github.napolitain.drink.break");
const HOLD_TIMER: usize = 1;
/// sRGB 0.055, 0.12, 0.14, as on macOS.
const BACKGROUND: COLORREF = rgb(14, 31, 36);
const TITLE: &str = "Get some water.";
const DETAIL: &str = "Look away and blink.";

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | (green as u32) << 8 | (blue as u32) << 16)
}

/// Break windows covering every display.
#[derive(Debug)]
pub struct Overlay {
    owner: HWND,
    present: bool,
    windows: Vec<HWND>,
    previous: Option<HWND>,
}

impl Overlay {
    /// Break windows owned by `owner`, which receives [`WM_DISMISS`]. With
    /// `present` false the windows are created but never shown or focused.
    #[must_use]
    pub fn new(owner: HWND, present: bool) -> Self {
        register_class();
        Self {
            owner,
            present,
            windows: Vec::new(),
            previous: None,
        }
    }

    /// Break windows, primary display first.
    #[must_use]
    pub fn windows(&self) -> &[HWND] {
        &self.windows
    }

    /// Cover every display. Returns false when Windows refused keyboard
    /// focus; nothing is shown then, and the caller retries later.
    pub fn show(&mut self) -> bool {
        if !self.windows.is_empty() {
            return true;
        }
        self.previous =
            foreground_window().filter(|hwnd| window_process(*hwnd) != std::process::id());
        self.create();
        if !self.present {
            return !self.windows.is_empty();
        }
        let Some(&first) = self.windows.first() else {
            return false;
        };
        // A timed break has no click to hand focus over, so take it before
        // showing anything.
        if !take_foreground(first) {
            self.destroy();
            self.previous = None;
            return false;
        }
        self.reveal();
        true
    }

    /// Recreate the windows after displays change.
    pub fn rebuild(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        let focused = self.present && foreground_is_own();
        self.destroy();
        self.create();
        if self.present {
            if focused && let Some(&first) = self.windows.first() {
                // SAFETY: `first` is a live window owned by this thread.
                let _ = unsafe { SetForegroundWindow(first) };
            }
            self.reveal();
        }
    }

    /// Close the windows and give focus back to the previous app if the user
    /// has not switched elsewhere.
    pub fn hide(&mut self) {
        let previous = self.previous.take();
        if self.windows.is_empty() {
            return;
        }
        if self.present
            && foreground_is_own()
            && let Some(previous) = previous
        {
            // SAFETY: IsWindow and SetForegroundWindow accept stale handles;
            // this process owns the foreground, so it may hand it back.
            unsafe {
                if IsWindow(Some(previous)).as_bool() {
                    let _ = SetForegroundWindow(previous);
                }
            }
        }
        self.destroy();
    }

    fn create(&mut self) {
        let instance = module();
        let title = HSTRING::from("Drink break");
        for rect in monitors() {
            // SAFETY: the class is registered and `owner` outlives its owned
            // windows, which are destroyed in `destroy` or on drop.
            let created = unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    CLASS,
                    &title,
                    WS_POPUP,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    Some(self.owner),
                    None,
                    instance,
                    None,
                )
            };
            if let Ok(hwnd) = created {
                self.windows.push(hwnd);
            }
        }
    }

    fn reveal(&self) {
        for (index, &hwnd) in self.windows.iter().enumerate() {
            // SAFETY: live windows owned by this thread.
            unsafe {
                let _ = ShowWindow(hwnd, if index == 0 { SW_SHOW } else { SW_SHOWNA });
            }
        }
        if let Some(&first) = self.windows.first() {
            // SAFETY: `first` belongs to the foreground thread.
            let _ = unsafe { SetFocus(Some(first)) };
        }
    }

    fn destroy(&mut self) {
        for hwnd in self.windows.drain(..) {
            // SAFETY: destroys a window created by this thread.
            let _ = unsafe { DestroyWindow(hwnd) };
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// Bring `hwnd` to the foreground. Windows lets only the process that
/// received the latest input do so. An empty mouse input, which neither moves
/// the pointer nor clicks, makes this process that source.
fn take_foreground(hwnd: HWND) -> bool {
    let focused = || foreground_window() == Some(hwnd);
    // SAFETY: `hwnd` is a live window owned by this thread.
    if unsafe { SetForegroundWindow(hwnd) }.as_bool() && focused() {
        return true;
    }
    let input = INPUT {
        r#type: INPUT_MOUSE,
        ..Default::default()
    };
    // SAFETY: one fully initialised INPUT of the stated size; `hwnd` is live.
    unsafe {
        SendInput(
            &[input],
            i32::try_from(size_of::<INPUT>()).unwrap_or(i32::MAX),
        );
        SetForegroundWindow(hwnd).as_bool() && focused()
    }
}

/// Monitor rectangles in physical pixels, primary first.
#[must_use]
pub fn monitors() -> Vec<RECT> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let mut info = MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or_default(),
            ..Default::default()
        };
        // SAFETY: `data` is the vector passed below, alive for the call.
        unsafe {
            let found = &mut *(data.0 as *mut Vec<(bool, RECT)>);
            if GetMonitorInfoW(monitor, &raw mut info).as_bool() {
                found.push((info.dwFlags & MONITORINFOF_PRIMARY == 0, info.rcMonitor));
            }
        }
        TRUE
    }
    let mut found: Vec<(bool, RECT)> = Vec::new();
    // SAFETY: the callback only writes to `found` during this call.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(std::ptr::from_mut(&mut found) as isize),
        )
    };
    found.sort_by_key(|(secondary, _)| *secondary);
    found.into_iter().map(|(_, rect)| rect).collect()
}

fn module() -> Option<windows::Win32::Foundation::HINSTANCE> {
    // SAFETY: returns the handle of this executable without a reference count.
    unsafe { GetModuleHandleW(None) }.ok().map(Into::into)
}

fn register_class() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        // SAFETY: the class name and procedure are static; a background brush
        // registered with a class lives for the process.
        unsafe {
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: module().unwrap_or_default(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: CreateSolidBrush(BACKGROUND),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassW(&raw const class);
        }
    });
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: called by Windows for windows of this class on their thread.
    unsafe {
        match message {
            WM_PAINT => paint(hwnd),
            WM_KEYDOWN if wparam.0 == usize::from(VK_ESCAPE.0) => {
                // Bit 30 is set for auto-repeat, which must not restart the hold.
                if lparam.0 & (1 << 30) == 0 {
                    SetTimer(Some(hwnd), HOLD_TIMER, HOLD_MS, None);
                }
            }
            WM_KEYUP if wparam.0 == usize::from(VK_ESCAPE.0) => {
                let _ = KillTimer(Some(hwnd), HOLD_TIMER);
            }
            WM_KILLFOCUS => {
                let _ = KillTimer(Some(hwnd), HOLD_TIMER);
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
            WM_TIMER if wparam.0 == HOLD_TIMER => {
                let _ = KillTimer(Some(hwnd), HOLD_TIMER);
                dismiss(hwnd);
            }
            WM_CLOSE => dismiss(hwnd),
            _ => return DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
    LRESULT(0)
}

fn dismiss(hwnd: HWND) {
    // SAFETY: GetWindow tolerates any handle; posting reaches the owner later.
    if let Ok(owner) = unsafe { GetWindow(hwnd, GW_OWNER) } {
        crate::win::WindowId::of(owner).post(WM_DISMISS, 0);
    }
}

fn font(height: i32, weight: i32, face: &str) -> Owned<HFONT> {
    // SAFETY: creates a font object owned by the returned value.
    unsafe {
        Owned::new(CreateFontW(
            -height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            &HSTRING::from(face),
        ))
    }
}

/// Paint the message centred on the window, scaled for its display.
///
/// # Safety
///
/// `hwnd` must be a live break window and this must handle its `WM_PAINT`.
unsafe fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let mut area = RECT::default();
    // SAFETY: the caller handles WM_PAINT for `hwnd`; every GDI object
    // selected into the DC is restored before it is freed.
    unsafe {
        let dc = BeginPaint(hwnd, &raw mut paint);
        let _ = GetClientRect(hwnd, &raw mut area);
        let background = Owned::new(CreateSolidBrush(BACKGROUND));
        FillRect(dc, &raw const area, *background);
        let dpi = i32::try_from(GetDpiForWindow(hwnd)).unwrap_or(96);
        let scale = |points: i32| points * dpi / 96;
        let title = font(scale(44), 600, "Segoe UI Variable Display");
        let detail = font(scale(24), 400, "Segoe UI Variable Text");
        SetBkMode(dc, TRANSPARENT);
        let previous = SelectObject(dc, HGDIOBJ(title.0));
        let measure = |font: &Owned<HFONT>, text: &str| {
            SelectObject(dc, HGDIOBJ(font.0));
            let mut text: Vec<u16> = text.encode_utf16().collect();
            let mut bounds = area;
            DrawTextW(
                dc,
                &mut text,
                &raw mut bounds,
                DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
            );
            bounds.bottom - bounds.top
        };
        let (title_height, detail_height) = (measure(&title, TITLE), measure(&detail, DETAIL));
        let gap = scale(12);
        let top = area.top + (area.bottom - area.top - title_height - gap - detail_height) / 2;
        let lines = [
            (&title, TITLE, rgb(255, 255, 255), top, title_height),
            (
                &detail,
                DETAIL,
                rgb(214, 214, 214),
                top + title_height + gap,
                detail_height,
            ),
        ];
        for (font, text, color, y, height) in lines {
            SelectObject(dc, HGDIOBJ(font.0));
            SetTextColor(dc, color);
            let mut text: Vec<u16> = text.encode_utf16().collect();
            let mut line = RECT {
                top: y,
                bottom: y + height,
                ..area
            };
            DrawTextW(
                dc,
                &mut text,
                &raw mut line,
                DT_CENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
        SelectObject(dc, previous);
        let _ = EndPaint(hwnd, &raw const paint);
    }
}
