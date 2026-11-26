//! The notification-area app. One hidden window owns the update timer, the
//! icon, session registrations and the break windows, and every event is
//! handled on its thread.
#![allow(unsafe_code)]

use crate::audio::Microphone;
use crate::controller::{Change, Controller, Observations, Status};
use crate::flavor::Flavor;
use crate::foreground::{self, Hook};
use crate::menu::{self, Command, Entry, Outcome, Request, Updates};
use crate::overlay::{Overlay, WM_DISMISS};
use crate::session::{self, Registration};
use crate::settings;
use crate::tray::{self, Tray};
use crate::update;
use crate::win::{WindowId, clock_ms};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::PoisonError;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, KillTimer, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW,
    PostQuitMessage, QS_ALLINPUT, RegisterClassW, RegisterWindowMessageW, SM_CMONITORS,
    SetCoalescableTimer, SetTimer, TranslateMessage, USER_TIMER_MAXIMUM, WM_APP, WM_CLOSE,
    WM_CONTEXTMENU, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_POWERBROADCAST, WM_QUIT,
    WM_SETTINGCHANGE, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR, w};

const WM_TRAY: u32 = WM_APP + 2;
const WM_AUDIO: u32 = WM_APP + 3;
const WM_FOREGROUND: u32 = WM_APP + 4;
const WM_UPDATED: u32 = WM_APP + 5;
const WM_EVENT: u32 = WM_APP + 6;
/// `NIN_SELECT | NINF_KEY`: the icon was chosen with the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;

const TIMER_UPDATE: usize = 1;
const TIMER_DAILY: usize = 2;
const DAY_MS: u32 = 86_400_000;
const DAY_TOLERANCE_MS: u32 = 300_000;
/// Retry delay after Windows refused keyboard focus for a break.
const FOCUS_RETRY: u64 = 1_000;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static WINDOW: Cell<Option<WindowId>> = const { Cell::new(None) };
}

const CHANGES: [Change; 8] = [
    Change::Connected,
    Change::Disconnected,
    Change::Locked,
    Change::Unlocked,
    Change::Suspending,
    Change::Resumed,
    Change::DisplayOff,
    Change::DisplayOn,
];

/// Something the app window was told, decoded from its Win32 message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Tick,
    Daily,
    Change(Change),
    Displays,
    Theme,
    TaskbarCreated,
    Dismiss,
    Menu { x: i16, y: i16 },
    Audio,
    Foreground,
    Updated(Outcome),
    Command(Command),
    Quit,
}

impl Event {
    /// Encode as `WM_EVENT` parameters, to handle later.
    fn pack(self) -> (usize, isize) {
        match self {
            Self::Tick => (0, 0),
            Self::Daily => (1, 0),
            Self::Change(change) => (
                2,
                CHANGES
                    .iter()
                    .position(|c| *c == change)
                    .map_or(0, usize::cast_signed),
            ),
            Self::Displays => (3, 0),
            Self::Theme => (4, 0),
            Self::TaskbarCreated => (5, 0),
            Self::Dismiss => (6, 0),
            Self::Menu { x, y } => {
                let anchor =
                    (usize::from(x.cast_unsigned()) << 16) | usize::from(y.cast_unsigned());
                (7, anchor.cast_signed())
            }
            Self::Audio => (8, 0),
            Self::Foreground => (9, 0),
            Self::Updated(outcome) => (10, (outcome as usize).cast_signed()),
            Self::Command(command) => (11, command as isize),
            Self::Quit => (12, 0),
        }
    }

    fn unpack(tag: usize, data: isize) -> Option<Self> {
        let low = |shift: u32| {
            u16::try_from((data >> shift) & 0xFFFF)
                .unwrap_or(0)
                .cast_signed()
        };
        Some(match tag {
            0 => Self::Tick,
            1 => Self::Daily,
            2 => Self::Change(*CHANGES.get(usize::try_from(data).ok()?)?),
            3 => Self::Displays,
            4 => Self::Theme,
            5 => Self::TaskbarCreated,
            6 => Self::Dismiss,
            7 => Self::Menu {
                x: low(16),
                y: low(0),
            },
            8 => Self::Audio,
            9 => Self::Foreground,
            10 => Self::Updated(Outcome::from_code(usize::try_from(data).ok()?)),
            11 => Self::Command(Command::from_id(u32::try_from(data).ok()?)?),
            12 => Self::Quit,
            _ => return None,
        })
    }
}

/// Decode a message sent to the app window.
///
/// # Safety
///
/// `lparam` must be the parameter Windows sent with `message`.
unsafe fn decode(message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<Event> {
    let word = |value: usize, shift: u32| u16::try_from((value >> shift) & 0xFFFF).unwrap_or(0);
    Some(match message {
        WM_TIMER if wparam.0 == TIMER_UPDATE => Event::Tick,
        WM_TIMER if wparam.0 == TIMER_DAILY => Event::Daily,
        WM_WTSSESSION_CHANGE => {
            Event::Change(session::session_change(u32::try_from(wparam.0).ok()?)?)
        }
        WM_POWERBROADCAST => {
            let event = u32::try_from(wparam.0).ok()?;
            // SAFETY: forwarded from the caller.
            Event::Change(unsafe { session::power_change(event, lparam) }?)
        }
        WM_DISPLAYCHANGE => Event::Displays,
        WM_SETTINGCHANGE | WM_DPICHANGED => Event::Theme,
        WM_DISMISS => Event::Dismiss,
        WM_TRAY => match u32::from(word(lparam.0.cast_unsigned(), 0)) {
            WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT => Event::Menu {
                x: word(wparam.0, 0).cast_signed(),
                y: word(wparam.0, 16).cast_signed(),
            },
            _ => return None,
        },
        WM_AUDIO => Event::Audio,
        WM_FOREGROUND => Event::Foreground,
        WM_UPDATED => Event::Updated(Outcome::from_code(wparam.0)),
        WM_EVENT => Event::unpack(wparam.0, lparam.0)?,
        WM_CLOSE => Event::Quit,
        _ if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            Event::TaskbarCreated
        }
        _ => return None,
    })
}

/// Work that must run after the app is no longer borrowed.
enum After {
    Nothing,
    Menu { x: i16, y: i16, entries: Vec<Entry> },
    Quit,
}

/// How the app presents itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Add the notification-area icon.
    pub tray: bool,
    /// Show break windows and take keyboard focus. Without it, break windows
    /// are created hidden, so tests never disturb the desktop.
    pub present: bool,
}

impl Options {
    /// The real app.
    pub const INTERACTIVE: Self = Self {
        tray: true,
        present: true,
    };
    /// No icon and hidden break windows.
    pub const HIDDEN: Self = Self {
        tray: false,
        present: false,
    };
}

/// The running reminder, owned by the thread that called [`start`].
pub struct App {
    flavor: Flavor,
    window: WindowId,
    controller: Controller,
    seen: Observations,
    overlay: Overlay,
    microphone: Microphone,
    games: Vec<String>,
    updates: Option<Updates>,
    restarting: bool,
    clock_offset: u64,
    tray: Option<Tray>,
    _hook: Option<Hook>,
    _session: Registration,
}

impl App {
    /// Milliseconds on the scheduler clock.
    #[must_use]
    pub fn now(&self) -> u64 {
        clock_ms().saturating_add(self.clock_offset)
    }

    /// Time added to the system clock; the smoke test advances it.
    #[must_use]
    pub const fn clock_offset(&self) -> u64 {
        self.clock_offset
    }

    /// Advance the scheduler clock past the system clock.
    pub const fn set_clock_offset(&mut self, offset: u64) {
        self.clock_offset = offset;
    }

    /// The latest desktop observations.
    #[must_use]
    pub const fn observations(&self) -> &Observations {
        &self.seen
    }

    /// Change session or power observations, as Windows notifications would.
    pub const fn observations_mut(&mut self) -> &mut Observations {
        &mut self.seen
    }

    /// The scheduler state.
    #[must_use]
    pub const fn controller(&self) -> &Controller {
        &self.controller
    }

    /// Break windows on screen, primary display first.
    #[must_use]
    pub fn break_windows(&self) -> &[HWND] {
        self.overlay.windows()
    }

    /// The status line of the menu.
    #[must_use]
    pub fn status(&self) -> Status {
        self.controller.status(self.now(), &self.seen)
    }

    /// The menu as it would open now.
    #[must_use]
    pub fn menu(&self) -> Vec<Entry> {
        menu::entries(self.flavor.name, &self.status(), self.updates)
    }

    /// Sample activity and advance the scheduler.
    pub fn update(&mut self) {
        self.observe();
        self.step();
    }

    /// Take break now, respecting protected activities.
    pub fn take_break(&mut self) {
        self.observe();
        self.controller.request_break(self.now(), &self.seen);
        self.step();
    }

    /// Pause for an hour, or resume.
    pub fn toggle_pause(&mut self) {
        self.controller.toggle_pause(self.now());
        self.update();
    }

    /// Start a fresh interval.
    pub fn restart(&mut self) {
        self.controller.restart(self.now());
        self.update();
    }

    fn observe(&mut self) {
        self.seen.microphone = self.microphone.sample();
        self.seen.microphone_retry = self.microphone.needs_retry();
        self.seen.shell = foreground::shell_state();
        // SAFETY: a plain metric query.
        self.seen.displays =
            usize::try_from(unsafe { GetSystemMetrics(SM_CMONITORS) }).unwrap_or(0);
    }

    fn step(&mut self) {
        let shown = !self.overlay.windows().is_empty();
        let plan = self.controller.update(self.now(), &self.seen, shown);
        let mut wake = plan.wake;
        if !plan.overlay {
            self.overlay.hide();
        } else if !self.overlay.show() {
            wake = Some(wake.map_or(FOCUS_RETRY, |ms| ms.min(FOCUS_RETRY)));
        }
        if plan.break_finished && self.updates.is_some_and(Updates::installed) {
            self.restart_into_update();
        }
        self.schedule(wake);
    }

    fn schedule(&self, wake: Option<u64>) {
        let hwnd = Some(self.window.hwnd());
        // SAFETY: the timer belongs to the app window on this thread.
        unsafe {
            let _ = KillTimer(hwnd, TIMER_UPDATE);
            if let Some(ms) = wake {
                let ms = u32::try_from(ms)
                    .unwrap_or(USER_TIMER_MAXIMUM)
                    .clamp(1, USER_TIMER_MAXIMUM);
                SetTimer(hwnd, TIMER_UPDATE, ms, None);
            }
        }
    }

    fn foreground_changed(&mut self) {
        let Some(game) = foreground::foreground_game(&self.games) else {
            return self.update();
        };
        self.seen.game = game;
        self.observe();
        // Switching to another app must never leave opaque windows above it.
        if !self.overlay.windows().is_empty() && !self.seen.blocked() {
            self.restart();
        } else {
            self.step();
        }
    }

    fn refresh_icon(&mut self) {
        if let Some(tray) = &mut self.tray {
            tray.refresh();
        }
    }

    fn request_updates(&mut self, manual: bool) {
        let Some(updates) = &mut self.updates else {
            return;
        };
        match updates.request(manual) {
            Request::Check => self.check_for_updates(),
            Request::Restart => self.restart_into_update(),
            Request::Ignore => {}
        }
    }

    fn check_for_updates(&self) {
        let window = self.window;
        let version = self.flavor.release.unwrap_or_default();
        std::thread::spawn(move || {
            let outcome = std::env::current_exe().map_or(Outcome::Failed, |exe| {
                update::install_latest(&update::Install::running(&exe, version), &update::Curl)
            });
            window.post(WM_UPDATED, outcome as usize);
        });
    }

    fn restart_into_update(&mut self) {
        if self.restarting {
            return;
        }
        match std::env::current_exe().and_then(|exe| update::relaunch(&exe)) {
            Ok(()) => {
                self.restarting = true;
                self.window.post(WM_CLOSE, 0);
            }
            Err(error) => eprintln!("Drink restart: {error}"),
        }
    }

    fn handle(&mut self, event: Event) -> After {
        match event {
            Event::Tick | Event::Audio => self.update(),
            Event::Daily => self.request_updates(false),
            Event::Change(change) => {
                self.seen.apply(change);
                self.update();
            }
            Event::Displays => {
                self.update();
                self.overlay.rebuild();
                self.refresh_icon();
            }
            Event::Theme => self.refresh_icon(),
            Event::TaskbarCreated => {
                if let Some(tray) = &self.tray {
                    tray.readd();
                }
            }
            Event::Dismiss => self.restart(),
            Event::Foreground => self.foreground_changed(),
            Event::Updated(outcome) => {
                if let Some(updates) = &mut self.updates {
                    updates.finish(outcome);
                }
            }
            Event::Menu { x, y } => {
                return After::Menu {
                    x,
                    y,
                    entries: self.menu(),
                };
            }
            Event::Command(Command::TakeBreak) => self.take_break(),
            Event::Command(Command::TogglePause) => self.toggle_pause(),
            Event::Command(Command::Updates) => self.request_updates(true),
            Event::Command(Command::Quit) | Event::Quit => return After::Quit,
        }
        After::Nothing
    }
}

/// The app running on this thread. Dropping it removes the icon and closes
/// every window.
pub struct Handle {
    window: WindowId,
    _thread: PhantomData<*const ()>,
}

impl Handle {
    /// The hidden app window.
    #[must_use]
    pub const fn window(&self) -> HWND {
        self.window.hwnd()
    }

    /// Use the app, unless it is busy handling an event.
    pub fn with<R>(&self, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        APP.with(|cell| {
            let mut app = cell.try_borrow_mut().ok()?;
            app.as_mut().map(f)
        })
    }

    /// Send a menu command through the message loop.
    pub fn post_command(&self, command: Command) {
        let (tag, data) = Event::Command(command).pack();
        post_event(self.window, tag, data);
    }

    /// Handle messages for `duration`. Returns false once the app quits.
    #[must_use]
    pub fn pump(&self, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        let mut message = MSG::default();
        loop {
            // SAFETY: a message loop on the thread that owns the windows.
            unsafe {
                while PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    if message.message == WM_QUIT {
                        return false;
                    }
                    let _ = TranslateMessage(&raw const message);
                    DispatchMessageW(&raw const message);
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            let ms = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
            // SAFETY: waits for input to this thread or the timeout.
            unsafe { MsgWaitForMultipleObjects(None, false, ms, QS_ALLINPUT) };
        }
    }

    /// Run until the app quits.
    #[must_use]
    pub fn run(self) -> u8 {
        let mut message = MSG::default();
        // SAFETY: the standard message loop on the thread that owns the windows.
        unsafe {
            while GetMessageW(&raw mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let app = APP.with(|cell| cell.borrow_mut().take());
        drop(app);
        WINDOW.with(|window| window.set(None));
        // SAFETY: destroys the app window created by this thread, if it remains.
        let _ = unsafe { DestroyWindow(self.window.hwnd()) };
        // Never exit halfway through replacing the executable.
        drop(
            update::EXCHANGE
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
    }
}

/// Start the app on this thread.
///
/// # Errors
///
/// The app window could not be created.
pub fn start(flavor: Flavor, options: Options) -> windows::core::Result<Handle> {
    // SAFETY: per-thread COM initialisation for Core Audio. Repeated calls,
    // and setting the process DPI awareness more than once, fail harmlessly.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
    }
    let hwnd = create_window(flavor)?;
    let window = WindowId::of(hwnd);
    WINDOW.with(|cell| cell.set(Some(window)));
    let mut seen = Observations::default();
    session::read_initial(&mut seen);
    let app = App {
        flavor,
        window,
        controller: Controller::new(clock_ms()),
        seen,
        overlay: Overlay::new(hwnd, options.present),
        microphone: Microphone::new(window, WM_AUDIO),
        games: settings::protected_games(&flavor.registry_key()),
        updates: flavor.release.map(|_| Updates::default()),
        restarting: false,
        clock_offset: 0,
        tray: options.tray.then(|| Tray::add(hwnd, WM_TRAY, flavor.name)),
        _hook: Hook::install(Some(on_foreground)),
        _session: Registration::register(hwnd),
    };
    APP.with(|cell| cell.replace(Some(app)));
    let handle = Handle {
        window,
        _thread: PhantomData,
    };
    handle.with(|app| {
        if let Some(game) = foreground::foreground_game(&app.games) {
            app.seen.game = game;
        }
        app.update();
        if app.updates.is_some() {
            app.request_updates(false);
            // SAFETY: a timer on the app window.
            unsafe { SetCoalescableTimer(Some(hwnd), TIMER_DAILY, DAY_MS, None, DAY_TOLERANCE_MS) };
        }
    });
    Ok(handle)
}

/// Run the reminder until it quits. Returns the process exit code.
#[must_use]
pub fn run(flavor: Flavor) -> u8 {
    match start(flavor, Options::INTERACTIVE) {
        Ok(handle) => handle.run(),
        Err(error) => {
            eprintln!("Drink: {error}");
            1
        }
    }
}

fn create_window(flavor: Flavor) -> windows::core::Result<HWND> {
    let class = HSTRING::from(flavor.identifier);
    // SAFETY: registers a class whose name is copied, then creates a hidden
    // tool window of it. Registering an existing class fails harmlessly.
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class_info = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&raw const class_info);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            &class,
            &HSTRING::from(flavor.name),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
}

fn post_event(window: WindowId, tag: usize, data: isize) {
    window.post_with(WM_EVENT, tag, data);
}

fn dispatch(window: WindowId, event: Event) {
    let after = APP.with(|cell| {
        cell.try_borrow_mut().map_or_else(
            |_| {
                // A Win32 call made while handling an event delivered this
                // one re-entrantly. Handle it from the queue afterwards.
                let (tag, data) = event.pack();
                post_event(window, tag, data);
                None
            },
            |mut app| app.as_mut().map(|app| app.handle(event)),
        )
    });
    match after {
        Some(After::Menu { x, y, entries }) => {
            let chosen = tray::show_menu(window.hwnd(), &entries, x.into(), y.into());
            if let Some(command) = Command::from_id(chosen) {
                dispatch(window, Event::Command(command));
            }
        }
        // SAFETY: ends this thread's message loop.
        Some(After::Quit) => unsafe { PostQuitMessage(0) },
        Some(After::Nothing) | None => {}
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        // SAFETY: ends this thread's message loop.
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    // SAFETY: Windows sent `lparam` with `message`.
    let Some(event) = (unsafe { decode(message, wparam, lparam) }) else {
        // SAFETY: default handling for messages the app ignores.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    dispatch(WindowId::of(hwnd), event);
    LRESULT(isize::from(message == WM_POWERBROADCAST))
}

unsafe extern "system" fn on_foreground(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if let Some(window) = WINDOW.with(Cell::get) {
        window.post(WM_FOREGROUND, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::Absence;
    use crate::overlay::monitors;
    use drink_core::{BREAK, INTERVAL};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsWindow, IsWindowVisible, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND,
        SendMessageW, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_PAINT, WTS_SESSION_LOCK,
        WTS_SESSION_UNLOCK,
    };

    const TEST: Flavor = Flavor {
        name: "DrinkTest",
        identifier: "io.github.napolitain.drink.test",
        release: None,
    };

    fn started(options: Options) -> Handle {
        let handle = start(TEST, options).unwrap_or_else(|error| panic!("{error}"));
        handle.with(|app| {
            app.observations_mut().absence = Absence::default();
            app.update();
        });
        handle
    }

    fn status(handle: &Handle) -> String {
        handle.with(|app| app.status().text).unwrap_or_default()
    }

    fn windows(handle: &Handle) -> Vec<HWND> {
        handle
            .with(|app| app.break_windows().to_vec())
            .unwrap_or_default()
    }

    /// Breaks only appear when a display is attached and nothing on this
    /// machine defers them.
    fn idle(handle: &Handle) -> bool {
        let idle = handle
            .with(|app| app.observations().present() && !app.observations().blocked())
            .unwrap_or(false);
        if !idle {
            eprintln!("skipped: no display, or a microphone, game or full-screen app is active");
        }
        idle
    }

    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) {
        // SAFETY: delivers a message synchronously to a window on this thread.
        unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) };
    }

    #[test]
    fn launch_is_quiet_and_the_menu_reflects_the_schedule() {
        let handle = started(Options::HIDDEN);
        assert!(windows(&handle).is_empty());
        assert_eq!(status(&handle), "Next break in 30 min");
        let menu = handle.with(|app| app.menu()).unwrap_or_default();
        assert_eq!(
            menu.first(),
            Some(&Entry::Label("Next break in 30 min".into()))
        );
        assert!(menu.contains(&Entry::Action {
            command: Command::Quit,
            text: "Quit DrinkTest".into(),
            enabled: true
        }));
        assert!(!menu.iter().any(|entry| matches!(
            entry,
            Entry::Action {
                command: Command::Updates,
                ..
            }
        )));
        assert!(handle.pump(Duration::from_millis(50)));
    }

    #[test]
    fn a_timed_break_covers_every_display_then_finishes() {
        let handle = started(Options::HIDDEN);
        if !idle(&handle) {
            return;
        }
        handle.with(|app| {
            app.set_clock_offset(INTERVAL);
            app.update();
        });
        let shown = windows(&handle);
        let screens = monitors();
        assert_eq!(shown.len(), screens.len());
        for (hwnd, screen) in shown.iter().zip(&screens) {
            let mut rect = RECT::default();
            // SAFETY: queries live windows created on this thread.
            unsafe {
                assert!(GetWindowRect(*hwnd, &raw mut rect).is_ok());
                assert!(
                    !IsWindowVisible(*hwnd).as_bool(),
                    "tests never show windows"
                );
            }
            assert_eq!(rect, *screen);
            send(*hwnd, WM_PAINT, 0, 0);
        }
        assert_eq!(status(&handle), "Water break · hold Esc to end");
        handle.with(|app| {
            app.set_clock_offset(app.clock_offset() + BREAK);
            app.update();
        });
        assert!(windows(&handle).is_empty());
        let exists = |hwnd: &HWND| {
            // SAFETY: the old handle may be stale, which IsWindow reports.
            unsafe { IsWindow(Some(*hwnd)) }.as_bool()
        };
        assert!(
            !shown.iter().any(exists),
            "every break window was destroyed"
        );
        assert_eq!(status(&handle), "Next break in 30 min");
    }

    #[test]
    fn only_a_two_second_escape_hold_ends_a_break() {
        let handle = started(Options::HIDDEN);
        if !idle(&handle) {
            return;
        }
        handle.with(App::take_break);
        let first = windows(&handle)[0];
        let escape = usize::from(VK_ESCAPE.0);
        // A press interrupted by focus loss, then an auto-repeat, never dismisses.
        send(first, WM_KEYDOWN, escape, 0);
        send(first, WM_KILLFOCUS, 0, 0);
        send(first, WM_KEYDOWN, escape, 1 << 30);
        assert!(handle.pump(Duration::from_millis(2_200)));
        assert_eq!(windows(&handle).len(), monitors().len());
        // A tap, then a held press.
        send(first, WM_KEYDOWN, escape, 0);
        send(first, WM_KEYUP, escape, 0);
        send(first, WM_KEYDOWN, escape, 0);
        assert!(handle.pump(Duration::from_millis(2_200)));
        assert!(windows(&handle).is_empty());
        assert_eq!(status(&handle), "Next break in 30 min");
    }

    #[test]
    fn closing_a_break_window_or_switching_displays_is_handled() {
        let handle = started(Options::HIDDEN);
        if !idle(&handle) {
            return;
        }
        handle.post_command(Command::TakeBreak);
        assert!(handle.pump(Duration::from_millis(50)));
        let before = windows(&handle);
        assert!(!before.is_empty());
        send(handle.window(), WM_DISPLAYCHANGE, 0, 0);
        let after = windows(&handle);
        assert_eq!(after.len(), monitors().len());
        assert!(after.iter().all(|hwnd| !before.contains(hwnd)), "rebuilt");
        send(after[0], WM_CLOSE, 0, 0);
        assert!(handle.pump(Duration::from_millis(50)));
        assert!(windows(&handle).is_empty());
    }

    #[test]
    fn session_and_power_changes_suppress_breaks() {
        let handle = started(Options::HIDDEN);
        let main = handle.window();
        send(main, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize, 0);
        assert_eq!(status(&handle), "Waiting for your return");
        handle.with(App::take_break);
        assert!(
            windows(&handle).is_empty(),
            "a locked session suppresses a manual break"
        );
        handle.with(|app| {
            app.set_clock_offset(INTERVAL + 1);
            app.update();
        });
        assert!(windows(&handle).is_empty(), "and a due one");
        send(main, WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize, 0);
        assert_eq!(
            status(&handle),
            "Next break in 30 min",
            "unlocking starts a fresh interval"
        );
        send(main, WM_POWERBROADCAST, PBT_APMSUSPEND as usize, 0);
        assert_eq!(status(&handle), "Waiting for your return");
        send(main, WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
        assert_eq!(status(&handle), "Next break in 30 min");
        send(main, WM_WTSSESSION_CHANGE, 0, 0);
        assert_eq!(status(&handle), "Next break in 30 min");
    }

    #[test]
    fn menu_commands_pause_resume_and_quit() {
        let handle = started(Options::HIDDEN);
        handle.post_command(Command::TogglePause);
        assert!(handle.pump(Duration::from_millis(50)));
        assert_eq!(status(&handle), "Paused · 60 min left");
        assert!(
            handle
                .with(|app| app.controller().paused())
                .unwrap_or(false)
        );
        handle.post_command(Command::TogglePause);
        handle.post_command(Command::Updates);
        assert!(handle.pump(Duration::from_millis(50)));
        assert_eq!(status(&handle), "Next break in 30 min");
        handle.post_command(Command::Quit);
        assert!(!handle.pump(Duration::from_secs(1)));
    }

    #[test]
    fn system_notices_and_update_results_are_handled() {
        let handle = started(Options {
            tray: true,
            present: false,
        });
        let main = handle.window();
        send(main, WM_SETTINGCHANGE, 0, 0);
        send(main, TASKBAR_CREATED.load(Ordering::Relaxed), 0, 0);
        send(main, WM_AUDIO, 0, 0);
        send(main, WM_TRAY, 0, 0x200);
        // SAFETY: the hook callback ignores its arguments.
        unsafe { on_foreground(HWINEVENTHOOK::default(), 0, HWND::default(), 0, 0, 0, 0) };
        send(main, WM_TIMER, TIMER_DAILY, 0);
        assert!(handle.pump(Duration::from_millis(100)));
        handle.with(|app| app.updates = Some(Updates::Checking));
        send(main, WM_UPDATED, Outcome::Current as usize, 0);
        let label = handle.with(|app| app.updates.map(Updates::label)).flatten();
        assert_eq!(label, Some("Up to Date · Check Again"));
        handle.with(|app| app.updates = None);
        send(main, WM_CLOSE, 0, 0);
        assert!(!handle.pump(Duration::from_secs(1)));
    }

    #[test]
    fn busy_apps_defer_events_through_the_queue() {
        let handle = started(Options::HIDDEN);
        let main = handle.window();
        APP.with(|cell| {
            let _busy = cell.borrow_mut();
            send(main, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize, 0);
        });
        assert_eq!(status(&handle), "Next break in 30 min", "deferred");
        assert!(handle.pump(Duration::from_millis(50)));
        assert_eq!(status(&handle), "Waiting for your return");
    }

    #[test]
    fn events_survive_deferral_and_messages_decode() {
        let events = [
            Event::Tick,
            Event::Daily,
            Event::Displays,
            Event::Theme,
            Event::TaskbarCreated,
            Event::Dismiss,
            Event::Menu { x: -5, y: 1200 },
            Event::Audio,
            Event::Foreground,
            Event::Updated(Outcome::Installed),
            Event::Command(Command::Quit),
            Event::Quit,
        ];
        for event in events.into_iter().chain(CHANGES.map(Event::Change)) {
            let (tag, data) = event.pack();
            assert_eq!(Event::unpack(tag, data), Some(event));
        }
        assert_eq!(Event::unpack(99, 0), None);
        assert_eq!(Event::unpack(2, 99), None);
        assert_eq!(Event::unpack(11, 0), None);
        // SAFETY: none of these messages carry pointers.
        unsafe {
            let anchor = WPARAM(0xFFFB_0010);
            let menu = Some(Event::Menu { x: 16, y: -5 });
            for code in [NIN_SELECT, NIN_KEYSELECT, WM_CONTEXTMENU] {
                assert_eq!(
                    decode(WM_TRAY, anchor, LPARAM(isize::try_from(code).unwrap_or(0))),
                    menu
                );
            }
            assert_eq!(decode(WM_TRAY, anchor, LPARAM(0x200)), None);
            assert_eq!(
                decode(WM_TIMER, WPARAM(TIMER_UPDATE), LPARAM(0)),
                Some(Event::Tick)
            );
            assert_eq!(decode(WM_TIMER, WPARAM(9), LPARAM(0)), None);
            assert_eq!(
                decode(WM_DISMISS, WPARAM(0), LPARAM(0)),
                Some(Event::Dismiss)
            );
            assert_eq!(
                decode(WM_DPICHANGED, WPARAM(0), LPARAM(0)),
                Some(Event::Theme)
            );
            assert_eq!(decode(0, WPARAM(0), LPARAM(0)), None);
        }
    }
}
