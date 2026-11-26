#pragma once

#include <QObject>
#include <QProcess>
#include <QTimer>
#include <cstdint>

namespace kde {
class AppImageUpdate : public QObject {
    Q_OBJECT
public:
    explicit AppImageUpdate(QObject *parent = nullptr);
    ~AppImageUpdate() override;
    [[nodiscard]] bool enabled() const { return !image.isEmpty(); }
    [[nodiscard]] bool checking() const { return state == State::Checking; }
    [[nodiscard]] bool ready() const { return state == State::Installed; }
    [[nodiscard]] QString menuText() const;

    // Subprocesses outside the mounted image need the host Qt environment.
    static QProcessEnvironment hostEnvironment();

    Q_SLOT void check();
    Q_SLOT void restartIfReady();

signals:
    void changed();
    void restartRequested(const QString &outerPath);

private:
    Q_DISABLE_COPY_MOVE(AppImageUpdate)
    enum class State : std::uint8_t {
        Idle,
        Checking,
        Current,
        Failed,
        Installed
    };
    void setState(State value);
    QString image, initialIdentity;
    State state = State::Idle;
    QProcess worker;
    QTimer daily;
};
} // namespace kde
