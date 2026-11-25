#import "BreakWindows.h"

@interface BreakWindow ()
@property(nonatomic, copy) void (^dismiss)(void);
@property(nonatomic, copy) dispatch_block_t escapeHold;
@end

@implementation BreakWindow
- (instancetype)initForScreen:(NSScreen *)screen
                      dismiss:(void (^)(void))dismiss {
    self = [super initWithContentRect:screen.frame
                            styleMask:NSWindowStyleMaskBorderless
                              backing:NSBackingStoreBuffered
                                defer:NO];
    if (!self)
        return nil;
    self.dismiss = dismiss;
    [self setFrame:screen.frame display:NO];
    self.level = NSMainMenuWindowLevel + 1;
    self.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                              NSWindowCollectionBehaviorCanJoinAllApplications |
                              NSWindowCollectionBehaviorStationary |
                              NSWindowCollectionBehaviorIgnoresCycle;
    self.backgroundColor = [NSColor colorWithSRGBRed:0.055
                                               green:0.12
                                                blue:0.14
                                               alpha:1];
    self.opaque = YES;
    self.hidesOnDeactivate = YES;
    self.hasShadow = NO;
    self.animationBehavior = NSWindowAnimationBehaviorNone;
    NSTextField *title = [NSTextField labelWithString:@"Get some water."];
    title.font = [NSFont systemFontOfSize:44 weight:NSFontWeightSemibold];
    title.textColor = NSColor.whiteColor;
    NSTextField *detail = [NSTextField labelWithString:@"Look away and blink."];
    detail.font = [NSFont systemFontOfSize:24];
    detail.textColor = [NSColor colorWithWhite:0.84 alpha:1];
    NSStackView *stack = [NSStackView stackViewWithViews:@[ title, detail ]];
    stack.orientation = NSUserInterfaceLayoutOrientationVertical;
    stack.alignment = NSLayoutAttributeCenterX;
    stack.spacing = 12;
    stack.translatesAutoresizingMaskIntoConstraints = NO;
    [self.contentView addSubview:stack];
    [NSLayoutConstraint activateConstraints:@[
        [stack.centerXAnchor
            constraintEqualToAnchor:self.contentView.centerXAnchor],
        [stack.centerYAnchor
            constraintEqualToAnchor:self.contentView.centerYAnchor]
    ]];
    return self;
}
- (BOOL)canBecomeKeyWindow {
    return YES;
}
- (void)cancelHold {
    if (self.escapeHold)
        dispatch_block_cancel(self.escapeHold);
    self.escapeHold = nil;
}
- (void)keyDown:(NSEvent *)event {
    if (event.keyCode != 53 || event.isARepeat || self.escapeHold)
        return;
    self.escapeHold =
        dispatch_block_create(dispatch_block_flags_t{}, self.dismiss);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC),
                   dispatch_get_main_queue(), self.escapeHold);
}
- (void)keyUp:(NSEvent *)event {
    if (event.keyCode == 53)
        [self cancelHold];
}
- (void)resignKeyWindow {
    [self cancelHold];
    [super resignKeyWindow];
}
@end

@implementation BreakWindows {
    NSMutableArray<BreakWindow *> *_windows;
    NSRunningApplication *_previous;
    void (^_dismiss)(void);
}
- (instancetype)initWithDismiss:(void (^)(void))dismiss {
    if ((self = [super init])) {
        _windows = [NSMutableArray array];
        _dismiss = [dismiss copy];
    }
    return self;
}
- (NSUInteger)count {
    return _windows.count;
}
- (void)show {
    if (_windows.count)
        return;
    if (!NSApp.isActive) {
        _previous = NSWorkspace.sharedWorkspace.frontmostApplication;
        // A timed break has no click to hand off focus from the foreground app.
        [NSApp activateIgnoringOtherApps:YES];
        if (!NSApp.isActive)
            return; // The delegate retries when AppKit grants keyboard focus.
    }
    if (!_windows.count)
        [self rebuild];
}
- (void)rebuild {
    for (BreakWindow *window in _windows)
        [window orderOut:nil];
    [_windows removeAllObjects];
    for (NSScreen *screen in NSScreen.screens) {
        BreakWindow *window = [[BreakWindow alloc] initForScreen:screen
                                                         dismiss:_dismiss];
        [_windows addObject:window];
        [window orderFront:nil];
    }
    [_windows.firstObject makeKeyAndOrderFront:nil];
}
- (void)hide {
    BOOL restore = _windows.count > 0 && NSApp.isActive;
    for (BreakWindow *window in _windows) {
        [window cancelHold];
        [window orderOut:nil];
    }
    [_windows removeAllObjects];
    if (restore && _previous && !_previous.isTerminated && !_previous.active &&
        _previous.processIdentifier !=
            NSProcessInfo.processInfo.processIdentifier) {
        [NSApp yieldActivationToApplication:_previous];
        [_previous activateWithOptions:0];
    }
    _previous = nil;
}
@end
