#include "Session.h"
#include <QDBusConnection>
#include <QDBusReply>
#include <QDBusServiceWatcher>

namespace kde {
static const QString saver = QStringLiteral("org.freedesktop.ScreenSaver");
Session::Session()
    : watch(saver, QDBusConnection::sessionBus(),
            QDBusServiceWatcher::WatchForOwnerChange) {
    auto bus = QDBusConnection::sessionBus();
    bus.connect(saver, "/ScreenSaver", saver, "ActiveChanged", this,
                SLOT(locked(bool)));
    connect(&watch, &QDBusServiceWatcher::serviceOwnerChanged, this,
            &Session::readLock);
    QDBusConnection::systemBus().connect(
        "org.freedesktop.login1", "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager", "PrepareForSleep", this,
        SLOT(sleep(bool)));
    retry.setSingleShot(true);
    retry.setInterval(5000);
    connect(&retry, &QTimer::timeout, this, &Session::readLock);
    readLock();
}

void Session::readLock() {
    auto call = QDBusMessage::createMethodCall(saver, "/ScreenSaver", saver,
                                               "GetActive");
    QDBusReply<bool> active =
        QDBusConnection::sessionBus().call(call, QDBus::Block, 1500);
    locked(!active.isValid() || active.value());
    if (!active.isValid()) {
        retry.start();
    }
}

void Session::locked(bool active) {
    retry.stop();
    unlocked = !active;
    emit changed();
}

void Session::sleep(bool active) {
    sleeping = active;
    if (!active) {
        readLock();
    } else {
        emit changed();
    }
}
} // namespace kde
