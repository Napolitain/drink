// Run through tests/plasma-session so the lock service, audio routing and
// foreground changes all belong to this disposable compositor.
#include "plasma.h"
#include "Drink.h"
#include <QApplication>
#include <QDBusConnection>
#include <QElapsedTimer>
#include <QFile>
#include <QProcess>
#include <QScreen>
#include <QStandardPaths>
#include <QTest>
#include <QWidget>
#include <algorithm>
#include <iostream>
#include <stdexcept>

static void require(bool okay, const char *reason) {
    if (!okay) {
        throw std::runtime_error(reason);
    }
}

static QAction *action(const QString &text) {
    for (auto *widget : QApplication::topLevelWidgets()) {
        if (auto *menu = qobject_cast<QMenu *>(widget)) {
            for (auto *item : menu->actions()) {
                if (item->text().startsWith(text)) {
                    return item;
                }
            }
        }
    }
    return nullptr;
}

static QList<QWindow *> overlays() {
    QList<QWindow *> result;
    for (auto *window : QGuiApplication::topLevelWindows()) {
        if (window->title() == "Drink" && window->isExposed()) {
            result.append(window);
        }
    }
    return result;
}

static void until(auto predicate, const char *reason) {
    require(QTest::qWaitFor(predicate, 5000), reason);
}

static void startClient(QProcess &client, const QString &id) {
    client.start(QCoreApplication::applicationFilePath(), {"--client", id});
    require(client.waitForStarted(), "start Wayland client");
    until([&] { return client.canReadLine(); }, "client got keyboard focus");
    require(client.readLine().trimmed() == "focused",
            "unexpected client focus state");
}

static int runClient(const QApplication &app, const QString &desktopId) {
    QGuiApplication::setDesktopFileName(desktopId);
    QWidget window;
    QObject::connect(&app, &QGuiApplication::focusWindowChanged,
                     [](const QWindow *focused) {
                         if (focused != nullptr) {
                             std::cout << "focused\n" << std::flush;
                         }
                     });
    window.show();
    return QApplication::exec();
}

static int exercise() {
    require(qEnvironmentVariableIsSet("DRINK_TEST_SESSION"),
            "start with tests/plasma-session");
    require(QGuiApplication::platformName() == "wayland" &&
                QGuiApplication::screens().size() == 2,
            "two virtual Wayland outputs");
    QProcess audioServer;
    auto startAudio = [&] {
        audioServer.start("pipewire", {});
        require(audioServer.waitForStarted(), "start private PipeWire server");
        until(
            [] {
                return QFile::exists(QStandardPaths::writableLocation(
                                         QStandardPaths::RuntimeLocation) +
                                     "/pipewire-0");
            },
            "PipeWire socket");
    };
    startAudio();
    auto bus = QDBusConnection::sessionBus();
    ScreenSaver saver;
    LoginManager login;
    auto system = QDBusConnection::systemBus();
    require(system.baseService() != bus.baseService() &&
                system.registerService("org.freedesktop.login1") &&
                system.registerObject("/org/freedesktop/login1", &login,
                                      QDBusConnection::ExportAllSignals),
            "private login service");
    require(bus.registerService(kde::busName) &&
                bus.registerService("org.freedesktop.ScreenSaver"),
            "isolated session bus");
    require(bus.registerObject("/ScreenSaver", &saver,
                               QDBusConnection::ExportAllSlots |
                                   QDBusConnection::ExportAllSignals),
            "lock service");
    QCoreApplication::setApplicationName("Drink");
    QCoreApplication::setOrganizationName("Napolitain");
    QElapsedTimer elapsed;
    elapsed.start();
    std::uint64_t offset = 0;
    kde::Drink drink(nullptr, [&] {
        return static_cast<std::uint64_t>(elapsed.elapsed()) + offset;
    });
    const auto *original = bus.objectRegisteredAt("/Foreground");
    require(original != nullptr, "foreground observer registered");
    {
        const kde::Foreground duplicate;
        require(!duplicate.isAvailable(),
                "second foreground observer could not register");
    }
    require(
        bus.objectRegisteredAt("/Foreground") == original,
        "destroying the duplicate preserved the active foreground observer");
    int finished = 0;
    QObject::connect(&drink, &kde::Drink::breakFinished, [&] { ++finished; });
    QProcess second;
    second.start(DRINK_TEST_PROGRAM, {});
    require(second.waitForFinished(3000) &&
                second.exitStatus() == QProcess::NormalExit &&
                second.exitCode() == 0,
            "duplicate launch exited cleanly");
    until(
        [] {
            return !action("Microphone status")->isVisible() &&
                   !action("Foreground game status")->isVisible();
        },
        "PipeWire and KWin observers online");
    auto *take = action("Take break now");
    offset += 1800000;
    drink.update();
    take->trigger();
    require(action("Waiting for your return") != nullptr &&
                overlays().isEmpty(),
            "lock suppressed timed and manual breaks");
    saver.unlock();
    until([] { return action("Next break in 30 min"); }, "unlock starts fresh");

    // Steam the launcher should allow an ordinary timed break.
    QProcess editor;
    startClient(editor, "steam");
    offset += 1799900;
    drink.update();
    until(
        [] {
            return overlays().size() == 2 &&
                   overlays().contains(QGuiApplication::focusWindow());
        },
        "timed overlay covered two outputs with focus");
    for (auto *screen : QGuiApplication::screens()) {
        const auto visible = overlays();
        require(std::ranges::any_of(visible,
                                    [screen](QWindow *window) {
                                        return window->screen() == screen &&
                                               window->size() == screen->size();
                                    }),
                "each surface fills its own output");
    }
    // Let the real 30-second deadline expire and KWin return keyboard focus.
    const auto visibleSince = elapsed.elapsed();
    require(QTest::qWaitFor([] { return overlays().isEmpty(); }, 32000),
            "automatic dismissal");
    require(elapsed.elapsed() - visibleSince >= 28500,
            "overlay closed before its deadline");
    until([&] { return editor.canReadLine(); },
          "previous window regained focus");
    require(finished == 1, "timed break can finish a staged update");
    take->trigger();
    until([] { return overlays().contains(QGuiApplication::focusWindow()); },
          "manual break");
    QTest::keyClick(QGuiApplication::focusWindow(), Qt::Key_Escape);
    QTest::qWait(2100);
    require(overlays().size() == 2, "short Escape tap kept the break");
    QTest::keyPress(QGuiApplication::focusWindow(), Qt::Key_Escape);
    QTest::qWait(1200);
    require(overlays().size() == 2, "a partial Escape hold kept the break");
    until([] { return overlays().isEmpty(); },
          "Escape hold dismissed the break");
    require(finished == 2, "deliberate dismissal can finish a staged update");

    take->trigger();
    auto *pause = action("Pause for 1 hour");
    pause->trigger();
    require(action("Paused") != nullptr && action("Resume") != nullptr,
            "pause suppresses the break");
    require(finished == 2, "pausing a break leaves a staged update waiting");
    pause->trigger();
    require(action("Next break in 30 min") != nullptr,
            "resume starts a fresh interval");
    QProcess game;
    startClient(game, "steam_app_1234");
    take->trigger();
    until([] { return action("Break due · game in foreground"); },
          "foreground game deferred manual break");
    require(overlays().isEmpty(), "no overlay over the game");
    game.terminate();
    require(game.waitForFinished(), "close game");
    until([] { return action("Break due · waiting"); },
          "game exit starts grace");

    // The private server's source is a tone generator. It has no hardware
    // devices.
    auto link = [](QStringList args) {
        args << "test-mic:capture_FL" << "test-call:input_AUX0";
        require(QProcess::execute("pw-link", args) == 0,
                "change synthetic capture route");
    };
    link({});
    until([] { return action("Break due · microphone"); },
          "microphone interrupted grace");
    link({"-d"});
    until([] { return action("Break due · waiting"); },
          "unlink starts grace again");
    offset += 9900;
    drink.update();
    until([] { return overlays().size() == 2; }, "quiet grace completed");
    link({});
    until(
        [] { return overlays().isEmpty() && action("Break due · microphone"); },
        "new microphone interrupted visible break");
    require(finished == 2, "interrupted break leaves a staged update waiting");
    emit login.PrepareForSleep(true);
    until([] { return action("Waiting for your return"); },
          "suspend cleared pending break");
    offset += 1800000;
    emit login.PrepareForSleep(false);
    until([] { return action("Next break in 30 min"); }, "wake starts fresh");
    audioServer.terminate();
    require(audioServer.waitForFinished(), "stop private PipeWire server");
    until([] { return action("Microphone status")->isVisible(); },
          "lost audio connection is reported");
    take->trigger();
    until([] { return overlays().size() == 2; },
          "reminders usable during audio outage");
    pause->trigger();
    startAudio();
    require(
        QTest::qWaitFor(
            [] { return !action("Microphone status")->isVisible(); }, 12000),
        "PipeWire observer reconnected");
    audioServer.terminate();
    require(audioServer.waitForFinished(), "close private PipeWire server");
    editor.terminate();
    editor.waitForFinished();
    require(kde::isGame("example.desktop", "other", {"example"}),
            "configured game ID");
    require(kde::isGame("org.example.DrinkGame.desktop", "other", {}),
            "desktop entry in the Game category");
    require(!kde::isGame("com.valvesoftware.Steam", "steam", {}),
            "Steam launcher");
    qInfo("PASS: two KWin outputs, focus and return, timed and manual breaks, "
          "Escape, pause, lock, wake, foreground games, live PipeWire routes "
          "and reconnect");
    return 0;
}

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QApplication::setQuitOnLastWindowClosed(false);
    try {
        const auto args = QCoreApplication::arguments();
        if (args.size() == 3 && args[1] == "--client") {
            return runClient(app, args[2]);
        }
        return exercise();
    } catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << '\n';
        return 1;
    }
}
