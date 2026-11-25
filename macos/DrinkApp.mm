#import "DrinkApp.h"
#import "BreakWindows.h"
#import "Microphone.h"
#include "drink-macos-bridge/src/lib.rs.h"
#include <optional>

static BOOL isGame(NSRunningApplication *app) {
    if (!app.bundleIdentifier)
        return NO;
    NSArray *configured = [NSUserDefaults.standardUserDefaults
        stringArrayForKey:@"ProtectedGameBundleIDs"];
    if ([configured containsObject:app.bundleIdentifier])
        return YES;
    if (!app.bundleURL)
        return NO;
    NSString *category = [[NSBundle bundleWithURL:app.bundleURL]
        objectForInfoDictionaryKey:@"LSApplicationCategoryType"];
    return [category isEqualToString:@"public.app-category.games"] ||
           [category hasSuffix:@"-games"];
}

@implementation DrinkApp {
    std::optional<rust::Box<drink::Scheduler>> _scheduler;
    NSStatusItem *_item;
    NSMenuItem *_status;
    NSMenuItem *_pause;
    NSMenuItem *_micStatus;
    NSTimer *_timer;
    Microphone *_mic;
    BreakWindows *_overlay;
    InputActivity _input;
    BOOL _game, _sleeping, _screenOff, _locked, _sessionAway;
}
- (uint64_t)clockMilliseconds {
    return (uint64_t)(NSProcessInfo.processInfo.systemUptime * 1000);
}

- (void)applicationDidFinishLaunching:(NSNotification *)note {
    _scheduler.emplace(drink::new_scheduler(self.clockMilliseconds));
    __weak DrinkApp *weakSelf = self;
    _mic = [[Microphone alloc] initWithChange:^{
      [weakSelf update];
    }];
    _overlay = [[BreakWindows alloc] initWithDismiss:^{
      [weakSelf restart];
    }];

    _item = [NSStatusBar.systemStatusBar
        statusItemWithLength:NSSquareStatusItemLength];
    _item.button.image = [NSImage imageWithSystemSymbolName:@"drop"
                                   accessibilityDescription:@"Drink"];
    _item.button.toolTip = @"Drink";
    NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Drink"];
    menu.delegate = self;
    menu.autoenablesItems = NO;
    _status = [menu addItemWithTitle:@"" action:nil keyEquivalent:@""];
    _status.enabled = NO;
    _micStatus = [menu addItemWithTitle:@"Microphone status unavailable"
                                 action:nil
                          keyEquivalent:@""];
    _micStatus.enabled = NO;
    [menu addItem:NSMenuItem.separatorItem];
    [menu addItemWithTitle:@"Take break now"
                    action:@selector(takeBreak:)
             keyEquivalent:@""]
        .target = self;
    _pause = [menu addItemWithTitle:@"Pause for 1 hour"
                             action:@selector(togglePause:)
                      keyEquivalent:@""];
    _pause.target = self;
    [menu addItem:NSMenuItem.separatorItem];
    [menu addItemWithTitle:@"Quit Drink"
                    action:@selector(terminate:)
             keyEquivalent:@"q"]
        .target = NSApp;
    _item.menu = menu;

    auto workspace = NSWorkspace.sharedWorkspace.notificationCenter;
    for (NSString *name in @[
             NSWorkspaceDidActivateApplicationNotification,
             NSWorkspaceWillSleepNotification, NSWorkspaceDidWakeNotification,
             NSWorkspaceSessionDidResignActiveNotification,
             NSWorkspaceSessionDidBecomeActiveNotification,
             NSWorkspaceScreensDidSleepNotification,
             NSWorkspaceScreensDidWakeNotification
         ]) {
        [workspace addObserver:self
                      selector:@selector(workspaceChanged:)
                          name:name
                        object:nil];
    }
    for (NSString *name in
         @[ @"com.apple.screenIsLocked", @"com.apple.screenIsUnlocked" ]) {
        [NSDistributedNotificationCenter.defaultCenter
                   addObserver:self
                      selector:@selector(lockChanged:)
                          name:name
                        object:nil
            suspensionBehavior:
                NSNotificationSuspensionBehaviorDeliverImmediately];
    }
    [NSNotificationCenter.defaultCenter
        addObserver:self
           selector:@selector(displaysChanged:)
               name:NSApplicationDidChangeScreenParametersNotification
             object:nil];
    NSDictionary *session = CFBridgingRelease(CGSessionCopyCurrentDictionary());
    _locked = [session[@"CGSSessionScreenIsLocked"] boolValue];
    _sessionAway =
        ![session[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue];
    _game = isGame(NSWorkspace.sharedWorkspace.frontmostApplication);
    [self update];
}

- (void)applicationDidBecomeActive:(NSNotification *)note {
    if (_scheduler && (*_scheduler)->visible() && !_overlay.count)
        [self update];
}
- (void)workspaceChanged:(NSNotification *)note {
    NSString *name = note.name;
    if ([name isEqual:NSWorkspaceDidActivateApplicationNotification]) {
        NSRunningApplication *front = note.userInfo[NSWorkspaceApplicationKey];
        if (front.processIdentifier !=
            NSProcessInfo.processInfo.processIdentifier) {
            _game = isGame(front);
            // Switching to another app must never leave opaque windows above
            // its keyboard focus.
            if (_overlay.count && !_game && !self.blocked) {
                [self restart];
                return;
            }
        }
    } else if ([name isEqual:NSWorkspaceWillSleepNotification]) {
        _sleeping = YES;
    } else if ([name isEqual:NSWorkspaceDidWakeNotification]) {
        _sleeping = NO;
    } else if ([name isEqual:NSWorkspaceScreensDidSleepNotification]) {
        _screenOff = YES;
    } else if ([name isEqual:NSWorkspaceScreensDidWakeNotification]) {
        _screenOff = NO;
    } else {
        _sessionAway =
            [name isEqual:NSWorkspaceSessionDidResignActiveNotification];
    }
    [self update];
}
- (void)lockChanged:(NSNotification *)note {
    _locked = [note.name isEqual:@"com.apple.screenIsLocked"];
    [self update];
}
- (void)displaysChanged:(NSNotification *)note {
    [self update];
    if (_overlay.count)
        [_overlay rebuild];
}
- (BOOL)present {
    return !_sleeping && !_screenOff && !_locked && !_sessionAway &&
           NSScreen.screens.count > 0;
}
- (BOOL)blocked {
    return _input == InputActivity::Active || _game;
}

- (void)update {
    _input = [_mic sample];
    auto &s = **_scheduler;
    uint64_t now = self.clockMilliseconds;
    s.update(now, self.present, self.blocked);
    if (s.visible())
        [_overlay show];
    else
        [_overlay hide];

    [_timer invalidate];
    _timer = nil;
    uint64_t deadline = s.deadline();
    if (_mic.needsRetry && self.present) {
        uint64_t retry = now + (s.pending() || s.visible() ? 1000 : 60000);
        if (!deadline || retry < deadline)
            deadline = retry;
    }
    if (deadline) {
        // Use signed floating point subtraction so late or rounded deadlines
        // fire promptly.
        NSTimeInterval delay = MAX(
            0.001, ((double)deadline - (double)self.clockMilliseconds) / 1000);
        __weak DrinkApp *weakSelf = self;
        _timer = [NSTimer timerWithTimeInterval:delay
                                        repeats:NO
                                          block:^(NSTimer *) {
                                            [weakSelf update];
                                          }];
        [NSRunLoop.mainRunLoop addTimer:_timer forMode:NSRunLoopCommonModes];
    }
    [self refreshStatus];
}
- (void)refreshStatus {
    auto &s = **_scheduler;
    long minutes = (long)ceil(
        MAX(0., (double)s.deadline() - (double)self.clockMilliseconds) / 60000);
    NSString *text;
    if (s.paused())
        text = [NSString stringWithFormat:@"Paused · %ld min left", minutes];
    else if (!self.present)
        text = @"Waiting for your return";
    else if (s.visible())
        text = @"Water break · hold Esc to end";
    else if (s.pending()) {
        if (_input == InputActivity::Active)
            text = @"Break due · microphone in use";
        else if (_game)
            text = @"Break due · game in foreground";
        else
            text = @"Break due · waiting for ten quiet seconds";
    } else
        text = [NSString stringWithFormat:@"Next break in %ld min", minutes];
    _status.title = text;
    _micStatus.hidden = _input != InputActivity::Unavailable;
    _pause.title = s.paused() ? @"Resume" : @"Pause for 1 hour";
}
- (void)menuWillOpen:(NSMenu *)menu {
    [self refreshStatus];
}
- (void)restart {
    (*_scheduler)->restart(self.clockMilliseconds);
    [self update];
}
- (void)takeBreak:(id)sender {
    _input = [_mic sample];
    (*_scheduler)
        ->request_break(self.clockMilliseconds, self.present, self.blocked);
    [self update];
}
- (void)togglePause:(id)sender {
    if ((*_scheduler)->paused())
        [self restart];
    else {
        (*_scheduler)->pause(self.clockMilliseconds);
        [self update];
    }
}
- (void)applicationWillTerminate:(NSNotification *)note {
    [_timer invalidate];
    if (_overlay.count)
        [_overlay hide];
    [NSDistributedNotificationCenter.defaultCenter removeObserver:self];
    [NSWorkspace.sharedWorkspace.notificationCenter removeObserver:self];
    [NSNotificationCenter.defaultCenter removeObserver:self];
}
@end
