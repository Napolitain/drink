#import "ReleaseUpdater.h"
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

static UpdateResult stageAndExchange(NSURL *bundle) {
    NSFileManager *files = NSFileManager.defaultManager;
    NSError *error = nil;
    // macOS allocates a private temporary directory on the installation volume.
    NSURL *stage = [files URLForDirectory:NSItemReplacementDirectory
                                 inDomain:NSUserDomainMask
                        appropriateForURL:bundle
                                   create:YES
                                    error:&error];
    if (!stage) {
        NSLog(@"Drink update: %@", error.localizedDescription);
        return UpdateResult::Failed;
    }
    NSTask *fetch = [NSTask new];
    fetch.executableURL =
        [bundle URLByAppendingPathComponent:@"Contents/Resources/fetch-update"];
    fetch.arguments = @[ bundle.path, stage.path ];
    UpdateResult result = UpdateResult::Failed;
    if ([fetch launchAndReturnError:&error]) {
        [fetch waitUntilExit];
        if (fetch.terminationReason == NSTaskTerminationReasonExit &&
            fetch.terminationStatus == 0) {
            NSURL *candidate = [stage URLByAppendingPathComponent:@"Drink.app"];
            if (![files fileExistsAtPath:candidate.path]) {
                result = UpdateResult::Current;
            } else if (renamex_np(candidate.fileSystemRepresentation,
                                  bundle.fileSystemRepresentation,
                                  RENAME_SWAP) == 0) {
                result = UpdateResult::Installed;
            } else {
                perror("Drink update exchange");
            }
        }
    } else {
        NSLog(@"Drink update: %@", error.localizedDescription);
    }
    [files removeItemAtURL:stage error:nil];
    return result;
}

UpdateResult InstallLatestRelease(NSURL *bundle) {
    int lock = open(bundle.fileSystemRepresentation,
                    O_RDONLY | O_DIRECTORY | O_EXLOCK | O_NONBLOCK | O_CLOEXEC);
    if (lock < 0)
        return errno == EWOULDBLOCK ? UpdateResult::Busy : UpdateResult::Failed;
    UpdateResult result = stageAndExchange(bundle);
    close(lock);
    return result;
}

@implementation ReleaseUpdater {
    NSMenuItem *_item;
    NSTimer *_daily;
    BOOL _checking, _installed;
}

- (instancetype)initWithMenu:(NSMenu *)menu {
    if ((self = [super init])) {
        _item = [menu addItemWithTitle:@"Check for Updates…"
                                action:@selector(check:)
                         keyEquivalent:@""];
        _item.target = self;
        // Source builds can opt into the release channel using the menu.
        if ([NSBundle.mainBundle.infoDictionary[@"DrinkReleaseBuild"]
                boolValue]) {
            __weak ReleaseUpdater *weakSelf = self;
            _daily = [NSTimer scheduledTimerWithTimeInterval:24 * 60 * 60
                                                     repeats:YES
                                                       block:^(NSTimer *) {
                                                         [weakSelf check:nil];
                                                       }];
            _daily.tolerance = 300;
            [self check:nil];
        }
    }
    return self;
}

- (void)check:(id)sender {
    if (_installed) {
        if (sender)
            [self restartIfUpdated];
        return;
    }
    if (_checking)
        return;
    _checking = YES;
    _item.title = @"Checking for Updates…";
    _item.enabled = NO;
    NSURL *bundle = NSBundle.mainBundle.bundleURL;
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^{
      UpdateResult result = InstallLatestRelease(bundle);
      dispatch_async(dispatch_get_main_queue(), ^{
        self->_checking = NO;
        self->_item.enabled = YES;
        self->_installed = result == UpdateResult::Installed;
        switch (result) {
        case UpdateResult::Installed:
            self->_item.title = @"Restart to Finish Update";
            break;
        case UpdateResult::Current:
            self->_item.title = @"Up to Date · Check Again";
            break;
        case UpdateResult::Busy:
            self->_item.title = @"Update in Progress · Check Again";
            break;
        case UpdateResult::Failed:
            self->_item.title = @"Update Unavailable · Retry";
            break;
        }
        self->_item.toolTip = result == UpdateResult::Failed
                                  ? @"Check your connection to GitHub and "
                                    @"write access to the app’s folder."
                                  : nil;
      });
    });
}

- (void)restartIfUpdated {
    if (!_installed || _checking)
        return;
    _checking = YES;
    NSWorkspaceOpenConfiguration *config =
        NSWorkspaceOpenConfiguration.configuration;
    config.createsNewApplicationInstance = YES;
    config.activates = NO;
    [NSWorkspace.sharedWorkspace
        openApplicationAtURL:NSBundle.mainBundle.bundleURL
               configuration:config
           completionHandler:^(NSRunningApplication *app, NSError *error) {
             dispatch_async(dispatch_get_main_queue(), ^{
               self->_checking = NO;
               if (app && !error)
                   [NSApp terminate:nil];
               else
                   NSLog(@"Drink restart: %@", error.localizedDescription);
             });
           }];
}

- (void)dealloc {
    [_daily invalidate];
}
@end
