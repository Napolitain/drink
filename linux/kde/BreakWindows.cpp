#include "BreakWindows.h"
#include <LayerShellQt/Window>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QPainter>
#include <QScreen>
#include <QTimer>
#include <algorithm>

namespace kde {
class BreakWindow : public QRasterWindow {
public:
    BreakWindow(QScreen *output, BreakWindows *owner) {
        setTitle("Drink");
        setScreen(output);
        setFlags(Qt::Window | Qt::FramelessWindowHint);
        using Layer = LayerShellQt::Window;
        auto *layer = Layer::get(this);
        layer->setScope("drink-break");
        layer->setScreen(output);
        layer->setAnchors({Layer::AnchorTop, Layer::AnchorBottom,
                           Layer::AnchorLeft, Layer::AnchorRight});
        layer->setExclusiveZone(
            -1); // Cover panels without moving other windows.
        layer->setLayer(Layer::LayerOverlay);
        layer->setKeyboardInteractivity(Layer::KeyboardInteractivityExclusive);
        resize(output->size());
        hold.setSingleShot(true);
        hold.setTimerType(Qt::PreciseTimer);
        hold.setInterval(2000);
        connect(&hold, &QTimer::timeout, owner, &BreakWindows::dismissed);
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.fillRect(QRect(QPoint(), size()), QColor("#0e1f24"));
        const int unit = std::clamp(width() / 22, 12, 48);
        QFont font = painter.font();
        font.setPixelSize(unit);
        font.setWeight(QFont::DemiBold);
        painter.setFont(font);
        painter.setPen(Qt::white);
        painter.drawText(QRect(0, 0, width(), height() / 2),
                         Qt::AlignBottom | Qt::AlignHCenter, "Get some water.");
        font.setPixelSize(unit * 2 / 3);
        font.setWeight(QFont::Normal);
        painter.setFont(font);
        painter.setPen(QColor("#d5dcdc"));
        painter.drawText(QRect(0, height() / 2 + 10, width(), height() / 2),
                         Qt::AlignTop | Qt::AlignHCenter,
                         "Look away and blink.");
    }

    void keyPressEvent(QKeyEvent *key) override {
        if (key->key() == Qt::Key_Escape && !key->isAutoRepeat() &&
            !hold.isActive())
            hold.start();
    }
    void keyReleaseEvent(QKeyEvent *key) override {
        if (key->key() == Qt::Key_Escape && !key->isAutoRepeat())
            hold.stop();
    }
    void focusOutEvent(QFocusEvent *) override { hold.stop(); }

private:
    QTimer hold;
};

void BreakWindows::sync(bool visible) {
    const auto screens = QGuiApplication::screens();
    std::erase_if(windows, [&](const auto &entry) {
        return !visible || !screens.contains(entry.first);
    });
    if (visible) {
        for (auto *screen : screens) {
            if (!screen->name().isEmpty() && !windows.contains(screen)) {
                auto window = std::make_unique<BreakWindow>(screen, this);
                window->show();
                windows.emplace(screen, std::move(window));
            }
        }
    }
}
} // namespace kde
