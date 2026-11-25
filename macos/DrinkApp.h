#import <AppKit/AppKit.h>

@interface DrinkApp : NSObject <NSApplicationDelegate, NSMenuDelegate>
- (void)takeBreak:(id)sender;
- (void)togglePause:(id)sender;
@end
