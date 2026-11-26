#include "Microphone.h"
#include "CaptureGraph.h"
#include <algorithm>
#include <cstring>
#include <map>
#include <pipewire/pipewire.h>
#include <spa/utils/string.h>

namespace drink {
struct Microphone::Connection {
    struct DestroyProxy {
        void operator()(pw_proxy *proxy) const { pw_proxy_destroy(proxy); }
    };

    struct Subscription {
        // Destroy the proxy while its registered listener is still alive.
        spa_hook listener{};
        std::unique_ptr<pw_proxy, DestroyProxy> proxy;
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

    static void global(void *data, uint32_t id,
                       [[maybe_unused]] uint32_t permissions, const char *type,
                       uint32_t version, const spa_dict *props) {
        auto &self = *static_cast<Connection *>(data);
        if (std::strcmp(type, PW_TYPE_INTERFACE_Link) == 0) {
            auto &subscription = self.links[id];
            subscription.proxy.reset(static_cast<pw_proxy *>(pw_registry_bind(
                self.registry, id, type,
                std::min(version, uint32_t{PW_VERSION_LINK}), 0)));
            if (!subscription.proxy) {
                self.failed = true;
                return;
            }
            static const pw_link_events events{
                .version = PW_VERSION_LINK_EVENTS, .info = linkInfo};
            pw_proxy_add_object_listener(subscription.proxy.get(),
                                         &subscription.listener, &events, data);
        } else if (props != nullptr &&
                   std::strcmp(type, PW_TYPE_INTERFACE_Node) == 0) {
            const char *name = spa_dict_lookup(props, PW_KEY_MEDIA_CLASS);
            self.graph.node(id, name != nullptr ? name : "");
        } else if (props != nullptr &&
                   std::strcmp(type, PW_TYPE_INTERFACE_Port) == 0) {
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
        self.links.erase(id);
        self.dirty = true;
    }

    static void done(void *data, uint32_t id, int sequence) {
        auto &self = *static_cast<Connection *>(data);
        if (id != PW_ID_CORE || self.sync != sequence) {
            return;
        }
        if (self.enumerated) {
            self.ready = true;
        } else {
            // Bind requests made during enumeration need their own round trip.
            self.enumerated = true;
            self.sync = pw_core_sync(self.core, PW_ID_CORE, self.sync);
        }
    }

    static void error(void *data, uint32_t id, [[maybe_unused]] int sequence,
                      int result, [[maybe_unused]] const char *message) {
        if (id == PW_ID_CORE || result == -EPIPE) {
            static_cast<Connection *>(data)->failed = true;
        }
    }

    static pw_loop *initializeLoop() {
        pw_init(nullptr, nullptr);
        return pw_loop_new(nullptr);
    }

    Connection() : loop(initializeLoop()) {
        if (loop == nullptr) {
            return;
        }
        pw_loop_enter(loop);
        context = pw_context_new(loop, nullptr, 0);
        if (context == nullptr) {
            return;
        }
        core = pw_context_connect(context, nullptr, 0);
        if (core == nullptr) {
            return;
        }
        registry = pw_core_get_registry(core, PW_VERSION_REGISTRY, 0);
        if (registry == nullptr) {
            return;
        }
        static const auto coreEvents = [] {
            pw_core_events events{};
            events.version = PW_VERSION_CORE_EVENTS;
            events.done = done;
            events.error = error;
            return events;
        }();
        static const pw_registry_events events{.version =
                                                   PW_VERSION_REGISTRY_EVENTS,
                                               .global = global,
                                               .global_remove = removed};
        pw_core_add_listener(core, &coreListener, &coreEvents, this);
        pw_registry_add_listener(registry, &registryListener, &events, this);
        sync = pw_core_sync(core, PW_ID_CORE, 0);
    }

    Connection(const Connection &) = delete;
    Connection &operator=(const Connection &) = delete;
    Connection(Connection &&) = delete;
    Connection &operator=(Connection &&) = delete;

    ~Connection() {
        links.clear();
        if (registry != nullptr) {
            // pw_core_get_registry returns a typed client proxy. PipeWire's
            // local destructor accepts only pw_proxy*; pw_registry_destroy
            // instead destroys a server global.
            // NOLINTNEXTLINE(cppcoreguidelines-pro-type-reinterpret-cast)
            pw_proxy_destroy(reinterpret_cast<pw_proxy *>(registry));
        }
        if (core != nullptr) {
            pw_core_disconnect(core);
        }
        if (context != nullptr) {
            pw_context_destroy(context);
        }
        if (loop != nullptr) {
            pw_loop_leave(loop);
            pw_loop_destroy(loop);
        }
        pw_deinit();
    }
};

Microphone::Microphone() : connection(std::make_unique<Connection>()) {}
Microphone::~Microphone() = default;

int Microphone::fd() const {
    return connection->registry != nullptr && !connection->failed
               ? pw_loop_get_fd(connection->loop)
               : -1;
}

Activity Microphone::dispatch() {
    auto &c = *connection;
    if (fd() < 0) {
        return Activity::Unavailable;
    }
    if (pw_loop_iterate(c.loop, 0) < 0) {
        c.failed = true;
    }
    if (!c.ready || c.failed) {
        return Activity::Unavailable;
    }
    if (c.dirty) {
        c.activity = c.graph.capturing() ? Activity::Active : Activity::Clear;
        c.dirty = false;
    }
    return c.activity;
}
} // namespace drink
