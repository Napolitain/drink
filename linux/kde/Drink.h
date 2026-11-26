#pragma once
#include "BreakWindows.h"
#include "Foreground.h"
#include "Microphone.h"
#include "Session.h"
#include "drink-macos-bridge/src/lib.rs.h"
#include <KStatusNotifierItem>
#include <QMenu>
#include <QSocketNotifier>
#include <functional>

namespace kde {
class Drink : public QObject {
public:
    explicit Drink(
        std::function<std::uint64_t()> clock = monotonicMilliseconds);
    void update();

private:
    static std::uint64_t monotonicMilliseconds();
    void setupMenu();
    void finishBreak();
    void connectAudio();
    void refreshMenu();
    bool present() const;
    bool blocked() const;
    std::function<std::uint64_t()> now;
    rust::Box<drink::Scheduler> scheduler;
    Session session;
    Foreground foreground;
    BreakWindows windows;
    std::unique_ptr<drink::Microphone> microphone;
    std::unique_ptr<QSocketNotifier> audioEvents;
    drink::Activity audio = drink::Activity::Unavailable;
    QTimer timer;
    KStatusNotifierItem tray;
    QAction *status, *micStatus, *gameStatus, *pause;
};
} // namespace kde
