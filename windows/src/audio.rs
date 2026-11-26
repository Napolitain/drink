//! Microphone activity from Core Audio capture sessions on every active
//! capture endpoint, including non-default headsets. Drink reads session
//! state only; it never opens an audio input.
#![allow(unsafe_code)]

use crate::controller::Input;
use crate::win::WindowId;
use callbacks::{Created, Devices, Events};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, DEVICE_STATE_ACTIVE, IAudioSessionControl, IAudioSessionEvents,
    IAudioSessionManager2, IAudioSessionNotification, IMMDeviceEnumerator, IMMNotificationClient,
    MMDeviceEnumerator, eCapture,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::Result;

/// Posts a change notice to the app window from Core Audio worker threads.
#[derive(Clone)]
struct Notify {
    window: WindowId,
    message: u32,
    /// Devices or sessions came or went, so subscriptions must be rebuilt.
    stale: Arc<AtomicBool>,
}

impl Notify {
    fn changed(&self, structural: bool) {
        if structural {
            self.stale.store(true, Ordering::Release);
        }
        self.window.post(self.message, 0);
    }
}

/// Core Audio callbacks. The `#[implement]` expansion in `windows-core`
/// declares `#[inline(always)]` helpers and casts references to pointers; the
/// allowance covers only that generated code.
#[allow(clippy::inline_always, clippy::ref_as_ptr)]
mod callbacks {
    use super::Notify;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Media::Audio::{
        AudioSessionDisconnectReason, AudioSessionState, DEVICE_STATE, EDataFlow, ERole,
        IAudioSessionControl, IAudioSessionEvents, IAudioSessionEvents_Impl,
        IAudioSessionNotification, IAudioSessionNotification_Impl, IMMNotificationClient,
        IMMNotificationClient_Impl,
    };
    use windows::core::{BOOL, GUID, PCWSTR, Ref, Result, implement};

    /// Capture endpoints were added, removed or changed state.
    #[implement(IMMNotificationClient)]
    pub(super) struct Devices(pub(super) Notify);

    impl IMMNotificationClient_Impl for Devices_Impl {
        fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
            self.0.changed(true);
            Ok(())
        }
        fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
            self.0.changed(true);
            Ok(())
        }
        fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
            self.0.changed(true);
            Ok(())
        }
        fn OnDefaultDeviceChanged(&self, _: EDataFlow, _: ERole, _: &PCWSTR) -> Result<()> {
            Ok(())
        }
        fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
            Ok(())
        }
    }

    /// A session started on an observed endpoint.
    #[implement(IAudioSessionNotification)]
    pub(super) struct Created(pub(super) Notify);

    impl IAudioSessionNotification_Impl for Created_Impl {
        fn OnSessionCreated(&self, _: Ref<IAudioSessionControl>) -> Result<()> {
            self.0.changed(true);
            Ok(())
        }
    }

    /// An observed session became active, inactive or disconnected.
    #[implement(IAudioSessionEvents)]
    pub(super) struct Events(pub(super) Notify);

    impl IAudioSessionEvents_Impl for Events_Impl {
        fn OnDisplayNameChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
            Ok(())
        }
        fn OnIconPathChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
            Ok(())
        }
        fn OnSimpleVolumeChanged(&self, _: f32, _: BOOL, _: *const GUID) -> Result<()> {
            Ok(())
        }
        fn OnChannelVolumeChanged(
            &self,
            _: u32,
            _: *const f32,
            _: u32,
            _: *const GUID,
        ) -> Result<()> {
            Ok(())
        }
        fn OnGroupingParamChanged(&self, _: *const GUID, _: *const GUID) -> Result<()> {
            Ok(())
        }
        fn OnStateChanged(&self, _: AudioSessionState) -> Result<()> {
            self.0.changed(false);
            Ok(())
        }
        fn OnSessionDisconnected(&self, _: AudioSessionDisconnectReason) -> Result<()> {
            self.0.changed(true);
            Ok(())
        }
    }
}

/// Capture-session observer. Create and use it on one COM-initialised thread.
pub struct Microphone {
    notify: Notify,
    devices: Option<(IMMDeviceEnumerator, IMMNotificationClient)>,
    managers: Vec<(IAudioSessionManager2, IAudioSessionNotification)>,
    sessions: Vec<(IAudioSessionControl, IAudioSessionEvents)>,
    needs_retry: bool,
}

impl Microphone {
    /// Observe capture sessions and post `message` to `window` on changes.
    pub fn new(window: WindowId, message: u32) -> Self {
        Self {
            notify: Notify {
                window,
                message,
                stale: Arc::new(AtomicBool::new(true)),
            },
            devices: None,
            managers: Vec::new(),
            sessions: Vec::new(),
            needs_retry: false,
        }
    }

    /// Whether a subscription failed, so changes could go unnoticed.
    pub const fn needs_retry(&self) -> bool {
        self.needs_retry
    }

    /// Read whether any capture session is active.
    pub fn sample(&mut self) -> Input {
        self.needs_retry = false;
        if self.devices.is_none() {
            let Ok(devices) = subscribe_devices(&self.notify) else {
                self.needs_retry = true;
                return Input::Unavailable;
            };
            self.devices = Some(devices);
        }
        let mut unavailable =
            self.notify.stale.swap(false, Ordering::AcqRel) && self.rebuild().is_err();
        let mut active = false;
        for (control, _) in &self.sessions {
            // SAFETY: `control` is a live COM interface owned by this thread.
            match unsafe { control.GetState() } {
                Ok(state) => active |= state == AudioSessionStateActive,
                Err(_) => unavailable = true,
            }
        }
        if unavailable {
            self.needs_retry = true;
            self.notify.stale.store(true, Ordering::Release);
        }
        if active {
            Input::Active
        } else if unavailable {
            Input::Unavailable
        } else {
            Input::Clear
        }
    }

    /// Subscribe to every session on every active capture endpoint.
    fn rebuild(&mut self) -> Result<()> {
        self.unsubscribe_sessions();
        let Some((enumerator, _)) = &self.devices else {
            return Ok(());
        };
        // SAFETY: the enumerator and every interface below are live COM
        // objects used on this thread. Callback objects are kept alongside
        // the interfaces they are registered with until unsubscribed.
        unsafe {
            let endpoints = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            for index in 0..endpoints.GetCount()? {
                let manager: IAudioSessionManager2 =
                    endpoints.Item(index)?.Activate(CLSCTX_ALL, None)?;
                // Enumerating first also starts session-created notifications.
                let list = manager.GetSessionEnumerator()?;
                let created: IAudioSessionNotification = Created(self.notify.clone()).into();
                self.needs_retry |= manager.RegisterSessionNotification(&created).is_err();
                self.managers.push((manager, created));
                for session in 0..list.GetCount()? {
                    let control = list.GetSession(session)?;
                    let events: IAudioSessionEvents = Events(self.notify.clone()).into();
                    self.needs_retry |= control.RegisterAudioSessionNotification(&events).is_err();
                    self.sessions.push((control, events));
                }
            }
        }
        Ok(())
    }

    fn unsubscribe_sessions(&mut self) {
        // SAFETY: each callback is unregistered from the interface it was
        // registered with, outside any Core Audio callback.
        unsafe {
            for (control, events) in self.sessions.drain(..) {
                let _ = control.UnregisterAudioSessionNotification(&events);
            }
            for (manager, created) in self.managers.drain(..) {
                let _ = manager.UnregisterSessionNotification(&created);
            }
        }
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        self.unsubscribe_sessions();
        if let Some((enumerator, client)) = self.devices.take() {
            // SAFETY: unregisters the client registered in `subscribe_devices`.
            let _ = unsafe { enumerator.UnregisterEndpointNotificationCallback(&client) };
        }
    }
}

fn subscribe_devices(notify: &Notify) -> Result<(IMMDeviceEnumerator, IMMNotificationClient)> {
    // SAFETY: the caller initialised COM on this thread. The client stays
    // alive with the enumerator until it is unregistered on drop.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let client: IMMNotificationClient = Devices(notify.clone()).into();
        enumerator.RegisterEndpointNotificationCallback(&client)?;
        Ok((enumerator, client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{HWND, PROPERTYKEY};
    use windows::Win32::Media::Audio::{AudioSessionDisconnectReason, ERole};
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::core::PCWSTR;

    fn microphone() -> Microphone {
        // SAFETY: initialises COM for this test thread.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Microphone::new(WindowId::of(HWND::default()), 0)
    }

    #[test]
    fn samples_capture_sessions_without_opening_an_input() {
        let mut microphone = microphone();
        let first = microphone.sample();
        // A runner without audio services reports Unavailable and retries.
        assert_eq!(first == Input::Unavailable, microphone.needs_retry());
        microphone.notify.changed(true);
        assert!(matches!(
            microphone.sample(),
            Input::Clear | Input::Active | Input::Unavailable
        ));
        assert!(!microphone.notify.stale.load(Ordering::Acquire) || microphone.needs_retry());
    }

    #[test]
    fn device_and_session_callbacks_request_a_fresh_sample() {
        let microphone = microphone();
        let stale = || microphone.notify.stale.swap(false, Ordering::AcqRel);
        let devices: IMMNotificationClient = Devices(microphone.notify.clone()).into();
        let created: IAudioSessionNotification = Created(microphone.notify.clone()).into();
        let events: IAudioSessionEvents = Events(microphone.notify.clone()).into();
        let none = std::ptr::null();
        stale();
        // SAFETY: calls the callbacks as Core Audio would, with null strings
        // and contexts that the implementations ignore.
        unsafe {
            assert!(devices.OnDeviceAdded(PCWSTR::null()).is_ok() && stale());
            assert!(devices.OnDeviceRemoved(PCWSTR::null()).is_ok() && stale());
            assert!(
                devices
                    .OnDeviceStateChanged(PCWSTR::null(), DEVICE_STATE_ACTIVE)
                    .is_ok()
                    && stale()
            );
            assert!(created.OnSessionCreated(None).is_ok() && stale());
            assert!(
                devices
                    .OnDefaultDeviceChanged(eCapture, ERole(0), PCWSTR::null())
                    .is_ok()
            );
            assert!(
                devices
                    .OnPropertyValueChanged(PCWSTR::null(), PROPERTYKEY::default())
                    .is_ok()
            );
            assert!(events.OnDisplayNameChanged(PCWSTR::null(), none).is_ok());
            assert!(events.OnIconPathChanged(PCWSTR::null(), none).is_ok());
            assert!(events.OnSimpleVolumeChanged(1.0, false, none).is_ok());
            assert!(events.OnChannelVolumeChanged(&[1.0], 0, none).is_ok());
            assert!(events.OnGroupingParamChanged(none, none).is_ok());
            assert!(events.OnStateChanged(AudioSessionStateActive).is_ok());
            assert!(!stale(), "state changes keep subscriptions");
            assert!(
                events
                    .OnSessionDisconnected(AudioSessionDisconnectReason(0))
                    .is_ok()
                    && stale()
            );
        }
    }
}
