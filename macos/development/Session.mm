#import "Session.h"
#import <AppKit/AppKit.h>
#include <crt_externs.h>
#include <fcntl.h>
#include <spawn.h>
#include <unistd.h>

static NSString *const ReleaseID = @"io.github.napolitain.drink";

@interface ReleaseObserver : NSObject
@end
@implementation ReleaseObserver
- (void)observeValueForKeyPath:(NSString *)keyPath
                      ofObject:(id)object
                        change:(NSDictionary *)change
                       context:(void *)context {
    dispatch_async(dispatch_get_main_queue(), ^{
      if ([NSRunningApplication
              runningApplicationsWithBundleIdentifier:ReleaseID]
              .count)
          [NSApp terminate:nil];
    });
}
@end

// Keep the session lock through restoration. EOF on stdin also wakes the
// helper if the development process crashes or the terminal interrupts it.
static bool restoreOnExit(int lock, NSString *path) {
    int lifetime[2];
    if (pipe(lifetime) < 0)
        return false;
    fcntl(lifetime[0], F_SETFD, FD_CLOEXEC);
    fcntl(lifetime[1], F_SETFD, FD_CLOEXEC);
    posix_spawn_file_actions_t actions;
    posix_spawn_file_actions_init(&actions);
    posix_spawn_file_actions_adddup2(&actions, lifetime[0], STDIN_FILENO);
    posix_spawn_file_actions_adddup2(&actions, lock, 9);
    posix_spawnattr_t attributes;
    posix_spawnattr_init(&attributes);
    posix_spawnattr_setflags(&attributes, POSIX_SPAWN_SETPGROUP |
                                              POSIX_SPAWN_CLOEXEC_DEFAULT);
    const char *args[] = {"sh",
                          "-c",
                          "IFS= read -r _; /usr/bin/open -g \"$1\"",
                          "drink-dev",
                          path.fileSystemRepresentation,
                          nullptr};
    pid_t child;
    int error = posix_spawn(&child, "/bin/sh", &actions, &attributes,
                            const_cast<char **>(args), *_NSGetEnviron());
    posix_spawnattr_destroy(&attributes);
    posix_spawn_file_actions_destroy(&actions);
    close(lifetime[0]);
    if (error) {
        close(lifetime[1]);
        return false;
    }
    // The write end belongs to this process until exit. Children cannot inherit
    // it across exec.
    return true;
}

bool BeginDevelopmentSession() {
    NSURL *cache =
        [NSFileManager.defaultManager URLForDirectory:NSCachesDirectory
                                             inDomain:NSUserDomainMask
                                    appropriateForURL:nil
                                               create:YES
                                                error:nil];
    if (!cache)
        return false;
    NSURL *file = [cache
        URLByAppendingPathComponent:@"io.github.napolitain.drink.dev.lock"];
    int lock =
        open(file.fileSystemRepresentation,
             O_CREAT | O_RDWR | O_EXLOCK | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW,
             0600);
    if (lock < 0) {
        perror("Cannot start another Drink development session");
        return false;
    }

    NSArray<NSRunningApplication *> *running = [NSRunningApplication
        runningApplicationsWithBundleIdentifier:ReleaseID];
    if (running.count > 1) {
        fputs("More than one release is running. Quit the extra copy first.\n",
              stderr);
        return false;
    }
    NSRunningApplication *release = running.firstObject;
    if (release) {
        if (!release.bundleURL.isFileURL ||
            !restoreOnExit(lock, release.bundleURL.path))
            return false;
        if (!release.terminated && ![release terminate])
            return false;
        auto deadline = NSProcessInfo.processInfo.systemUptime + 8;
        while (!release.terminated &&
               NSProcessInfo.processInfo.systemUptime < deadline) {
            [NSRunLoop.currentRunLoop
                runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }
        if (!release.terminated) {
            fputs("Drink is still running. Development session cancelled.\n",
                  stderr);
            return false;
        }
    }

    // The regular launch notification excludes menu-bar apps.
    static ReleaseObserver *observer = [ReleaseObserver new];
    [NSWorkspace.sharedWorkspace addObserver:observer
                                  forKeyPath:@"runningApplications"
                                     options:NSKeyValueObservingOptionInitial
                                     context:nil];
    return true;
}
