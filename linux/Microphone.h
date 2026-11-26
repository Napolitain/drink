#pragma once

#include <cstdint>
#include <memory>

namespace drink {
enum class Activity : std::uint8_t { Unavailable, Clear, Active };

// Drive PipeWire's pollable loop from the desktop's event loop. All operations
// including teardown run on the same thread. No audio streams are created.
class Microphone {
public:
    Microphone();
    ~Microphone();
    Microphone(const Microphone &) = delete;
    Microphone &operator=(const Microphone &) = delete;
    Microphone(Microphone &&) = delete;
    Microphone &operator=(Microphone &&) = delete;
    [[nodiscard]] int fd() const;
    Activity dispatch();

private:
    struct Connection;
    std::unique_ptr<Connection> connection;
};
} // namespace drink
