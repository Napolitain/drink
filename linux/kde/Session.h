#pragma once
#include <QObject>
#include <QTimer>

namespace kde {
class Session : public QObject {
    Q_OBJECT
public:
    Session();
    bool present() const { return unlocked && !sleeping; }

signals:
    void changed();

private slots:
    void locked(bool active);
    void sleep(bool active);
    void readLock();

private:
    bool unlocked = false;
    bool sleeping = false;
    QTimer retry;
};
} // namespace kde
