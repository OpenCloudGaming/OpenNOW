#pragma once

#include <QPointer>
#include <QQuickItem>

class HdrChromeEffect : public QQuickItem
{
    Q_OBJECT
    Q_PROPERTY(QQuickItem *source READ source WRITE setSource NOTIFY sourceChanged)

public:
    explicit HdrChromeEffect(QQuickItem *parent = nullptr);
    QQuickItem *source() const { return m_source; }
    void setSource(QQuickItem *source);

signals:
    void sourceChanged();

protected:
    QSGNode *updatePaintNode(QSGNode *node, UpdatePaintNodeData *) override;

private:
    QPointer<QQuickItem> m_source;
};
