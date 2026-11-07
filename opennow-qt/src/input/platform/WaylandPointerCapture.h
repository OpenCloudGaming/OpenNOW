#pragma once

#include <QObject>
#include <QPointF>
#include <QRect>
#include <memory>

class QWindow;

class WaylandPointerCapture final : public QObject
{
    Q_OBJECT
public:
    explicit WaylandPointerCapture(QObject *parent = nullptr);
    ~WaylandPointerCapture() override;

    [[nodiscard]] static bool isWayland();
    [[nodiscard]] bool locked() const;
    // Relative input can run: the pointer is locked, or the seat has no pointer
    // to lock (a touchscreen-only seat), so no mouse input can reach the window.
    [[nodiscard]] bool relativeInputReady() const;
    [[nodiscard]] QString error() const;
    void setCapture(QWindow *window, bool enabled, const QRect &surfaceRegion);
    void release();
    [[nodiscard]] static QPoint boundedDelta(QPointF &remainder, const QPointF &delta);

signals:
    void stateChanged();
    void relativeMotion(qint16 x, qint16 y);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    struct Private;
    std::unique_ptr<Private> d;
};
