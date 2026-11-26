#include "Drink.h"
#include "Instance.h"
#include <QApplication>
#include <cstdio>
#include <cstring>
#include <memory>

namespace {
#ifdef DRINK_RELEASE_VERSION
constexpr bool development = false;
constexpr auto version = DRINK_RELEASE_VERSION;
#else
constexpr bool development = true;
constexpr auto version = "development";
#endif
constexpr auto name = development ? "Drink Dev" : "Drink";
} // namespace

int main(int argc, char **argv) {
    QCoreApplication::setApplicationName(name);
    QCoreApplication::setApplicationVersion(version);
    QCoreApplication::setOrganizationName("Napolitain");
    if (argc == 2 && (std::strcmp(argv[1], "--version") == 0 ||
                      std::strcmp(argv[1], "-v") == 0)) {
        std::printf("%s %s\n", name, version);
        return 0;
    }
    QApplication app(argc, argv);
    const auto desktops = qEnvironmentVariable("XDG_CURRENT_DESKTOP")
                              .split(':', Qt::SkipEmptyParts);
    if (!app.platformName().startsWith("wayland") ||
        (!desktops.isEmpty() &&
         !desktops.contains("KDE", Qt::CaseInsensitive))) {
        qCritical("Drink for Plasma requires a KDE Wayland session.");
        return 1;
    }
    app.setDesktopFileName(development ? "io.github.napolitain.Drink.Dev"
                                       : "io.github.napolitain.Drink");
    app.setQuitOnLastWindowClosed(false);
    app.setQuitLockEnabled(false);
    std::unique_ptr<kde::Drink> reminder;
    kde::Instance instance(development);
    QObject::connect(&instance, &kde::Instance::activate,
                     [&] { reminder = std::make_unique<kde::Drink>(); });
    QObject::connect(&instance, &kde::Instance::deactivate,
                     [&] { reminder.reset(); });
    switch (instance.start()) {
    case kde::Instance::Start::Duplicate:
        return 0;
    case kde::Instance::Start::Failed:
        return 1;
    case kde::Instance::Start::Active:
        return app.exec();
    }
    return 1;
}
