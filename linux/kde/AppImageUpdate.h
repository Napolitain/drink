#pragma once

#include <QObject>
#include <QProcess>
#include <QTimer>

namespace kde {
class AppImageUpdate : public QObject {
    Q_OBJECT
public:
    explicit AppImageUpdate(QObject *parent = nullptr);
    ~AppImageUpdate() override;
    bool enabled() const { return !image.isEmpty(); }
    bool checking() const { return state == State::Checking; }
    bool ready() const { return state == State::Installed; }
    QString menuText() const;

    // Subprocesses outside the mounted image need the host Qt environment.
    static QProcessEnvironment hostEnvironment();

public slots:
    void check();
    void restartIfReady();

signals:
    void changed();
    void restartRequested(const QString &outerPath);

private:
    enum class State { Idle, Checking, Current, Failed, Installed };
    void setState(State value);
    QString image, initialIdentity;
    State state = State::Idle;
    QProcess worker;
    QTimer daily;
};
} // namespace kde
