#import "DrinkApp.h"

int main() {
    @autoreleasepool {
        NSApplication *app = NSApplication.sharedApplication;
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        __attribute__((objc_precise_lifetime)) DrinkApp *delegate =
            [[DrinkApp alloc] init];
        app.delegate = delegate;
        [app run];
    }
    return 0;
}
