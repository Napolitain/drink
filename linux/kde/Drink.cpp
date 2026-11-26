#include "Drink.h"
#include <QApplication>
#include <QScreen>
#include <algorithm>
#include <chrono>

namespace kde {
Drink::Drink(std::function<std::uint64_t()> clock)
    : now(std::move(clock)), scheduler(drink::new_scheduler(now())) {
    timer.setSingleShot(true);
    timer.setTimerType(Qt::PreciseTimer);
    connect(&timer, &QTimer::timeout, this, &Drink::update);
    connect(&session, &Session::changed, this, &Drink::update);
    connect(&foreground, &Foreground::changed, this, &Drink::update);
    connect(&foreground, &Foreground::otherAppActivated, this,
            &Drink::finishBreak);
    connect(
        qApp, &QGuiApplication::focusWindowChanged, this,
        [this](QWindow *window) {
            if (!window && !foreground.available)
                finishBreak();
        },
        Qt::QueuedConnection);
    connect(qApp, &QGuiApplication::screenAdded, this, &Drink::update);
    connect(qApp, &QGuiApplication::screenRemoved, this, &Drink::update);
    connect(&windows, &BreakWindows::dismissed, this, &Drink::finishBreak,
            Qt::QueuedConnection);
    setupMenu();
    connectAudio();
    update();
}

void Drink::finishBreak() {
    if (scheduler->visible()) {
        scheduler->restart(now());
        update();
    }
}

void Drink::setupMenu() {
    tray.setStandardActionsEnabled(false);
    tray.setIsMenu(true);
    tray.setTitle("Drink");
    tray.setIconByPixmap(QIcon(":/drink/drink.svg"));
    tray.setStatus(KStatusNotifierItem::Active);
    auto &menu = *tray.contextMenu();
    status = menu.addAction(QString());
    micStatus = menu.addAction("Microphone status unavailable");
    gameStatus = menu.addAction("Foreground game status unavailable");
    for (auto *label : {status, micStatus, gameStatus})
        label->setEnabled(false);
    menu.addSeparator();
    menu.addAction("Take break now", this, [this] {
        scheduler->request_break(now(), present(), blocked());
        update();
    });
    pause = menu.addAction("Pause for 1 hour", this, [this] {
        if (scheduler->paused())
            scheduler->restart(now());
        else
            scheduler->pause(now());
        update();
    });
    menu.addSeparator();
    menu.addAction("Quit Drink", qApp, &QCoreApplication::quit);
    connect(&menu, &QMenu::aboutToShow, this, &Drink::refreshMenu);
}

void Drink::connectAudio() {
    audioEvents.reset();
    microphone.reset();
    microphone = std::make_unique<drink::Microphone>();
    if (microphone->fd() < 0) {
        QTimer::singleShot(5000, this, &Drink::connectAudio);
        return;
    }
    audioEvents = std::make_unique<QSocketNotifier>(microphone->fd(),
                                                    QSocketNotifier::Read);
    connect(audioEvents.get(), &QSocketNotifier::activated, this, [this] {
        const auto sample = microphone->dispatch();
        if (microphone->fd() < 0) {
            audioEvents->setEnabled(false);
            QTimer::singleShot(5000, this, &Drink::connectAudio);
        }
        if (sample != audio) {
            audio = sample;
            update();
        }
    });
}

std::uint64_t Drink::monotonicMilliseconds() {
    using namespace std::chrono;
    return duration_cast<milliseconds>(steady_clock::now().time_since_epoch())
        .count();
}

bool Drink::present() const {
    return session.present() &&
           std::ranges::any_of(qApp->screens(),
                               [](auto *s) { return !s->name().isEmpty(); });
}
bool Drink::blocked() const {
    return audio == drink::Activity::Active || foreground.active;
}

void Drink::update() {
    scheduler->update(now(), present(), blocked());
    windows.sync(scheduler->visible());
    const auto deadline = scheduler->deadline();
    timer.stop();
    if (deadline) {
        const auto current = now();
        timer.start(
            static_cast<int>(deadline > current ? deadline - current : 0));
    }
    refreshMenu();
}

void Drink::refreshMenu() {
    const auto time = now(), end = scheduler->deadline();
    const auto minutes = end > time ? (end - time + 59999) / 60000 : 0;
    QString text;
    if (scheduler->paused())
        text = QString("Paused · %1 min left").arg(minutes);
    else if (!present())
        text = "Waiting for your return";
    else if (scheduler->visible())
        text = "Water break · hold Esc to end";
    else if (!scheduler->pending())
        text = QString("Next break in %1 min").arg(minutes);
    else if (audio == drink::Activity::Active)
        text = "Break due · microphone in use";
    else if (foreground.active)
        text = "Break due · game in foreground";
    else
        text = "Break due · waiting for ten quiet seconds";
    status->setText(text);
    pause->setText(scheduler->paused() ? "Resume" : "Pause for 1 hour");
    micStatus->setVisible(audio == drink::Activity::Unavailable);
    gameStatus->setVisible(!foreground.available);
}
} // namespace kde
