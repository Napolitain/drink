#pragma once
#include <QObject>
#include <QRasterWindow>
#include <map>
#include <memory>

namespace kde {
class BreakWindows : public QObject {
    Q_OBJECT
public:
    void sync(bool visible);

signals:
    void dismissed();

private:
    std::map<QScreen *, std::unique_ptr<QRasterWindow>> windows;
};
} // namespace kde
