#include "Instance.h"
#include "Foreground.h"
#include <QCoreApplication>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusReply>
#include <QLockFile>
#include <QStandardPaths>
#include <QTimer>

namespace kde {
namespace {
constexpr auto devRole = "io.github.napolitain.Drink.Development";
constexpr auto releaseRole = "io.github.napolitain.Drink.Release";
constexpr auto objectPath = "/Instance";
constexpr auto interface = "io.github.napolitain.Drink.Instance";

QString lockPath() {
    return QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation) +
           "/io.github.napolitain.Drink.start.lock";
}

QString owner(const QDBusConnection &bus, const QString &name) {
    return bus.interface()->serviceOwner(name).value();
}

void request(const QDBusConnection &bus, const QString &peer,
             const QString &method) {
    bus.call(
        QDBusMessage::createMethodCall(peer, objectPath, interface, method),
        QDBus::Block, 8000);
}

bool quitDevelopment(const QDBusConnection &bus) {
    const auto peer = owner(bus, devRole);
    if (peer.isEmpty()) {
        return true;
    }
    request(bus, peer, "quit");
    if (owner(bus, devRole) != peer) {
        return true;
    }
    qCritical("Drink Dev did not release the desktop session.");
    return false;
}
} // namespace

Instance::Instance(bool development)
    : dev(development), bus(QDBusConnection::sessionBus()),
      watch(devRole, bus, QDBusServiceWatcher::WatchForUnregistration, this) {
    connect(&watch, &QDBusServiceWatcher::serviceUnregistered, this,
            &Instance::resume);
}

Instance::~Instance() {
    releaseReminder();
    if (ownsRole) {
        bus.unregisterService(dev ? devRole : releaseRole);
    }
}

Instance::Start Instance::start() {
    if (!bus.isConnected() ||
        !bus.registerObject(objectPath, this,
                            QDBusConnection::ExportScriptableSlots)) {
        qCritical("Drink needs a working session bus for desktop handoff.");
        return Start::Failed;
    }
    // Serialize competing launches, including two flavors starting together.
    // A running app always services park/quit without acquiring this lock.
    QLockFile lock(lockPath());
    if (!lock.tryLock(8000)) {
        qCritical("Timed out waiting for a Drink launch to finish.");
        return Start::Failed;
    }
    return startLocked();
}

Instance::Start Instance::startLocked() {
    const auto *const role = dev ? devRole : releaseRole;
    ownsRole = bus.registerService(role);
    // Opening any release ends Dev. A parked release then resumes itself.
    if (!dev && !quitDevelopment(bus)) {
        return Start::Failed;
    }
    if (!ownsRole) {
        return owner(bus, role).isEmpty() ? Start::Failed : Start::Duplicate;
    }

    const auto previous = owner(bus, busName);
    if (!previous.isEmpty()) {
        // Older builds own just the shared name and cannot yield their tray.
        if (!dev || previous != owner(bus, releaseRole)) {
            qWarning("An older Drink instance is running. Quit it from the "
                     "tray and launch this build again to switch.");
            return Start::Duplicate;
        }
        request(bus, previous, "park");
        if (owner(bus, busName) == previous) {
            qCritical("The running Drink could not yield to Drink Dev.");
            return Start::Failed;
        }
    }
    return claimReminder() ? Start::Active : Start::Failed;
}

bool Instance::claimReminder() {
    if (!bus.registerService(busName)) {
        return false;
    }
    active = true;
    parked = false;
    emit activate();
    return true;
}

void Instance::releaseReminder() {
    if (active) {
        active = false;
        emit deactivate(); // Destroy the tray, scheduler and observers first.
        bus.unregisterService(busName);
    }
}

bool Instance::park() {
    if (dev || !calledFromDBus() ||
        message().service() != owner(bus, devRole)) {
        return false;
    }
    parked = true;
    releaseReminder();
    return true;
}

void Instance::quit() {
    parked = false;
    releaseReminder();
    if (ownsRole) {
        bus.unregisterService(dev ? devRole : releaseRole);
        ownsRole = false;
    }
    QCoreApplication::quit();
}

void Instance::resume() {
    if (!parked || !ownsRole) {
        return;
    }
    QLockFile lock(lockPath());
    if (!lock.tryLock()) {
        QTimer::singleShot(100, this, &Instance::resume);
        return;
    }
    if (!owner(bus, devRole).isEmpty()) {
        return;
    }
    if (!claimReminder()) {
        qWarning("Could not restore Drink after the development session.");
        QCoreApplication::quit();
    }
}
} // namespace kde
