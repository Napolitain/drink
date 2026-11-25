#import "DrinkApp.h"
#import "ReleaseUpdater.h"
#ifdef DRINK_DEVELOPMENT_SESSION
#import "development/Session.h"
#endif

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc == 2 && strcmp(argv[1], "--check-for-updates") == 0) {
            if (![NSBundle.mainBundle.bundleIdentifier
                    isEqualToString:@"io.github.napolitain.drink"]) {
                fputs("Updates apply only to release builds.\n", stderr);
                return 1;
            }
            switch (InstallLatestRelease(NSBundle.mainBundle.bundleURL)) {
            case UpdateResult::Current:
                puts("Up to date.");
                return 0;
            case UpdateResult::Installed:
                puts("Installed new release.");
                return 0;
            case UpdateResult::Busy:
                fputs("Another update is in progress.\n", stderr);
                return 1;
            case UpdateResult::Failed:
                fputs("Update failed.\n", stderr);
                return 1;
            }
        }
#ifdef DRINK_DEVELOPMENT_SESSION
        if (!BeginDevelopmentSession())
            return 1;
#endif
        NSApplication *app = NSApplication.sharedApplication;
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        __attribute__((objc_precise_lifetime)) DrinkApp *delegate =
            [[DrinkApp alloc] init];
        app.delegate = delegate;
        [app run];
    }
    return 0;
}
