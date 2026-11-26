#pragma once
#include <QDBusServiceWatcher>
#include <QObject>
#include <QTimer>

namespace kde {
class Session : public QObject {
    Q_OBJECT
public:
    Session();
    [[nodiscard]] bool present() const { return unlocked && !sleeping; }

signals:
    void changed();

private:
    Q_SLOT void locked(bool active);
    Q_SLOT void sleep(bool active);
    void readLock();
    bool unlocked = false;
    bool sleeping = false;
    QTimer retry;
    QDBusServiceWatcher watch;
};
} // namespace kde
