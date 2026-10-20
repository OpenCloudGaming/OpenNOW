#pragma once

#include <QObject>

// Local patch: holds the privileged AWDL suppression lease while a stream is
// running (macOS only). AWDL makes the Wi-Fi radio time-slice onto its
// discovery channel, which shows up as periodic jitter spikes and
// packet-loss bursts during streaming; the shipped awdl-helper daemon
// (io.github.opencloudgaming.OpenNOW.awdl, /Library/PrivilegedHelperTools)
// suppresses the awdl0 interface for the lease duration. The daemon protocol
// is one byte over /var/run/opennow-awdl/control.sock — 'A' acquires, 'R'
// releases — and closing the connection releases the lease, so an app crash
// restores the interface automatically. Measured on Wi-Fi + VPN (matched
// 4-minute sessions): jitter average −54%, jitter max −70%, loss-recovery
// events −17%, with no bitrate regression.
class AwdlSuppressionLease final : public QObject
{
    Q_OBJECT

public:
    explicit AwdlSuppressionLease(QObject *parent = nullptr);
    ~AwdlSuppressionLease() override;

    // Acquires the lease when active and the helper daemon is reachable;
    // releases it when inactive. Failure to reach the daemon is ignored:
    // streaming continues normally and AWDL simply stays enabled.
    void setActive(bool active);
    bool isActive() const { return m_active; }

private:
    void acquire();
    void release();

    bool m_active = false;
#ifdef Q_OS_MACOS
    int m_descriptor = -1;
#endif
};
