#pragma once

#include <QList>
#include <QObject>
#include <QString>
#include <QVariantList>

class QQuickWindow;

class GraphicsDeviceSelection final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool selectorVisible READ selectorVisible CONSTANT)
    Q_PROPERTY(QVariantList choices READ choices NOTIFY choicesChanged)
    Q_PROPERTY(bool savedDeviceUnavailable READ savedDeviceUnavailable CONSTANT)

public:
    struct Adapter {
        QString id;
        QString name;
        quint64 luid = 0;
        quint64 dedicatedMemoryBytes = 0;
        bool software = false;
    };

    static QList<Adapter> detectAdapters();
    GraphicsDeviceSelection(QList<Adapter> adapters, QString requestedDeviceId,
                            QObject *parent = nullptr);

    bool selectorVisible() const { return m_adapters.size() >= 2; }
    QVariantList choices() const;
    QString requestedDeviceId() const { return m_requestedDeviceId; }
    QString activeDeviceId() const { return m_active.id; }
    bool savedDeviceUnavailable() const;
    quint64 adapterLuid() const { return m_active.luid; }
    bool applyTo(QQuickWindow *window) const;

signals:
    void choicesChanged();

private:
    QList<Adapter> m_adapters;
    QString m_requestedDeviceId;
    Adapter m_active;
};
