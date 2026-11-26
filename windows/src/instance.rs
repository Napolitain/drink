//! Single-instance locks and the hand-off between a development session and
//! an installed release.
//!
//! A development or smoke session temporarily closes a running release. A
//! helper process inherits the session's lock and a handle to the session;
//! when the session exits, even by crashing, the helper reopens the release
//! and only then releases the lock, so the next session sees it.
#![allow(unsafe_code)]

use crate::cli::RESTORE_RELEASE;
use crate::flavor::{Flavor, RELEASE_IDENTIFIER};
use crate::win::{process_path, window_process};
use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_HANDLE_OPTIONS, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE,
    HANDLE_FLAG_INHERIT, HANDLE_FLAGS, HWND, LPARAM, SetHandleInformation, WAIT_ABANDONED,
    WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, ReleaseMutex,
    WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
use windows::core::{HSTRING, Owned, PCWSTR};

/// How long a development session waits for a release to quit.
const RELEASE_EXIT: u32 = 8_000;

/// Ownership of one build's per-session instance mutex.
pub struct Instance(Owned<HANDLE>);

impl Instance {
    /// Become the running instance of `identifier`, waiting up to `wait`
    /// for a previous instance to exit.
    #[must_use]
    pub fn acquire(identifier: &str, wait: Duration) -> Option<Self> {
        let name = HSTRING::from(format!("Local\\{identifier}"));
        // SAFETY: the name is a valid string and the handle is owned.
        let mutex = unsafe { Owned::new(CreateMutexW(None, false, &name).ok()?) };
        let wait = u32::try_from(wait.as_millis()).unwrap_or(INFINITE - 1);
        // SAFETY: waits on the live mutex. An abandoned mutex is owned too.
        let result = unsafe { WaitForSingleObject(*mutex, wait) };
        (result == WAIT_OBJECT_0 || result == WAIT_ABANDONED).then(|| Self(mutex))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: this thread owns the mutex it acquired.
        let _ = unsafe { ReleaseMutex(*self.0) };
    }
}

/// A running development or smoke session.
pub struct DevelopmentSession {
    _lock: File,
}

/// Start a development session: take the development lock, then close a
/// running release and arrange for it to reopen when this process exits.
///
/// # Errors
///
/// Another session is running, or the release did not quit.
pub fn begin_development_session() -> Result<DevelopmentSession, String> {
    let directory = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    begin(&directory, RELEASE_IDENTIFIER)
}

/// Take the lock in `directory` and hand over from a window of `release_class`.
fn begin(directory: &Path, release_class: &str) -> Result<DevelopmentSession, String> {
    let lock = development_lock(directory)
        .map_err(|error| format!("Cannot start another Drink development session: {error}"))?;
    if let Some(release) = find_window(release_class) {
        let pid = window_process(release);
        // SAFETY: opening for synchronisation has no side effects; the
        // handle is owned.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }
            .map(|handle| unsafe { Owned::new(handle) })
            .map_err(|error| format!("Cannot follow the running Drink: {error}"))?;
        let path = process_path(pid).ok_or("Cannot locate the running Drink")?;
        restore_on_exit(&lock, &path)?;
        // SAFETY: asks the release window to close; it may already be gone.
        let _ = unsafe { PostMessageW(Some(release), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        // SAFETY: waits on the owned process handle.
        if unsafe { WaitForSingleObject(*process, RELEASE_EXIT) } != WAIT_OBJECT_0 {
            return Err("Drink is still running. Development session cancelled.".into());
        }
    }
    Ok(DevelopmentSession { _lock: lock })
}

/// Ask running development and smoke sessions to quit, so a release that is
/// opened again takes over.
pub fn close_development_sessions() {
    for flavor in [Flavor::DEVELOPMENT, Flavor::SMOKE] {
        if let Some(window) = find_window(flavor.identifier) {
            // SAFETY: posting to another process's window has no memory effects.
            let _ = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
    }
}

/// Helper mode: wait for the development session to end, then reopen the
/// release. Returns the process exit code.
#[must_use]
pub fn restore_release(session: usize, lock: usize, release: &Path) -> u8 {
    let (session, lock) = (HANDLE(session as *mut _), HANDLE(lock as *mut _));
    // SAFETY: both handles were inherited from the development session for
    // this helper; invalid values make the calls fail harmlessly.
    unsafe {
        WaitForSingleObject(session, INFINITE);
        let _ = CloseHandle(session);
        // Keep the lock out of the release, then hold it until it has started.
        let _ = SetHandleInformation(lock, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
    }
    let started = Command::new(release).stdin(Stdio::null()).spawn().is_ok();
    // SAFETY: closes the inherited lock handle.
    let _ = unsafe { CloseHandle(lock) };
    u8::from(!started)
}

fn development_lock(directory: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(directory.join(format!("{}.lock", Flavor::DEVELOPMENT.identifier)))
}

/// Start the helper with inheritable handles to this process and the lock.
fn restore_on_exit(lock: &File, release: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let session = inheritable(Source::CurrentProcess)?;
    let lock = inheritable(Source::Handle(HANDLE(lock.as_raw_handle())))?;
    let spawned = Command::new(exe)
        .arg(RESTORE_RELEASE)
        .arg((session.0 as usize).to_string())
        .arg((lock.0 as usize).to_string())
        .arg(release)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    // SAFETY: the helper holds its own copies now.
    unsafe {
        let _ = CloseHandle(session);
        let _ = CloseHandle(lock);
    }
    spawned
        .map(drop)
        .map_err(|error| format!("Cannot arrange to reopen Drink: {error}"))
}

/// What to duplicate for the helper.
#[derive(Clone, Copy)]
enum Source {
    /// This process, so the helper can wait for it to exit.
    CurrentProcess,
    /// An open handle, duplicated with the same access.
    Handle(HANDLE),
}

/// An inheritable duplicate of `source`.
fn inheritable(source: Source) -> Result<HANDLE, String> {
    let mut duplicate = HANDLE::default();
    // SAFETY: duplicates a live handle within this process.
    unsafe {
        let current = GetCurrentProcess();
        let (source, access, options) = match source {
            Source::CurrentProcess => (
                current,
                PROCESS_SYNCHRONIZE.0,
                DUPLICATE_HANDLE_OPTIONS::default(),
            ),
            Source::Handle(handle) => (handle, 0, DUPLICATE_SAME_ACCESS),
        };
        DuplicateHandle(
            current,
            source,
            current,
            &raw mut duplicate,
            access,
            true,
            options,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(duplicate)
}

fn find_window(class: &str) -> Option<HWND> {
    // SAFETY: searches top-level windows by class name.
    unsafe { FindWindowW(&HSTRING::from(class), PCWSTR::null()) }.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_instance_per_identifier() {
        let id = format!(
            "io.github.napolitain.drink.test-instance-{}",
            std::process::id()
        );
        let first = Instance::acquire(&id, Duration::ZERO);
        assert!(first.is_some());
        let other = id.clone();
        let blocked = std::thread::spawn(move || {
            Instance::acquire(&other, Duration::from_millis(50)).is_some()
        });
        assert!(!blocked.join().unwrap_or(true));
        drop(first);
        let other = id;
        let next =
            std::thread::spawn(move || Instance::acquire(&other, Duration::from_secs(1)).is_some());
        assert!(next.join().unwrap_or(false));
    }

    #[test]
    fn one_development_session_at_a_time() {
        let directory =
            std::env::temp_dir().join(format!("drink-session-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&directory);
        let release = "io.github.napolitain.drink.test-no-release";
        let first = begin(&directory, release);
        assert!(first.is_ok());
        let second = begin(&directory, release).err().unwrap_or_default();
        assert!(
            second.starts_with("Cannot start another Drink development session"),
            "{second}"
        );
        drop(first);
        assert!(begin(&directory, release).is_ok());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn closing_absent_sessions_and_restoring_with_invalid_handles_is_harmless() {
        close_development_sessions();
        assert_eq!(
            find_window("io.github.napolitain.drink.test-no-such-window"),
            None
        );
        assert_eq!(
            restore_release(0, 0, Path::new("Z:\\missing\\Drink.exe")),
            1
        );
        let duplicate = inheritable(Source::CurrentProcess);
        assert!(duplicate.is_ok());
        if let Ok(handle) = duplicate {
            // SAFETY: closes the duplicate made above.
            let _ = unsafe { CloseHandle(handle) };
        }
    }
}
