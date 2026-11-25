#import "Microphone.h"
#import <CoreAudio/CoreAudio.h>
#include <set>
#include <vector>

static constexpr AudioObjectPropertyAddress Processes = {
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyElementMain};
static constexpr AudioObjectPropertyAddress Running = {
    kAudioProcessPropertyIsRunningInput, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyElementMain};

@implementation Microphone {
    AudioObjectPropertyListenerBlock _listener;
    std::set<AudioObjectID> _observed;
    BOOL _observingList;
    BOOL _needsRetry;
}

- (instancetype)initWithChange:(void (^)(void))changed {
    if ((self = [super init])) {
        _listener = ^(UInt32, const AudioObjectPropertyAddress *) {
          changed();
        };
    }
    return self;
}

- (BOOL)needsRetry {
    return _needsRetry;
}

- (InputActivity)sample {
    auto queue = dispatch_get_main_queue();
    if (!_observingList) {
        _observingList = AudioObjectAddPropertyListenerBlock(
                             kAudioObjectSystemObject, &Processes, queue,
                             _listener) == noErr;
    }
    _needsRetry = !_observingList;
    UInt32 size = 0;
    if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &Processes, 0,
                                       nullptr, &size) != noErr) {
        _needsRetry = YES;
        return InputActivity::Unavailable;
    }
    std::vector<AudioObjectID> objects(size / sizeof(AudioObjectID));
    if (size &&
        AudioObjectGetPropertyData(kAudioObjectSystemObject, &Processes, 0,
                                   nullptr, &size, objects.data()) != noErr) {
        _needsRetry = YES;
        return InputActivity::Unavailable;
    }
    objects.resize(size / sizeof(AudioObjectID));
    std::set<AudioObjectID> current(objects.begin(), objects.end());
    for (auto it = _observed.begin(); it != _observed.end();) {
        if (!current.count(*it)) {
            AudioObjectRemovePropertyListenerBlock(*it, &Running, queue,
                                                   _listener);
            it = _observed.erase(it);
        } else {
            ++it;
        }
    }
    bool active = false;
    bool unavailable = false;
    for (auto id : current) {
        if (!_observed.count(id)) {
            if (AudioObjectAddPropertyListenerBlock(id, &Running, queue,
                                                    _listener) == noErr) {
                _observed.insert(id);
            } else {
                _needsRetry = YES;
            }
        }
        UInt32 running = 0;
        UInt32 length = sizeof(running);
        if (AudioObjectGetPropertyData(id, &Running, 0, nullptr, &length,
                                       &running) != noErr) {
            unavailable = true;
            _needsRetry = YES;
        } else if (running) {
            active = true;
        }
    }
    if (active)
        return InputActivity::Active;
    return unavailable ? InputActivity::Unavailable : InputActivity::Clear;
}

- (void)dealloc {
    auto queue = dispatch_get_main_queue();
    if (_observingList)
        AudioObjectRemovePropertyListenerBlock(kAudioObjectSystemObject,
                                               &Processes, queue, _listener);
    for (auto id : _observed)
        AudioObjectRemovePropertyListenerBlock(id, &Running, queue, _listener);
}
@end
