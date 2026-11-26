#include "Foreground.h"
#include <KService>
#include <QCoreApplication>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusReply>
#include <QDBusServiceWatcher>
#include <QFileInfo>
#include <QRegularExpression>
#include <QSettings>
#include <QStandardPaths>
#include <algorithm>

namespace kde {
static const QString kwin = QStringLiteral("org.kde.KWin");
static const QString scripting = QStringLiteral("org.kde.kwin.Scripting");

static QDBusMessage scriptCall(const QString &method,
                               const QVariantList &args) {
    auto call =
        QDBusMessage::createMethodCall(kwin, "/Scripting", scripting, method);
    call.setArguments(args);
    return QDBusConnection::sessionBus().call(call, QDBus::Block, 1500);
}

static bool hasGameCategory(const QString &id) {
    if (id.isEmpty()) {
        return false;
    }
    const auto service = KService::serviceByDesktopName(id);
    return service && service->categories().contains("Game");
}

bool isGame(const QString &desktopFile, const QString &resourceClass,
            const QStringList &configured) {
    QString app = QFileInfo(desktopFile).fileName();
    if (app.endsWith(".desktop")) {
        app.chop(8);
    }
    const QStringList ids{app, resourceClass};
    static const QRegularExpression steamGame(
        "^steam_app_[1-9][0-9]*$", QRegularExpression::CaseInsensitiveOption);
    const auto isProtected = [&configured](const QString &id) {
        return !id.isEmpty() && (configured.contains(id, Qt::CaseInsensitive) ||
                                 steamGame.match(id).hasMatch());
    };
    if (std::ranges::any_of(ids, isProtected)) {
        return true;
    }
    const auto isSteamLauncher = [](const QString &id) {
        return id.compare("steam", Qt::CaseInsensitive) == 0 ||
               id.compare("com.valvesoftware.Steam", Qt::CaseInsensitive) == 0;
    };
    return std::ranges::none_of(ids, isSteamLauncher) &&
           std::ranges::any_of(ids, hasGameCategory);
}

Foreground::Foreground()
    : script(QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation) +
             "/drink-XXXXXX.js"),
      configured(QSettings().value("ProtectedGames").toStringList()),
      watch(kwin, QDBusConnection::sessionBus(),
            QDBusServiceWatcher::WatchForOwnerChange) {
    auto bus = QDBusConnection::sessionBus();
    QFile source(":/drink/foreground.js");
    if (!source.open(QIODevice::ReadOnly) || !script.open()) {
        return;
    }
    const QByteArray contents =
        "const drinkPid = " +
        QByteArray::number(QCoreApplication::applicationPid()) + ";\n" +
        source.readAll();
    if (script.write(contents) != contents.size() || !script.flush() ||
        !bus.registerObject("/Foreground", this,
                            QDBusConnection::ExportScriptableSlots)) {
        return;
    }
    connect(&watch, &QDBusServiceWatcher::serviceOwnerChanged, this, [this] {
        available = false;
        active = false;
        emit changed();
        attach();
    });
    retry.setSingleShot(true);
    retry.setInterval(10000);
    connect(&retry, &QTimer::timeout, this, &Foreground::attach);
    attach();
}

void Foreground::attach() {
    auto bus = QDBusConnection::sessionBus();
    owner = bus.interface()->serviceOwner(kwin).value();
    retry.start();
    if (owner.isEmpty()) {
        return;
    }
    // Remove a previous instance's observer after an unclean exit.
    scriptCall("unloadScript", {busName});
    QDBusReply<int> id = scriptCall("loadScript", {script.fileName(), busName});
    if (!id.isValid() || id.value() < 0) {
        return;
    }
    auto run = QDBusMessage::createMethodCall(
        kwin, QString("/Scripting/Script%1").arg(id.value()),
        "org.kde.kwin.Script", "run");
    bus.asyncCall(run);
}

void Foreground::report(const QString &desktopFile,
                        const QString &resourceClass, int pid) {
    if (!calledFromDBus() || message().service() != owner) {
        return;
    }
    retry.stop();
    const bool game = isGame(desktopFile, resourceClass, configured);
    if (!available || active != game) {
        active = game;
        available = true;
        emit changed();
    }
    if (pid > 0 && pid != QCoreApplication::applicationPid()) {
        emit otherAppActivated();
    }
}

Foreground::~Foreground() {
    if (!owner.isEmpty()) {
        scriptCall("unloadScript", {busName});
    }
    auto bus = QDBusConnection::sessionBus();
    if (bus.objectRegisteredAt("/Foreground") == this) {
        bus.unregisterObject("/Foreground");
    }
}
} // namespace kde
