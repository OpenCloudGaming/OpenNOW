#pragma once

#include "input/GenericHidWire.h"

#include <QHash>
#include <QMutex>
#include <QSet>
#include <QThread>
#include <QVariantList>

#include <array>
#include <deque>
#include <memory>

class GenericHidBackend
{
public:
    struct Device {
        QByteArray path;
        QByteArray physicalKey;
        QByteArray instanceKey;
        QString serial;
        quint16 vendor = 0;
        quint16 product = 0;
        quint16 revision = 0;
        int interfaceNumber = -1;
        quint16 usagePage = 0;
        quint16 usage = 0;
    };
    using Handle = void *;

    virtual ~GenericHidBackend() = default;
    virtual bool initialize() = 0;
    virtual void shutdown() = 0;
    virtual QList<Device> enumerate() = 0;
    virtual Handle open(const Device &device) = 0;
    virtual bool accessDenied(const Device &device) = 0;
    virtual void close(Handle handle) = 0;
    virtual QByteArray descriptor(Handle handle) = 0;
    virtual int read(Handle handle, unsigned char *bytes, int capacity) = 0;
    virtual int output(Handle handle, quint8 operation, unsigned char *bytes, int length) = 0;
};

class GenericHidInput final : public QThread
{
    Q_OBJECT
public:
    static constexpr int maxSources = 8;
    static constexpr int maxReportBytes = 256;
    struct DetectedDevice {
        QString deviceId;
        QByteArray key;
        QString serial;
        quint16 vendor = 0;
        quint16 product = 0;
        quint8 slot = 0;
        quint64 incarnation = 0;
        bool noAccess = false;
        bool operator==(const DetectedDevice &) const = default;
    };
    struct Batch {
        bool inventoryChanged = false;
        QList<GenericHidDeviceClaim> claims;
        QList<GenericHidReport> reports;
        QStringList diagnostics;
        quint64 scanGeneration = 0;
    };

    explicit GenericHidInput(std::shared_ptr<GenericHidBackend> backend = {}, QObject *parent = nullptr);
    ~GenericHidInput() override;
    static bool allowed(quint16 vendor, quint16 product);
    static QVariantList supportedDevices();
    static QVariantMap deviceMetadata(quint16 vendor, quint16 product);
    static QByteArray groupKey(const GenericHidBackend::Device &device);
    static QByteArray physicalDeviceKey(quint16 vendor, quint16 product, const char *path);
    Batch drain(int limit = 32);
    QList<GenericHidDeviceClaim> claims() const;
    QList<DetectedDevice> devices() const;
    void setForwardingEnabled(bool enabled);
    void submitOutput(const GenericHidReport &report);
    void clearOutputs();
    void stop();

signals:
    void inventoryChanged();

protected:
    void run() override;

private:
    struct Lane {
        GenericHidDeviceClaim claim;
        std::deque<GenericHidReport> reports;
        int bytes = 0;
    };
    void removeLocked(quint8 slot, quint64 incarnation);
    void markInventoryChangedLocked();
    bool append(const GenericHidReport &report);
    void diagnose(const QString &message);

    std::shared_ptr<GenericHidBackend> m_backend;
    mutable QMutex m_mutex;
    std::array<Lane, maxSources> m_lanes;
    std::deque<GenericHidReport> m_outputs;
    int m_outputBytes = 0;
    quint64 m_outputEpoch = 0;
    int m_drainSlot = 0;
    bool m_inventoryChanged = false;
    QSet<quint64> m_failedSources;
    QStringList m_diagnostics;
    quint32 m_droppedDiagnostics = 0;
    QList<DetectedDevice> m_devices;
    quint64 m_scanGeneration = 0;
    bool m_forwardingEnabled = true;
    quint64 m_forwardingEpoch = 0;
};
