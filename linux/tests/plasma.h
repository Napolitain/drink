#pragma once
#include <QObject>

class ScreenSaver : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.ScreenSaver")
public:
    bool locked = true;
public slots:
    bool GetActive() const { return locked; }
signals:
    void ActiveChanged(bool active);
};

class LoginManager : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.login1.Manager")
signals:
    void PrepareForSleep(bool active);
};
