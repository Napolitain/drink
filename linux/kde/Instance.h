#pragma once

#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusServiceWatcher>
#include <QObject>
#include <cstdint>

namespace kde {
// One role name per flavor, and one shared name for the active reminder.
// A displaced release stays on the bus as a guardian while Dev owns the tray.
class Instance : public QObject, protected QDBusContext {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "io.github.napolitain.Drink.Instance")

public:
    enum class Start : std::uint8_t { Active, Duplicate, Failed };
    explicit Instance(bool development);
    ~Instance() override;
    Start start();
    Q_SCRIPTABLE Q_SLOT bool park();
    Q_SCRIPTABLE Q_SLOT void quit();

signals:
    void activate();
    void deactivate();

private:
    Q_DISABLE_COPY_MOVE(Instance)
    Start startLocked();
    bool claimReminder();
    void releaseReminder();
    void resume();

    bool dev;
    bool active = false;
    bool parked = false;
    bool ownsRole = false;
    QDBusConnection bus;
    QDBusServiceWatcher watch;
};
} // namespace kde
