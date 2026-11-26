#include "Drink.h"
#include <QApplication>
#include <QDBusConnectionInterface>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    const auto desktops = qEnvironmentVariable("XDG_CURRENT_DESKTOP")
                              .split(':', Qt::SkipEmptyParts);
    if (!app.platformName().startsWith("wayland") ||
        (!desktops.isEmpty() &&
         !desktops.contains("KDE", Qt::CaseInsensitive))) {
        qCritical("Drink for Plasma requires a KDE Wayland session.");
        return 1;
    }
    app.setApplicationName("Drink");
    app.setOrganizationName("Napolitain");
    app.setDesktopFileName("io.github.napolitain.Drink");
    app.setQuitOnLastWindowClosed(false);
    auto bus = QDBusConnection::sessionBus();
    if (!bus.isConnected()) {
        qCritical("Drink requires the Plasma session bus.");
        return 1;
    }
    if (!bus.registerService(kde::busName))
        return bus.interface()->isServiceRegistered(kde::busName).value() ? 0
                                                                          : 1;
    kde::Drink drink;
    return app.exec();
}
