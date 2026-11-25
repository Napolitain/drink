# Drink

Drink is a water-break reminder. The first working adapter is a native macOS menu-bar app. Windows and Linux adapters are planned. Every 30 minutes it covers the displays for 30 seconds so you can get water, blink and look away. When a reminder falls during a call or game, it waits for a suitable moment. KDE Plasma is the first Linux target.

## Run on macOS

Build on macOS 14.2 or newer with Apple Command Line Tools and Rust installed through rustup. This produces an ad-hoc signed app for the Mac that built it.

```sh
macos/build
open target/Drink.app
```

Click the water drop in the menu bar for **Take break now**, **Pause for 1 hour**, **Resume** or **Quit Drink**. Take break now respects protected activities. Hold Escape for two seconds to end a visible break early. To launch at login, copy the bundle to Applications and add Drink under System Settings → General → Login Items.

The app observes Core Audio input activity, including virtual inputs, without recording audio. It recognizes foreground apps with a macOS game category. To protect an uncategorized game, append its bundle identifier and relaunch Drink:

```sh
defaults write io.github.napolitain.drink ProtectedGameBundleIDs -array-add com.example.game
```

## What a break feels like

One quiet, opaque, borderless window per display shows “Get some water. Look away and blink.” The windows close automatically, even if you walk away. The app runs in the user’s desktop session and needs no main window. A small status menu shows the next break or the reason it is waiting, with actions to take a break now, pause for an hour, resume or quit.

The proof of concept (PoC) uses the following defaults.

- Use a monotonic deadline. Keep at most one break pending. Start the next interval when the displayed break ends.
- If a protected activity is already under way when the break comes due, wait until all known blockers have cleared for 10 continuous seconds. If another blocker starts during those seconds, start the grace period again when it ends.
- Skip interruptions while the desktop is locked or asleep. Start a fresh interval on unlock or wake. A break interrupted by a newly active microphone or game should close promptly and become pending again.
- Ordinary clicks and typing should land on the overlay. Holding Escape for two seconds should end the break early. Keep OS shortcuts available and restore focus to the previous app if the user has not switched elsewhere.

For example, a break due at 10:30 during a call that ends at 10:47 appears at 10:47:10. The overlay closes at 10:47:40 and the next break is due at 11:17:40.

## Platform design

Use one Rust scheduling core. It consumes clock, session, and activity events and decides when to show or close the overlay. The desktop adapter owns the windows, menu and activity observations. Each detector reports active, clear or unavailable, plus a reason the menu can display. An unavailable detector leaves the timer usable. A manual pause covers cases such as listening to a meeting with the microphone disconnected.

The Mac adapter uses the AppKit run loop, one-shot timers and native display, session, foreground-app and Core Audio notifications. A failed audio subscription is retried. Objective-C++ calls the shared Rust scheduler through a small CXX bridge. The rendered message uses AppKit text controls. The core has zero runtime crate dependencies.

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

### First macOS run

On an Apple Silicon Mac running macOS 27.2.1, the release app launched in the menu-bar session. The native smoke test verified a display-sized opaque window offscreen, a short Escape tap, a two-second Escape hold, an idle Core Audio reading and suppression of a due or manual break while locked. In the unlocked session, the accelerated timer covered the connected display, acquired keyboard focus, dismissed the overlay automatically and restored the previous foreground app. Take break now, Pause and Resume passed in the same session. A real call or headset change, a game, multiple monitors and fullscreen Spaces still need interactive verification.

## Development baseline

The Rust core follows [template-rust at f60084d](https://github.com/Napolitain/template-rust/tree/f60084dd74f9c80f6985059ab37a1f7f6a3ad5ad), including its toolchain, lint and release profiles, proptest, coverage and mutation testing. The macOS adapter uses the formatting rules from `template-cpp`. CXX generates the bridge in its own crate, keeping native dependencies out of the core. The default Cargo package is the core. CI is configured to test it on Linux, macOS and Windows and build the native app on macOS.

```sh
prek install
cargo test --locked
prek run --all-files
prek run --all-files --hook-stage pre-push
prek run --hook-stage manual cargo-mutants --all-files
```

Coverage requires cargo-llvm-cov and the pinned llvm-tools component. Mutation testing requires cargo-mutants. The interactive `macos/build --smoke-test` command launches a separate test bundle and briefly takes keyboard focus. It advances the clock to exercise the same timer, menu actions and windows as the real app. A protected session can skip the visual checks (exit 77).
