#include "Drink.h"
#include "Instance.h"
#ifdef DRINK_APPIMAGE
#include "AppImageUpdate.h"
#include <QFileInfo>
#include <QProcess>
#endif
#include <QApplication>
#include <iostream>
#include <memory>
#include <span>
#include <string_view>

namespace {
#ifdef DRINK_RELEASE_VERSION
constexpr bool development = false;
constexpr auto version = DRINK_RELEASE_VERSION;
#else
constexpr bool development = true;
constexpr auto version = "development";
#endif
constexpr auto name = development ? "Drink Dev" : "Drink";

int runReminder(kde::AppImageUpdate *updates = nullptr) {
    std::unique_ptr<kde::Drink> reminder;
    kde::Instance instance(development);
    QObject::connect(&instance, &kde::Instance::activate,
                     [&] { reminder = std::make_unique<kde::Drink>(updates); });
    QObject::connect(&instance, &kde::Instance::deactivate,
                     [&] { reminder.reset(); });
    switch (instance.start()) {
    case kde::Instance::Start::Duplicate:
        return 0;
    case kde::Instance::Start::Failed:
        return 1;
    case kde::Instance::Start::Active:
        return QApplication::exec();
    }
    return 1;
}

#ifdef DRINK_APPIMAGE
// Ordinary quit, duplicate activation and startup failure use 0 or 1.
constexpr int restartForUpdate = 2;

int runAppImage() {
    QString restartPath;
    kde::AppImageUpdate updates;
    QObject::connect(&updates, &kde::AppImageUpdate::restartRequested, &updates,
                     [&restartPath](const QString &path) {
                         restartPath = path;
                         QCoreApplication::exit(restartForUpdate);
                     });
    for (;;) {
        const int result = runReminder(&updates);
        if (result != restartForUpdate) {
            return result;
        }
        // The old tray and both D-Bus names have been released. Start the
        // updated outer image in the host environment, outside this mount.
        QProcess next;
        next.setProgram(restartPath);
        next.setWorkingDirectory(QFileInfo(restartPath).absolutePath());
        auto env = kde::AppImageUpdate::hostEnvironment();
        // The type-2 runtime consumes --appimage-extract-and-run before
        // main(). Preserve that launch mode on machines without FUSE.
        if (QFileInfo(qEnvironmentVariable("APPDIR"))
                .fileName()
                .startsWith("appimage_extracted_")) {
            env.insert("APPIMAGE_EXTRACT_AND_RUN", "1");
        }
        next.setProcessEnvironment(env);
        if (next.startDetached()) {
            return 0;
        }
        qWarning().noquote()
            << "Could not relaunch the updated AppImage:" << next.errorString();
    }
}
#endif
} // namespace

int main(int argc, char **argv) {
    QCoreApplication::setApplicationName(name);
    QCoreApplication::setApplicationVersion(version);
    QCoreApplication::setOrganizationName("Napolitain");
    const std::span arguments(argv, static_cast<std::size_t>(argc));
    if (arguments.size() == 2 &&
        (arguments.back() == std::string_view("--version") ||
         arguments.back() == std::string_view("-v"))) {
        std::cout << name << ' ' << version << '\n';
        return 0;
    }
    QApplication app(argc, argv);
    const auto desktops = qEnvironmentVariable("XDG_CURRENT_DESKTOP")
                              .split(':', Qt::SkipEmptyParts);
    if (!QGuiApplication::platformName().startsWith("wayland") ||
        (!desktops.isEmpty() &&
         !desktops.contains("KDE", Qt::CaseInsensitive))) {
        qCritical("Drink for Plasma requires a KDE Wayland session.");
        return 1;
    }
    QGuiApplication::setDesktopFileName(development
                                            ? "io.github.napolitain.Drink.Dev"
                                            : "io.github.napolitain.Drink");
    QGuiApplication::setQuitOnLastWindowClosed(false);
    QCoreApplication::setQuitLockEnabled(false);
#ifdef DRINK_APPIMAGE
    return runAppImage();
#else
    return runReminder();
#endif
}
