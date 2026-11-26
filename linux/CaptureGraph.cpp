#include "CaptureGraph.h"
#include <vector>

namespace drink {
void CaptureGraph::node(Id id, std::string_view mediaClass) {
    nodes[id] = mediaClass;
}

void CaptureGraph::port(Id id, bool monitor) {
    if (monitor) {
        monitorPorts.insert(id);
    } else {
        monitorPorts.erase(id);
    }
}

void CaptureGraph::link(Id id, Id source, Id outputPort, Id destination,
                        bool active) {
    if (active) {
        activeLinks[id] = {.source = source,
                           .outputPort = outputPort,
                           .destination = destination};
    } else {
        activeLinks.erase(id);
    }
}

void CaptureGraph::remove(Id id) {
    nodes.erase(id);
    monitorPorts.erase(id);
    activeLinks.erase(id);
    std::erase_if(activeLinks, [id](const auto &entry) {
        const auto &route = entry.second;
        return route.source == id || route.destination == id ||
               route.outputPort == id;
    });
}

static std::vector<CaptureGraph::Id>
recordingStreams(const std::map<CaptureGraph::Id, std::string> &nodes) {
    std::vector<CaptureGraph::Id> streams;
    for (const auto &[id, mediaClass] : nodes) {
        if (mediaClass == "Stream/Input/Audio") {
            streams.push_back(id);
        }
    }
    return streams;
}

bool CaptureGraph::capturing() const {
    std::multimap<Id, Id> upstream;
    for (const auto &[id, route] : activeLinks) {
        if (!monitorPorts.contains(route.outputPort)) {
            upstream.emplace(route.destination, route.source);
        }
    }
    auto work = recordingStreams(nodes);
    std::set<Id> seen;
    while (!work.empty()) {
        const auto id = work.back();
        work.pop_back();
        if (!seen.insert(id).second) {
            continue;
        }
        const auto source = nodes.find(id);
        if (source == nodes.end() || source->second.starts_with("Audio/Sink")) {
            continue;
        }
        if (source->second == "Audio/Source") {
            return true;
        }
        const auto [first, end] = upstream.equal_range(id);
        for (auto it = first; it != end; ++it) {
            work.push_back(it->second);
        }
    }
    return false;
}
} // namespace drink
