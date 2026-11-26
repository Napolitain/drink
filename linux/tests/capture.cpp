#include "CaptureGraph.h"
#include <cstdlib>
#include <iostream>

static void check(bool condition, const char *message) {
    if (!condition) {
        std::cerr << message << '\n';
        std::exit(1);
    }
}

int main() {
    drink::CaptureGraph g;
    g.node(1, "Audio/Source");
    g.node(2, "Audio/Sink");
    g.node(3, "Stream/Input/Audio");
    g.node(4, "Audio/Filter");
    g.node(5, "Audio/Source"); // A second, non-default headset.
    g.link(10, 2, 20, 3, true);
    check(!g.capturing(), "speaker capture counted");
    g.link(11, 5, 21, 3, false);
    check(!g.capturing(), "paused capture counted");
    g.link(11, 5, 21, 3, true);
    check(g.capturing(), "second microphone missed");
    g.remove(11);
    check(!g.capturing(), "removed link still active");
    g.link(12, 1, 22, 4, true);
    g.link(13, 4, 23, 4, true);
    check(!g.capturing(), "filter without capture client counted");
    g.link(14, 4, 24, 3, true);
    check(g.capturing(), "filtered microphone missed");
    g.port(22, true);
    check(!g.capturing(), "monitor port or feedback cycle counted");
    g.port(22, false);
    g.link(12, 1, 22, 4, false);
    check(!g.capturing(), "inactive upstream link counted");
    g.link(12, 1, 22, 4, true);
    g.remove(1);
    g.node(1, "Audio/Source");
    check(!g.capturing(), "recycled microphone ID counted");
}
