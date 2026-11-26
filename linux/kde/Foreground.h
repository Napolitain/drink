#pragma once

#include <QDBusContext>
#include <QDBusServiceWatcher>
#include <QObject>
#include <QTemporaryFile>
#include <QTimer>

namespace kde {
inline constexpr auto busName = "io.github.napolitain.Drink";

bool isGame(const QString &desktopFile, const QString &resourceClass,
            const QStringList &configured);

class Foreground : public QObject, protected QDBusContext {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "io.github.napolitain.Drink.Foreground")
public:
    Foreground();
    ~Foreground() override;
    [[nodiscard]] bool isAvailable() const { return available; }
    [[nodiscard]] bool gameActive() const { return active; }
    Q_SCRIPTABLE Q_SLOT void report(const QString &desktopFile,
                                    const QString &resourceClass, int pid);

signals:
    void changed();
    void otherAppActivated();

private:
    Q_DISABLE_COPY_MOVE(Foreground)
    void attach();
    QTemporaryFile script;
    QTimer retry;
    QString owner;
    QStringList configured;
    QDBusServiceWatcher watch;
    bool available = false;
    bool active = false;
};
} // namespace kde
