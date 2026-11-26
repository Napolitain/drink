#include "Drink.h"
#include "Instance.h"
#ifdef DRINK_APPIMAGE
#include "AppImageUpdate.h"
#include <QFileInfo>
#include <QProcess>
#endif
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

int runReminder(QApplication &app, kde::AppImageUpdate *updates = nullptr) {
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
        return app.exec();
    }
    return 1;
}

#ifdef DRINK_APPIMAGE
int runAppImage(QApplication &app) {
    kde::AppImageUpdate updates;
    QString restartPath;
    QObject::connect(&updates, &kde::AppImageUpdate::restartRequested, &app,
                     [&](const QString &path) {
                         restartPath = path;
                         app.quit();
                     });
    for (;;) {
        restartPath.clear();
        const int result = runReminder(app, &updates);
        if (restartPath.isEmpty())
            return result;
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
                .startsWith("appimage_extracted_"))
            env.insert("APPIMAGE_EXTRACT_AND_RUN", "1");
        next.setProcessEnvironment(env);
        if (next.startDetached())
            return 0;
        qWarning("Could not relaunch the updated AppImage: %s",
                 qPrintable(next.errorString()));
    }
}
#endif
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
#ifdef DRINK_APPIMAGE
    return runAppImage(app);
#else
    return runReminder(app);
#endif
}
