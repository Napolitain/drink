#pragma once

#include <QDBusContext>
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
    bool available = false;
    bool active = false;

signals:
    void changed();
    void otherAppActivated();

public slots:
    Q_SCRIPTABLE void report(const QString &desktopFile,
                             const QString &resourceClass, int pid);

private:
    void attach();
    QTemporaryFile script;
    QTimer retry;
    QString owner;
    QStringList configured;
};
} // namespace kde
