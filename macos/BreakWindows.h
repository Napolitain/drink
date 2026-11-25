#import <AppKit/AppKit.h>

@interface BreakWindow : NSWindow
- (instancetype)initForScreen:(NSScreen *)screen
                      dismiss:(void (^)(void))dismiss;
- (void)cancelHold;
@end

@interface BreakWindows : NSObject
- (instancetype)initWithDismiss:(void (^)(void))dismiss;
- (void)show;
- (void)hide;
- (void)rebuild;
@property(nonatomic, readonly) NSUInteger count;
@end
