#pragma once
#include <QObject>

class ScreenSaver : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.ScreenSaver")
public:
    void unlock() {
        locked = false;
        emit ActiveChanged(false);
    }
    [[nodiscard]] Q_SLOT bool GetActive() const { return locked; }
signals:
    void ActiveChanged(bool active);

private:
    bool locked = true;
};

class LoginManager : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.login1.Manager")
signals:
    void PrepareForSleep(bool active);
};
