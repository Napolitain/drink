// Interactive macOS integration test. Only supplies a clock offset; the app
// samples real activity, schedules real timers, and creates real AppKit
// windows.
#import "BreakWindows.h"
#import "DrinkApp.h"
#import "Microphone.h"

@interface DrinkApp (TestClock)
- (uint64_t)clockMilliseconds;
- (void)update;
- (BOOL)blocked;
@end

@interface SmokeApp : DrinkApp
@property(nonatomic) uint64_t clockOffset;
@property(nonatomic) uint64_t started;
@property(nonatomic, strong) NSRunningApplication *previous;
@end

static void require(BOOL condition, NSString *message) {
    if (!condition) {
        fprintf(stderr, "FAIL: %s\n", message.UTF8String);
        exit(1);
    }
}
static NSArray<NSWindow *> *visibleWindows() {
    NSMutableArray *windows = [NSMutableArray array];
    for (NSWindow *window in NSApp.windows) {
        if ([window isKindOfClass:BreakWindow.class] && window.isVisible) {
            [windows addObject:window];
        }
    }
    return windows;
}
static void after(NSTimeInterval seconds, dispatch_block_t block) {
    dispatch_after(
        dispatch_time(DISPATCH_TIME_NOW, (int64_t)(seconds * NSEC_PER_SEC)),
        dispatch_get_main_queue(), block);
}
@implementation SmokeApp
- (uint64_t)clockMilliseconds {
    return [super clockMilliseconds] + self.clockOffset;
}
- (void)applicationDidFinishLaunching:(NSNotification *)note {
    _started = self.clockMilliseconds;
    [super applicationDidFinishLaunching:note];
    NSScreen *screen = NSScreen.screens.firstObject;
    require(screen != nil, @"smoke test needs a connected display");
    __block int dismissed = 0;
    BreakWindow *detached = [[BreakWindow alloc] initForScreen:screen
                                                       dismiss:^{
                                                         dismissed++;
                                                       }];
    require(!detached.visible && NSEqualRects(screen.frame, detached.frame) &&
                detached.opaque,
            @"offscreen break window matches the screen bounds");
    NSEvent * (^escape)(NSEventType) = ^NSEvent *(NSEventType type) {
      return [NSEvent keyEventWithType:type
                              location:NSZeroPoint
                         modifierFlags:0
                             timestamp:0
                          windowNumber:0
                               context:nil
                            characters:@"\x1b"
           charactersIgnoringModifiers:@"\x1b"
                             isARepeat:NO
                               keyCode:53];
    };
    [detached keyDown:escape(NSEventTypeKeyDown)];
    [detached keyUp:escape(NSEventTypeKeyUp)];
    after(2.1, ^{
      require(dismissed == 0, @"tapping Escape should keep the break open");
      [detached keyDown:escape(NSEventTypeKeyDown)];
      after(2.1, ^{
        require(dismissed == 1, @"holding Escape should dismiss once");
        [detached keyUp:escape(NSEventTypeKeyUp)];
        puts("PASS: offscreen frame and opacity, Escape tap and two-second "
             "hold");
        [self checkSession];
      });
    });
}
- (void)checkSession {
    after(0.3, ^{
      Microphone *mic = [[Microphone alloc] initWithChange:^{
      }];
      auto status = [mic sample];
      if (status == InputActivity::Active) {
          fprintf(stderr,
                  "Microphone in use; skipping interactive overlay test.\n");
          exit(77);
      }
      require(status == InputActivity::Clear,
              @"Core Audio should report an available idle input signal");
      require(!visibleWindows().count, @"launch should be quiet");
      NSDictionary *session =
          CFBridgingRelease(CGSessionCopyCurrentDictionary());
      if ([session[@"CGSSessionScreenIsLocked"] boolValue] ||
          ![session[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue]) {
          self.clockOffset = 1'800'001;
          [self update];
          [self takeBreak:nil];
          require(!visibleWindows().count,
                  @"an absent session suppresses even a manual break");
          puts("PASS: Core Audio idle, launch quiet, locked or inactive "
               "session suppressed a due and manual break.");
          puts("SKIP: visible overlay checks require an unlocked desktop.");
          exit(77);
      }
      if (self.blocked) {
          puts("SKIP: a protected foreground app is active.");
          exit(77);
      }
      self.previous = NSWorkspace.sharedWorkspace.frontmostApplication;
      self.clockOffset = 1'799'000 - (self.clockMilliseconds - _started);
      [self update];
      after(1.1, ^{
        [self verifyBreak];
      });
    });
}
- (void)verifyBreak {
    NSArray *windows = visibleWindows();
    require(windows.count > 0 && windows.count == NSScreen.screens.count,
            @"one visible window per display");
    for (NSScreen *screen in NSScreen.screens) {
        BOOL covered = NO;
        for (NSWindow *window in windows) {
            covered |=
                NSEqualRects(window.frame, screen.frame) && window.isOpaque;
        }
        require(covered, @"window covers the full screen frame");
    }
    require([windows containsObject:NSApp.keyWindow] && NSApp.isActive,
            @"overlay owns keyboard focus");
    printf("Core Audio idle, covered %lu display(s), keyboard focus acquired\n",
           (unsigned long)windows.count);
    self.clockOffset += 29'000;
    [self update];
    after(1.1, ^{
      require(!visibleWindows().count,
              @"timer should automatically close all windows");
      if (self.previous && !self.previous.terminated) {
          require(self.previous.active,
                  @"automatic dismissal returns focus to the prior app");
          puts("Prior foreground app regained focus");
      }
      [self takeBreak:nil];
      after(0.3, ^{
        require(visibleWindows().count > 0,
                @"Take break now opens the overlay");
        [self togglePause:nil];
        require(!visibleWindows().count, @"Pause closes the overlay");
        [self togglePause:nil];
        require(!visibleWindows().count, @"Resume starts a fresh interval");
        printf(
            "Automatic dismissal, Take break now, Pause and Resume passed\n");
        [NSApp terminate:nil];
      });
    });
}
@end

int main() {
    @autoreleasepool {
        NSApplication *app = NSApplication.sharedApplication;
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        __attribute__((objc_precise_lifetime)) SmokeApp *delegate =
            [[SmokeApp alloc] init];
        app.delegate = delegate;
        [app run];
    }
    return 0;
}
