# Drink

Drink is a water-break reminder. Native adapters run in the macOS menu bar and the Windows 11 notification area. A Linux adapter is planned. Every 30 minutes it covers the displays for 30 seconds so you can get water, blink and look away. When a reminder falls during a call or game, it waits for a suitable moment. KDE Plasma is the first Linux target.

## Run on macOS

Drink runs on macOS 14.2 or newer. Download the universal `Drink-macOS.zip` from the [latest GitHub release](https://github.com/Napolitain/drink/releases/latest), unzip it, and move Drink.app to a writable Applications folder such as `~/Applications`. The release supports Apple Silicon and Intel. It is ad-hoc code signed and has no Apple notarization ticket.

Release builds check at launch and daily. The current source downloads public release assets over HTTPS using the curl supplied with macOS. The first release, `v2026.11.25`, was built while the repository was private and uses a signed-in GitHub CLI for its own update checks. Drink checks the archive's Ed25519 signature before extraction, checks the bundle identity, version and macOS code signature, and exchanges the bundles atomically. The new version launches after the next completed break. **Restart to Finish Update** applies it sooner. If a check fails, the menu offers a retry.

To switch to a development build, run the following command from the checkout with Apple Command Line Tools and Rust installed:

```sh
macos/build --dev
```

**Drink Dev** uses its own preferences and has no release updates. On launch, it temporarily quits a running release. Choose **Quit Drink Dev** to reopen that same release in the background. You can also switch back by opening the installed Drink app. The release starts a fresh 30-minute interval. If it was already closed, it stays closed. The interactive smoke test uses the same handoff. Only one development or smoke session runs at a time. To build without switching, run `macos/build`; opening `target/Drink Dev.app` later performs the handoff.

Click the water drop in the menu bar for **Take break now**, **Pause for 1 hour**, **Resume** or **Quit Drink**. Take break now respects protected activities. Hold Escape for two seconds to end a visible break early. To launch at login, copy the bundle to Applications and add Drink under System Settings → General → Login Items.

The app observes Core Audio input activity, including virtual inputs, without recording audio. It recognizes foreground apps with a macOS game category. To protect an uncategorized game, append its bundle identifier and relaunch Drink:

```sh
defaults write io.github.napolitain.drink ProtectedGameBundleIDs -array-add com.example.game
```

## Run on Windows

Drink runs on Windows 11. Download `Drink-Windows-x64.zip` from the [latest GitHub release](https://github.com/Napolitain/drink/releases/latest), extract `Drink.exe` and move it to a folder you can write to, such as `%LOCALAPPDATA%\Programs\Drink`. Windows on Arm runs the x64 build through emulation. Releases after `v2026.11.25` include the Windows archive. `Drink.exe` has no Authenticode signature, so SmartScreen asks for confirmation on first launch (**More info → Run anyway**). Smart App Control blocks unsigned apps while it is on.

Release builds check for updates at launch and daily. They use the `curl.exe`, `tar.exe` and OpenSSH `ssh-keygen.exe` that ship with Windows. Drink checks the archive's Ed25519 signature before extraction. The signature namespace names the architecture and version, so a genuine archive cannot be replayed as another release. The running `Drink.exe` is renamed aside and the new one takes its place. The new version launches after the next completed break, and **Restart to Finish Update** applies it sooner. If a check fails, the menu offers a retry.

To switch to a development build, run the following command from the checkout with Rust, the Visual Studio C++ build tools and PowerShell 7.3 or newer installed:

```powershell
windows\build.ps1 -Dev
```

**Drink Dev** uses its own preferences and has no release updates. On launch, it temporarily closes a running release. Choose **Quit Drink Dev** to reopen that same release; it also reopens if Drink Dev crashes. Opening the installed Drink closes Drink Dev. The release starts a fresh 30-minute interval, and if it was already closed, it stays closed. Only one development or smoke session runs at a time. To build without switching, run `windows\build.ps1`; opening `target\Drink Dev.exe` later performs the handoff.

Click the water drop in the notification area for the same menu as on macOS. Windows 11 may first place it in the overflow area; to keep it visible, turn it on under Settings → Personalization → Taskbar → Other system tray icons. Hold Escape for two seconds to end a visible break early. Switching to another app, or Alt+F4, also ends it. To launch at login, add a shortcut to `Drink.exe` in the folder that `shell:startup` opens.

The app reads the state of audio sessions on every active capture device, including headsets that are not the default, without opening the microphone. Exclusive full-screen Direct3D apps and presentation mode defer breaks. Drink also treats an app as a game when the Xbox Game Bar has recognised it, or when you list it. When a break comes due while a game is in front, Drink checks whether you are playing: the break waits while you use both the keyboard and the mouse, and shows once 30 seconds pass without both, such as on the score screen or while you chat. To check, Drink learns only which device each input came from, never which key or button. To add a game the Game Bar missed, add its executable name; Drink rereads both lists every ten minutes. Drink Dev reads the same value under `io.github.napolitain.drink.dev`.

```powershell
$key = 'HKCU:\Software\io.github.napolitain.drink'
if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
$games = @((Get-ItemProperty -Path $key).ProtectedGameExecutables) + 'game.exe'
Set-ItemProperty -Path $key -Name ProtectedGameExecutables -Type MultiString -Value ($games | Where-Object { $_ })
```

## Weekly releases

GitHub Actions checks `main` every Monday at 08:17 UTC. If the commit differs from the last published release and descends from it, CI tests the core and both adapters, builds the universal Mac app and the x64 Windows executable, signs each ZIP and publishes all four assets together. The UTC release date supplies the tag, for example `v2026.11.25`. An unchanged week produces no release. A manual dispatch follows the same rules, including at most one release per UTC date. Installed apps accept only `vYYYY.MM.DD` tags, so a second fix on the same day ships under the next date; a suffix such as `v2026.11.26.1` would make every existing install reject the latest release. [Weekly release workflow](https://github.com/Napolitain/drink/actions/workflows/release.yml)

The release signing key is stored in the repository Actions secret `DRINK_RELEASE_SIGNING_KEY`. The matching public key is in `macos/release-signers`. Keep the signer available across releases. To verify a downloaded archive using a trusted checkout:

```sh
ssh-keygen -Y verify -f macos/release-signers -I drink -n drink-macos \
    -s Drink-macOS.zip.sig < Drink-macOS.zip
```

The Windows namespace includes the release version. For example, in PowerShell:

```powershell
cmd /c 'ssh-keygen -Y verify -f macos\release-signers -I drink -n drink-windows-x64-2026.12.07 -s Drink-Windows-x64.zip.sig < Drink-Windows-x64.zip'
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

The Windows adapter is Rust built on Microsoft's `windows` crate, so it calls the core directly. A hidden window owns a one-shot timer, the notification-area icon, session and display-power notifications, a foreground-change hook and Core Audio callbacks. Callbacks from other threads post messages to that window, so every decision runs on one thread. Portable policy, menu text and icon artwork are tested on every platform. Unsafe code is confined to the Win32 modules, and each block states why it is sound. Windows lets only the process with the latest input take keyboard focus. A timed break therefore sends an empty mouse input, which neither moves nor clicks, before taking focus. If Windows still refuses, nothing is shown and Drink retries. The message uses GDI text scaled for each display's DPI. Games come from your list and from the paths the Xbox Game Bar records under `HKCU\System\GameConfigStore`, matched by full path so that a generic host such as `javaw.exe` only counts in a game's own folder. While a due break waits on a game in front, Drink registers for Raw Input and requests only each event's header, which names the device type; key codes and button data never reach it. It stops listening as soon as the break shows or the game leaves the foreground. If the registration fails, a game in front keeps deferring the break.

| Target | PoC integration to validate |
| --- | --- |
| KDE Plasma on Wayland | Put PipeWire microphone detection in the shared Linux adapter. Follow active capture links to microphone sources, including headsets. Exclude speaker-monitor streams. Request a fullscreen surface on each output. Keep Plasma-specific placement or foreground-game detection in `linux/kde`. A small KWin script is an option if the compositor needs to supply focused-window information. [WirePlumber routing](https://pipewire.pages.freedesktop.org/wireplumber/policies/linking.html) · [Wayland fullscreen requests](https://wayland.app/protocols/xdg-shell#xdg_toplevel:request:set_fullscreen) · [KWin scripting API](https://develop.kde.org/docs/plasma/kwin/api/) |
| Windows | Inspect audio sessions on active capture endpoints, including non-default devices. Use the shell's fullscreen Direct3D and presentation signals to defer. Match the foreground executable against configured games for borderless or windowed titles. Opening Steam alone should allow breaks. Test typing into an editor as a break appears, headset changes and per-monitor DPI on multiple displays. [Audio session states](https://learn.microsoft.com/en-us/windows/win32/api/audiosessiontypes/ne-audiosessiontypes-audiosessionstate) · [Shell notification states](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state) |
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

### First Windows run

On an x64 PC with one display running Windows 11 Pro 10.0.26300, the native smoke test verified an opaque, display-sized break window created offscreen, a short Escape tap, a two-second Escape hold and an idle Core Audio reading. In the unlocked session, the accelerated timer covered the display, acquired keyboard focus, closed the break automatically and returned focus to the previous foreground app. Take break now, Pause and Resume passed in the same session. With real processes, a second release exited at once and Drink Dev closed the running release. Quitting Drink Dev, or ending its process abruptly, reopened the release. Opening the release closed Drink Dev, and a second development session was refused. Against the GitHub release page, a `v2026.11.25` build reported that it was up to date. An older build found that tag, failed on the missing Windows archive and left its executable unchanged. Signed installs, tampered archives, replayed versions and architectures, and an exchange over a running executable are covered by tests that use a disposable key. A real call or headset change, a game in each window mode, multiple monitors with mixed DPI, typing into an editor as a break appears, and locking or sleeping across a deadline still need interactive verification.

## Development baseline

The Rust core follows [template-rust at f60084d](https://github.com/Napolitain/template-rust/tree/f60084dd74f9c80f6985059ab37a1f7f6a3ad5ad), including its toolchain, lint and release profiles, proptest, coverage and mutation testing. The macOS adapter uses the formatting rules from `template-cpp`. CXX generates the bridge in its own crate, keeping native dependencies out of the core. The default Cargo packages are the core and the Windows adapter, whose Win32 modules compile only on Windows. CI is configured to test the core on Linux, macOS and Windows, lint and test the Windows adapter on Windows, and build the native apps on macOS and Windows. Windows builds link the C runtime statically, so `Drink.exe` needs no Visual C++ Redistributable.

```sh
prek install
cargo test --locked
prek run --all-files
prek run --all-files --hook-stage pre-push
prek run --hook-stage manual cargo-mutants --all-files
```

Coverage requires cargo-llvm-cov and the pinned llvm-tools component. The gate measures the whole workspace except the CXX bridge, so running the hooks on Windows also measures the Win32 modules. Mutation testing requires cargo-mutants. On Windows, `windows\build.ps1 -SmokeTest` runs the interactive smoke test with the same checks and exit codes as the macOS one. The interactive `macos/build --smoke-test` command launches a separate test bundle and briefly takes keyboard focus. It advances the clock to exercise the same timer, menu actions and windows as the real app. A protected session can skip the visual checks (exit 77).
