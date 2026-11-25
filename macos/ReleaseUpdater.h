#import <AppKit/AppKit.h>

// Synchronous entry point for the background worker and command-line checks.
enum class UpdateResult { Current, Installed, Busy, Failed };
UpdateResult InstallLatestRelease(NSURL *bundle);

@interface ReleaseUpdater : NSObject
- (instancetype)initWithMenu:(NSMenu *)menu;
- (void)restartIfUpdated;
@end
