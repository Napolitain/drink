//! Single-instance locks.
#![allow(unsafe_code)]

use std::time::Duration;
use windows::Win32::Foundation::{HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateMutexW, INFINITE, ReleaseMutex, WaitForSingleObject,
};
use windows::core::{HSTRING, Owned};

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
}
