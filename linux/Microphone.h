#pragma once

#include <memory>

namespace drink {
enum class Activity { Unavailable, Clear, Active };

// Drive PipeWire's pollable loop from the desktop's event loop. All operations
// including teardown run on the same thread. No audio streams are created.
class Microphone {
public:
    Microphone();
    ~Microphone();
    int fd() const;
    Activity dispatch();

private:
    struct Connection;
    std::unique_ptr<Connection> connection;
};
} // namespace drink
