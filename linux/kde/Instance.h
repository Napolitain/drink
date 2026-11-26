#pragma once

#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusServiceWatcher>
#include <QObject>

namespace kde {
// One role name per flavor, and one shared name for the active reminder.
// A displaced release stays on the bus as a guardian while Dev owns the tray.
class Instance : public QObject, protected QDBusContext {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "io.github.napolitain.Drink.Instance")

public:
    enum class Start { Active, Duplicate, Failed };
    explicit Instance(bool development);
    ~Instance() override;
    Start start();

signals:
    void activate();
    void deactivate();

public slots:
    Q_SCRIPTABLE bool park();
    Q_SCRIPTABLE void quit();

private:
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
