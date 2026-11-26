//! Interactive Windows smoke test. It only supplies a clock offset: the app
//! samples real activity, schedules real timers and shows real windows, and it
//! briefly takes keyboard focus. Exit code 77 means this session could not run
//! the visual checks.
#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    match smoke::run() {
        Ok(smoke::Finish::Passed) => ExitCode::SUCCESS,
        Ok(smoke::Finish::Skipped) => ExitCode::from(77),
        Err(message) => {
            eprintln!("FAIL: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("The Windows smoke test runs only on Windows.");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
mod smoke {
    use drink_core::{BREAK, INTERVAL};
    use drink_windows::app::{self, App, Handle, Options};
    use drink_windows::controller::Input;
    use drink_windows::flavor::Flavor;
    use drink_windows::instance;
    use drink_windows::overlay::{HOLD_MS, Overlay, WM_DISMISS, monitors};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GetForegroundWindow,
        GetWindowLongPtrW, GetWindowRect, IsWindow, IsWindowVisible, RegisterClassW, SendMessageW,
        WINDOW_EX_STYLE, WM_KEYDOWN, WM_KEYUP, WNDCLASSW, WS_EX_LAYERED, WS_POPUP,
    };
    use windows::core::w;

    /// How the smoke test ended without failing.
    pub enum Finish {
        Passed,
        Skipped,
    }

    static DISMISSED: AtomicU32 = AtomicU32::new(0);

    fn require(condition: bool, message: &str) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(message.into())
        }
    }

    fn rect(hwnd: HWND) -> RECT {
        let mut rect = RECT::default();
        // SAFETY: writes the bounds of a window into a local.
        let _ = unsafe { GetWindowRect(hwnd, &raw mut rect) };
        rect
    }

    fn visible(hwnd: HWND) -> bool {
        // SAFETY: queries a window owned by this thread.
        unsafe { IsWindowVisible(hwnd).as_bool() }
    }

    fn opaque(hwnd: HWND) -> bool {
        // SAFETY: reads the extended style of a window owned by this thread.
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        style & isize::try_from(WS_EX_LAYERED.0).unwrap_or(0) == 0
    }

    fn foreground() -> Option<HWND> {
        // SAFETY: no arguments.
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.is_invalid()).then_some(hwnd)
    }

    fn escape(hwnd: HWND, message: u32) {
        // SAFETY: delivers a key message to a window owned by this thread.
        unsafe {
            SendMessageW(
                hwnd,
                message,
                Some(WPARAM(usize::from(VK_ESCAPE.0))),
                Some(LPARAM(0)),
            );
        }
    }

    fn windows(handle: &Handle) -> Vec<HWND> {
        handle
            .with(|app| app.break_windows().to_vec())
            .unwrap_or_default()
    }

    fn pump(handle: &Handle, ms: u64) -> Result<(), String> {
        require(
            handle.pump(Duration::from_millis(ms)),
            "the app quit unexpectedly",
        )
    }

    unsafe extern "system" fn count_dismissals(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_DISMISS {
            DISMISSED.fetch_add(1, Ordering::Relaxed);
            return LRESULT(0);
        }
        // SAFETY: default handling on the window's own thread.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    /// A hidden owner that counts dismissals of detached break windows.
    fn counting_owner() -> Result<HWND, String> {
        let class = WNDCLASSW {
            lpfnWndProc: Some(count_dismissals),
            lpszClassName: w!("io.github.napolitain.drink.smoke.owner"),
            ..Default::default()
        };
        // SAFETY: registers a static class and creates a hidden window of it.
        unsafe {
            RegisterClassW(&raw const class);
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!("DrinkSmoke owner"),
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
        }
        .map_err(|error| error.to_string())
    }

    pub fn run() -> Result<Finish, String> {
        let _session = instance::begin_development_session()?;
        let handle =
            app::start(Flavor::SMOKE, Options::INTERACTIVE).map_err(|error| error.to_string())?;
        let started = handle.with(|app| app.now()).ok_or("the app is busy")?;
        detached_checks(&handle)?;
        session_checks(&handle, started)
    }

    fn detached_checks(handle: &Handle) -> Result<(), String> {
        let owner = counting_owner()?;
        let mut detached = Overlay::new(owner, false);
        require(detached.show(), "break windows can be created")?;
        let first = *detached.windows().first().ok_or("no break window")?;
        let screen = *monitors()
            .first()
            .ok_or("smoke test needs a connected display")?;
        require(
            rect(first) == screen && !visible(first) && opaque(first),
            "an offscreen break window matches the primary display and is opaque",
        )?;
        let hold = u64::from(HOLD_MS) + 100;
        escape(first, WM_KEYDOWN);
        escape(first, WM_KEYUP);
        pump(handle, hold)?;
        require(
            DISMISSED.load(Ordering::Relaxed) == 0,
            "tapping Escape should keep the break open",
        )?;
        escape(first, WM_KEYDOWN);
        pump(handle, hold)?;
        require(
            DISMISSED.load(Ordering::Relaxed) == 1,
            "holding Escape should dismiss once",
        )?;
        escape(first, WM_KEYUP);
        drop(detached);
        // SAFETY: destroys the owner created above.
        let _ = unsafe { DestroyWindow(owner) };
        println!("PASS: offscreen frame and opacity, Escape tap and two-second hold");
        Ok(())
    }

    fn session_checks(handle: &Handle, started: u64) -> Result<Finish, String> {
        pump(handle, 300)?;
        let (microphone, present, blocked) = handle
            .with(|app| {
                app.update();
                let seen = app.observations();
                (seen.microphone, seen.present(), seen.blocked())
            })
            .ok_or("the app is busy")?;
        if microphone == Input::Active {
            eprintln!("Microphone in use; skipping interactive overlay test.");
            return Ok(Finish::Skipped);
        }
        require(
            microphone == Input::Clear,
            "Core Audio should report an available idle capture state",
        )?;
        require(windows(handle).is_empty(), "launch should be quiet")?;
        if !present {
            handle.with(|app| {
                app.set_clock_offset(INTERVAL + 1);
                app.update();
                app.take_break();
            });
            require(
                windows(handle).is_empty(),
                "an absent session suppresses even a manual break",
            )?;
            println!(
                "PASS: Core Audio idle, launch quiet, locked or inactive session suppressed a due and manual break."
            );
            println!("SKIP: visible overlay checks require an unlocked desktop.");
            return Ok(Finish::Skipped);
        }
        if blocked {
            println!("SKIP: a protected foreground app or full-screen state is active.");
            return Ok(Finish::Skipped);
        }
        let previous = foreground();
        handle.with(|app| {
            let elapsed = app.now() - started;
            app.set_clock_offset(INTERVAL - 1_000 - elapsed);
            app.update();
        });
        pump(handle, 1_100)?;
        verify_break(handle)?;
        handle.with(|app| {
            app.set_clock_offset(app.clock_offset() + BREAK - 1_000);
            app.update();
        });
        pump(handle, 1_100)?;
        require(
            windows(handle).is_empty(),
            "the timer should close every window",
        )?;
        // SAFETY: IsWindow tolerates stale handles.
        if let Some(previous) = previous.filter(|hwnd| unsafe { IsWindow(Some(*hwnd)) }.as_bool()) {
            require(
                foreground() == Some(previous),
                "automatic dismissal returns focus to the prior app",
            )?;
            println!("Prior foreground app regained focus");
        }
        handle.with(App::take_break);
        pump(handle, 300)?;
        require(
            windows(handle).iter().any(|hwnd| visible(*hwnd)),
            "Take break now opens the overlay",
        )?;
        handle.with(App::toggle_pause);
        require(windows(handle).is_empty(), "Pause closes the overlay")?;
        handle.with(App::toggle_pause);
        require(windows(handle).is_empty(), "Resume starts a fresh interval")?;
        println!("Automatic dismissal, Take break now, Pause and Resume passed");
        Ok(Finish::Passed)
    }

    fn verify_break(handle: &Handle) -> Result<(), String> {
        let shown = windows(handle);
        let screens = monitors();
        require(
            !shown.is_empty() && shown.len() == screens.len(),
            "one window per display",
        )?;
        for screen in &screens {
            require(
                shown
                    .iter()
                    .any(|hwnd| visible(*hwnd) && opaque(*hwnd) && rect(*hwnd) == *screen),
                "an opaque window covers each display",
            )?;
        }
        require(
            foreground().is_some_and(|hwnd| shown.contains(&hwnd)),
            "the break owns keyboard focus",
        )?;
        println!(
            "Core Audio idle, covered {} display(s), keyboard focus acquired",
            shown.len()
        );
        Ok(())
    }
}
