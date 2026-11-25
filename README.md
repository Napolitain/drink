# Drink

Drink is a planned desktop reminder for macOS, Windows and Linux. Every 30 minutes it covers the displays for 30 seconds so you can get water, blink and look away. When a reminder falls during a call or game, it waits for a suitable moment. KDE Plasma is the first Linux target.

## What a break feels like

One quiet, opaque, borderless window per display shows “Get some water. Look away and blink.” The windows close automatically, even if you walk away. The app runs in the user’s desktop session and needs no main window. A small status menu shows the next break or the reason it is waiting, with actions to take a break now, pause for an hour, resume or quit.

Proposed defaults for the proof of concept (PoC):

- Use a monotonic deadline. Keep at most one break pending. Start the next interval when the displayed break ends.
- If a protected activity is already under way when the break comes due, wait until all known blockers have cleared for 10 continuous seconds. If another blocker starts during those seconds, start the grace period again when it ends.
- Skip interruptions while the desktop is locked or asleep. Start a fresh interval on unlock or wake. A break interrupted by a newly active microphone or game should close promptly and become pending again.
- Ordinary clicks and typing should land on the overlay. Holding Escape for two seconds should end the break early. Keep OS shortcuts available and restore focus to the previous app if the user has not switched elsewhere.

For example, a break due at 10:30 during a call that ends at 10:47 appears at 10:47:10. The overlay closes at 10:47:40 and the next break is due at 11:17:40.

## Platform design

Use one Rust scheduling core. It consumes clock, session, and activity events and decides when to show or close the overlay. The desktop adapter owns the windows, menu and activity observations. Each detector reports active, clear or unavailable, plus a reason the menu can display. An unavailable detector leaves the timer usable. A manual pause covers cases such as listening to a meeting with the microphone disconnected.

Start with an event loop that sleeps until a deadline or an OS event. A native window library such as winit can wait on either. Choose the renderer after testing whether a static message can be drawn and focused reliably on all displays. Measure idle CPU, wakeups, resident memory and installed size before adding a larger toolkit. [winit event loop](https://docs.rs/winit/latest/winit/event_loop/enum.ControlFlow.html)

| Target | PoC integration to validate |
| --- | --- |
| KDE Plasma on Wayland | Put PipeWire microphone detection in the shared Linux adapter. Follow active capture links to microphone sources, including headsets. Exclude speaker-monitor streams. Request a fullscreen surface on each output. Keep Plasma-specific placement or foreground-game detection in `linux/kde`. A small KWin script is an option if the compositor needs to supply focused-window information. [WirePlumber routing](https://pipewire.pages.freedesktop.org/wireplumber/policies/linking.html) · [Wayland fullscreen requests](https://wayland.app/protocols/xdg-shell#xdg_toplevel:request:set_fullscreen) · [KWin scripting API](https://develop.kde.org/docs/plasma/kwin/api/) |
| Windows | Inspect audio sessions on active capture endpoints, including non-default devices. Use the shell's fullscreen Direct3D and presentation signals to defer. Match the foreground executable against configured games for borderless or windowed titles. Opening Steam alone should allow breaks. [Audio session states](https://learn.microsoft.com/en-us/windows/win32/api/audiosessiontypes/ne-audiosessiontypes-audiosessionstate) · [Shell notification states](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state) |
| macOS | Check Core Audio input activity and associated devices. Use borderless windows in the current Space, including when another app is fullscreen. Test headset changes and multiple displays with separate Spaces. Allow configured foreground game apps to defer. [Core Audio input activity](https://developer.apple.com/documentation/coreaudio/kaudioprocesspropertyisrunninginput) · [AppKit window behaviors](https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct) |

GNOME and Hyprland can reuse the Linux audio and scheduling work. Add `linux/gnome` or `linux/hyprland` only when there is desktop-specific behavior to implement and a session to test it on. Start on Plasma Wayland. Verify X11 separately before claiming it works. Across all platforms, read activity metadata without opening the microphone. Mic activity can persist while a conferencing app is muted, or disappear while the user is still in the meeting.

## Second pass — screen recording and sharing

Recording and sharing are separate reasons to defer, using the same scheduling rules as microphone use. They are outside the PoC. The target includes silent recordings and screen shares where microphone detection cannot help.

| Use case | Design and acceptance check |
| --- | --- |
| OBS Studio | When the user enables its local WebSocket integration, query recording and streaming state on connect and reconnect, then follow state-change events. Defer during either output, including a paused recording session. An open OBS preview should allow reminders. Test starting Drink midway through an OBS recording and stopping the last active output. [OBS WebSocket protocol](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md#getrecordstatus) |
| FFmpeg screen capture | FFmpeg can record the desktop with inputs such as Windows `gdigrab`, Linux X11 `x11grab`, Linux DRM/KMS `kmsgrab` where accessible, and a macOS AVFoundation screen source. Target direct launches for identifiable display inputs. Prefer an observable active capture session. Where the platform exposes only same-user invocation metadata, match a screen input and its source, account for multiple inputs and treat ambiguous sources as unknown. Also design a command-scoped inhibitor, e.g. `drink inhibit -- ffmpeg …`, for pipelines whose capture source belongs to another process. Release it on process exit or failure. Test a silent screen capture and an ordinary file transcode side by side: only the capture should defer. [FFmpeg input devices](https://ffmpeg.org/ffmpeg-devices.html) |
| Screen sharing | On Plasma, investigate whether KWin or the portal backend exposes a share-active signal to ordinary applications. The public ScreenCast portal describes sessions created by the requesting client. Its API alone does not give Drink a list of other apps’ shares. Test a browser sharing a single window with its mic off, then ending the share while keeping the browser open. Verify equivalent observers separately on macOS and Windows. For unobservable paths, show the limitation and offer manual pause. [ScreenCast portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html) |

## How we will verify the PoC

Use a fake clock for the shared scheduler and property-test overlapping inhibitors, grace-period resets, sleep/wake and the one-pending-break rule. In each target desktop session, verify visible behavior with a short test interval and again with the 30-second break. Record the OS, compositor or windowing mode, hardware and result.

- Check two displays, a fullscreen editor, and typing as a break appears. Confirm the editor receives no hidden keystrokes, the overlay disappears on time, and focus returns sensibly. Check deliberate early dismissal.
- Start a call on a non-default headset before a deadline. Mute, switch input, then end the call. Record when the OS reports capture and confirm exactly one pending break appears once it clears.
- Start Steam alone, then try an actual game in fullscreen and borderless modes. Check that a brief Alt-Tab does not trigger a pending break and that returning to work does.
- Lock or suspend across a deadline. Confirm waking starts a fresh interval. Measure idle behavior with the overlay hidden.

## Development baseline

The starting template is [Napolitain/template-rust at f60084d](https://github.com/Napolitain/template-rust/tree/f60084dd74f9c80f6985059ab37a1f7f6a3ad5ad), selected from the local `../templates` checkout. The planning repository adopts its `AGENTS.md`, `.editorconfig` and `prek.toml`. Install hooks with `prek install` and run `prek run --all-files` to check the current files.

At the first code milestone, bring over the template's Cargo manifest, pinned toolchain, lint and release profiles, property tests, coverage and mutation workflows. Replace the example with scheduler tests. Keep the template's 80% line-coverage gate, and extend build checks to macOS and Windows. Use portable CPU targets for distributable builds. If native integration needs another language, adopt the corresponding template for that component where available. Desktop support requires the interactive checks above in addition to a successful build.
