#pragma once

#include <cstdint>
#include <map>
#include <set>
#include <string>
#include <string_view>

namespace drink {
// Only registry metadata is needed to identify a live microphone route.
class CaptureGraph {
public:
    using Id = std::uint32_t;
    void node(Id id, std::string_view mediaClass);
    void port(Id id, bool monitor);
    void link(Id id, Id source, Id outputPort, Id destination, bool active);
    void remove(Id id);
    [[nodiscard]] bool capturing() const;

private:
    struct Link {
        Id source, outputPort, destination;
    };
    std::map<Id, std::string> nodes;
    std::set<Id> monitorPorts;
    std::map<Id, Link> activeLinks;
};
} // namespace drink
