#include "CaptureGraph.h"
#include <vector>

namespace drink {
void CaptureGraph::node(Id id, std::string_view mediaClass) {
    nodes[id] = mediaClass;
}

void CaptureGraph::port(Id id, bool monitor) {
    if (monitor)
        monitorPorts.insert(id);
    else
        monitorPorts.erase(id);
}

void CaptureGraph::link(Id id, Id source, Id outputPort, Id destination,
                        bool active) {
    if (active)
        activeLinks[id] = {source, outputPort, destination};
    else
        activeLinks.erase(id);
}

void CaptureGraph::remove(Id id) {
    nodes.erase(id);
    monitorPorts.erase(id);
    activeLinks.erase(id);
    std::erase_if(activeLinks, [id](const auto &entry) {
        const auto &link = entry.second;
        return link.source == id || link.destination == id ||
               link.outputPort == id;
    });
}

bool CaptureGraph::capturing() const {
    std::multimap<Id, Id> upstream;
    for (const auto &[id, link] : activeLinks) {
        if (!monitorPorts.contains(link.outputPort))
            upstream.emplace(link.destination, link.source);
    }
    std::vector<Id> work;
    for (const auto &[id, mediaClass] : nodes) {
        if (mediaClass == "Stream/Input/Audio")
            work.push_back(id);
    }
    std::set<Id> seen;
    while (!work.empty()) {
        const auto id = work.back();
        work.pop_back();
        if (!seen.insert(id).second)
            continue;
        const auto node = nodes.find(id);
        if (node == nodes.end() || node->second.starts_with("Audio/Sink"))
            continue;
        if (node->second == "Audio/Source")
            return true;
        const auto [first, end] = upstream.equal_range(id);
        for (auto it = first; it != end; ++it)
            work.push_back(it->second);
    }
    return false;
}
} // namespace drink
