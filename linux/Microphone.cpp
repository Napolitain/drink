#include "Microphone.h"
#include "CaptureGraph.h"
#include <algorithm>
#include <cstring>
#include <map>
#include <pipewire/pipewire.h>
#include <spa/utils/string.h>

namespace drink {
struct Microphone::Connection {
    struct Subscription {
        pw_link *proxy = nullptr;
        spa_hook listener{};
    };

    pw_loop *loop = nullptr;
    pw_context *context = nullptr;
    pw_core *core = nullptr;
    pw_registry *registry = nullptr;
    spa_hook coreListener{}, registryListener{};
    std::map<uint32_t, Subscription> links;
    CaptureGraph graph;
    int sync = 0;
    bool enumerated = false, ready = false, failed = false, dirty = true;
    Activity activity = Activity::Unavailable;

    static void global(void *data, uint32_t id, uint32_t, const char *type,
                       uint32_t version, const spa_dict *props) {
        auto &self = *static_cast<Connection *>(data);
        if (std::strcmp(type, PW_TYPE_INTERFACE_Link) == 0) {
            auto *proxy = static_cast<pw_link *>(pw_registry_bind(
                self.registry, id, type,
                std::min(version, uint32_t(PW_VERSION_LINK)), 0));
            if (!proxy) {
                self.failed = true;
                return;
            }
            auto &link = self.links[id];
            link.proxy = proxy;
            static const pw_link_events events{PW_VERSION_LINK_EVENTS,
                                               linkInfo};
            pw_link_add_listener(proxy, &link.listener, &events, data);
        } else if (props && std::strcmp(type, PW_TYPE_INTERFACE_Node) == 0) {
            const char *name = spa_dict_lookup(props, PW_KEY_MEDIA_CLASS);
            self.graph.node(id, name ? name : "");
        } else if (props && std::strcmp(type, PW_TYPE_INTERFACE_Port) == 0) {
            self.graph.port(
                id, spa_atob(spa_dict_lookup(props, PW_KEY_PORT_MONITOR)));
        } else {
            return;
        }
        self.dirty = true;
    }

    static void linkInfo(void *data, const pw_link_info *info) {
        auto &self = *static_cast<Connection *>(data);
        self.graph.link(info->id, info->output_node_id, info->output_port_id,
                        info->input_node_id,
                        info->state == PW_LINK_STATE_ACTIVE);
        self.dirty = true;
    }

    static void removed(void *data, uint32_t id) {
        auto &self = *static_cast<Connection *>(data);
        self.graph.remove(id);
        auto it = self.links.find(id);
        if (it != self.links.end()) {
            pw_proxy_destroy(reinterpret_cast<pw_proxy *>(it->second.proxy));
            self.links.erase(it);
        }
        self.dirty = true;
    }

    static void done(void *data, uint32_t id, int sequence) {
        auto &self = *static_cast<Connection *>(data);
        if (id != PW_ID_CORE || self.sync != sequence)
            return;
        if (self.enumerated)
            self.ready = true;
        else {
            // Bind requests made during enumeration need their own round trip.
            self.enumerated = true;
            self.sync = pw_core_sync(self.core, PW_ID_CORE, self.sync);
        }
    }

    static void error(void *data, uint32_t id, int, int result, const char *) {
        if (id == PW_ID_CORE || result == -EPIPE)
            static_cast<Connection *>(data)->failed = true;
    }

    Connection() {
        pw_init(nullptr, nullptr);
        loop = pw_loop_new(nullptr);
        if (!loop)
            return;
        pw_loop_enter(loop);
        context = pw_context_new(loop, nullptr, 0);
        if (!context || !(core = pw_context_connect(context, nullptr, 0)))
            return;
        registry = pw_core_get_registry(core, PW_VERSION_REGISTRY, 0);
        if (!registry)
            return;
        static const auto coreEvents = [] {
            pw_core_events events{};
            events.version = PW_VERSION_CORE_EVENTS;
            events.done = done;
            events.error = error;
            return events;
        }();
        static const pw_registry_events events{PW_VERSION_REGISTRY_EVENTS,
                                               global, removed};
        pw_core_add_listener(core, &coreListener, &coreEvents, this);
        pw_registry_add_listener(registry, &registryListener, &events, this);
        sync = pw_core_sync(core, PW_ID_CORE, 0);
    }

    ~Connection() {
        for (auto &[id, sub] : links)
            pw_proxy_destroy(reinterpret_cast<pw_proxy *>(sub.proxy));
        if (registry)
            pw_proxy_destroy(reinterpret_cast<pw_proxy *>(registry));
        if (core)
            pw_core_disconnect(core);
        if (context)
            pw_context_destroy(context);
        if (loop) {
            pw_loop_leave(loop);
            pw_loop_destroy(loop);
        }
        pw_deinit();
    }
};

Microphone::Microphone() : connection(std::make_unique<Connection>()) {}
Microphone::~Microphone() = default;

int Microphone::fd() const {
    return connection->registry && !connection->failed
               ? pw_loop_get_fd(connection->loop)
               : -1;
}

Activity Microphone::dispatch() {
    auto &c = *connection;
    if (fd() < 0)
        return Activity::Unavailable;
    if (pw_loop_iterate(c.loop, 0) < 0)
        c.failed = true;
    if (!c.ready || c.failed)
        return Activity::Unavailable;
    if (c.dirty) {
        c.activity = c.graph.capturing() ? Activity::Active : Activity::Clear;
        c.dirty = false;
    }
    return c.activity;
}
} // namespace drink
