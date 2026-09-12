#pragma once

#include <QObject>
#include <QPointF>
#include <QRect>
#include <functional>
#include <memory>

class QWindow;

class MacPointerCapture final : public QObject
{
    Q_OBJECT
public:
    class NativeOperations
    {
    public:
        virtual ~NativeOperations() = default;
        virtual QString associate(bool associated) = 0;
        virtual QString center(QWindow *window, const QRect &windowRegion) = 0;
        virtual QString setHidden(bool hidden) = 0;
        virtual QString startMotion(QWindow *window, std::function<void(QPointF)> callback) = 0;
        virtual void stopMotion() = 0;
    };

    explicit MacPointerCapture(QObject *parent = nullptr);
    explicit MacPointerCapture(std::unique_ptr<NativeOperations> operations, QObject *parent = nullptr);
    ~MacPointerCapture() override;

    [[nodiscard]] static bool isSupported();
    [[nodiscard]] bool locked() const;
    [[nodiscard]] QString error() const;
    void setCapture(QWindow *window, bool enabled, const QRect &windowRegion);
    void release();

signals:
    void stateChanged();
    void relativeMotion(qint16 x, qint16 y);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void motion(const QPointF &delta);
    void fail(const QString &error);
    struct Private;
    std::unique_ptr<Private> d;
};
