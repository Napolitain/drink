#include "AppImageUpdate.h"
#include <QCoreApplication>
#include <QDate>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <csignal>
#include <sys/stat.h>

namespace kde {
namespace {
const auto day = std::chrono::hours(24);

QByteArray resource(const QString &path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray{};
}

// Follow launcher symlinks once, then update only this outer image. Requiring
// the mounted executable also excludes an ordinary program launched by an
// unrelated AppImage that happened to pass along APPDIR and APPIMAGE.
QString runningImage() {
    const auto appdir = qEnvironmentVariable("APPDIR");
    const auto outer = qEnvironmentVariable("APPIMAGE");
    const auto version = QCoreApplication::applicationVersion();
    const auto date = QDate::fromString(version, "yyyy.MM.dd");
    if (!QDir::isAbsolutePath(appdir) || !QDir::isAbsolutePath(outer) ||
        !date.isValid() || date.toString("yyyy.MM.dd") != version)
        return {};
    const auto executable =
        QFileInfo(QCoreApplication::applicationFilePath()).canonicalFilePath();
    if (executable.isEmpty() ||
        executable !=
            QFileInfo(appdir + "/usr/bin/drink-kde").canonicalFilePath())
        return {};
    return QFileInfo(outer).canonicalFilePath();
}
} // namespace

AppImageUpdate::AppImageUpdate(QObject *parent)
    : QObject(parent), image(runningImage()) {
    if (!enabled())
        return;
    struct stat info{};
    if (::stat(QFile::encodeName(image).constData(), &info) != 0 ||
        !S_ISREG(info.st_mode)) {
        image.clear();
        return;
    }
    initialIdentity = QString("%1:%2").arg(info.st_dev).arg(info.st_ino);
    worker.setUnixProcessParameters(
        QProcess::UnixProcessFlag::CreateNewSession);
    connect(&worker, &QProcess::finished, this,
            [this](int code, QProcess::ExitStatus exit) {
                const auto result = worker.readAllStandardOutput().trimmed();
                if (exit == QProcess::NormalExit && code == 0 &&
                    result == "installed") {
                    daily.stop();
                    setState(State::Installed);
                } else if (exit == QProcess::NormalExit && code == 0 &&
                           result == "current") {
                    setState(State::Current);
                } else {
                    qWarning().noquote()
                        << "Drink update:"
                        << worker.readAllStandardError().trimmed();
                    setState(State::Failed);
                }
            });
    connect(&worker, &QProcess::errorOccurred, this,
            [this](QProcess::ProcessError error) {
                if (error == QProcess::FailedToStart) {
                    qWarning().noquote()
                        << "Drink update:" << worker.errorString();
                    setState(State::Failed);
                }
            });
    connect(&daily, &QTimer::timeout, this, &AppImageUpdate::check);
    daily.start(std::chrono::duration_cast<std::chrono::milliseconds>(day));
    QTimer::singleShot(0, this, &AppImageUpdate::check);
}

AppImageUpdate::~AppImageUpdate() {
    // Stop the shell and its curl/verification subprocess together. The shell's
    // EXIT trap removes the private stage when a check is interrupted.
    if (worker.processId() > 0) {
        worker.disconnect(this);
        ::kill(-worker.processId(), SIGTERM);
        if (!worker.waitForFinished(1000) && worker.processId() > 0) {
            ::kill(-worker.processId(), SIGKILL);
            worker.waitForFinished();
        }
    }
}

QProcessEnvironment AppImageUpdate::hostEnvironment() {
    auto env = QProcessEnvironment::systemEnvironment();
    for (const QString &name :
         {QString("QT_PLUGIN_PATH"), QString("QT_QPA_PLATFORM_PLUGIN_PATH")}) {
        const auto saved = "DRINK_ORIGINAL_" + name;
        if (env.contains(saved))
            env.insert(name, env.value(saved));
        else
            env.remove(name);
        env.remove(saved);
    }
    for (const auto *name : {"APPIMAGE", "APPDIR", "ARGV0", "OWD"})
        env.remove(name);
    return env;
}

QString AppImageUpdate::menuText() const {
    switch (state) {
    case State::Checking:
        return "Checking for Updates…";
    case State::Current:
        return "Up to Date · Check Again";
    case State::Failed:
        return "Update Unavailable · Retry";
    case State::Installed:
        return "Restart to Finish Update";
    case State::Idle:
        return "Check for Updates…";
    }
    Q_UNREACHABLE();
}

void AppImageUpdate::setState(State value) {
    state = value;
    emit changed();
}

void AppImageUpdate::check() {
    if (!enabled() || checking() || ready())
        return;
    const auto helper = resource(":/drink/update-appimage");
    const auto signers = resource(":/drink/release-signers").trimmed();
    if (helper.isEmpty() || signers.isEmpty()) {
        setState(State::Failed);
        return;
    }
    setState(State::Checking);
    worker.setProcessEnvironment(hostEnvironment());
    worker.start("bash", {"-c", QString::fromUtf8(helper), "drink-update",
                          image, QCoreApplication::applicationVersion(),
                          initialIdentity, QString::fromUtf8(signers)});
}

void AppImageUpdate::restartIfReady() {
    if (ready())
        emit restartRequested(image);
}
} // namespace kde
