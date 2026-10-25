#include "input/ControllerInput.h"
#include "input/SdlDeviceClaim.h"
#include "input/SonySnapshotWire.h"
#include "streaming/NativeStreamRuntime.h"

#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QSignalSpy>
#include <QStringList>
#include <QTest>
#include <QUdpSocket>

#include <cerrno>

#include <unistd.h>

#include <functional>
#include <memory>
#include <thread>

namespace {

constexpr int kChainTimeoutMs = 20'000;
constexpr quint16 kHidDeviceMaskRich = 0x5;
constexpr quint16 kHidDeviceMaskOrdinary = 0x0;

constexpr int kReportOffset = 12;
constexpr int kReportBytes = 64;
constexpr int kDeviceChangeBytes = 26;

struct ChainPorts {
    quint16 engine = 0;
    quint16 peer = 0;
};

quint16 reserveUdpPort()
{
    QUdpSocket probe;
    if (!probe.bind(QHostAddress::LocalHost, 0)) return 0;
    return probe.localPort();
}

QString localRouteIp()
{
    QUdpSocket probe;
    if (!probe.bind(QHostAddress::AnyIPv4, 0)) return {};
    probe.connectToHost(QHostAddress(QStringLiteral("192.0.2.1")), 9);
    const auto local = probe.localAddress();
    if (local.isNull() || local.isLoopback()) return {};
    return local.toString();
}

void ensureHeadlessAudio()
{
    qputenv("ALSA_CONFIG_PATH",
            QByteArray(OPENNOW_QT_SOURCE_DIR "/tests/fixtures/alsa-null.conf"));
    qputenv("OPENNOW_NVST_TRACE", "1");
}

class EngineLogCapture
{
public:
    ~EngineLogCapture() { stop(); }

    bool start()
    {
        if (m_started) return true;
        int pipeFds[2];
        if (::pipe(pipeFds) != 0) return false;
        const int saved = ::dup(STDERR_FILENO);
        if (saved < 0) {
            ::close(pipeFds[0]);
            ::close(pipeFds[1]);
            return false;
        }
        if (::dup2(pipeFds[1], STDERR_FILENO) < 0) {
            ::close(saved);
            ::close(pipeFds[0]);
            ::close(pipeFds[1]);
            return false;
        }
        ::close(pipeFds[1]);
        m_savedStderr = saved;
        m_readFd = pipeFds[0];
        m_started = true;
        m_reader = std::thread([this] { readLoop(); });
        return true;
    }

    int countContaining(const QString &needle) const
    {
        QMutexLocker locker(&m_mutex);
        int count = 0;
        for (const auto &line : m_lines) {
            if (line.contains(needle)) ++count;
        }
        return count;
    }

private:
    void stop()
    {
        if (!m_started) return;
        if (m_savedStderr >= 0) {
            ::dup2(m_savedStderr, STDERR_FILENO);
            ::close(m_savedStderr);
            m_savedStderr = -1;
        }
        if (m_reader.joinable()) m_reader.join();
        if (m_readFd >= 0) {
            ::close(m_readFd);
            m_readFd = -1;
        }
        m_started = false;
    }

    void readLoop()
    {
        QByteArray buffer;
        char chunk[4096];
        for (;;) {
            const auto received = ::read(m_readFd, chunk, sizeof(chunk));
            if (received < 0) {
                if (errno == EINTR) continue;
                break;
            }
            if (received == 0) break;
            buffer.append(chunk, int(received));
            if (buffer.size() > kMaxPartialBytes) buffer = buffer.right(kMaxPartialBytes);
            int newline = 0;
            while ((newline = buffer.indexOf('\n')) >= 0) {
                const auto line = buffer.left(newline);
                buffer.remove(0, newline + 1);
                QMutexLocker locker(&m_mutex);
                m_lines.append(QString::fromUtf8(line));
                while (m_lines.size() > kMaxLines) m_lines.removeFirst();
            }
        }
    }

    static constexpr int kMaxLines = 4'096;
    static constexpr int kMaxPartialBytes = 16 * 1'024;

    int m_savedStderr = -1;
    int m_readFd = -1;
    std::thread m_reader;
    mutable QMutex m_mutex;
    QList<QString> m_lines;
    bool m_started = false;
};

ChainPorts reserveChainPorts()
{
    const quint16 engine = reserveUdpPort();
    quint16 peer = reserveUdpPort();
    while (peer == engine || peer == 0) peer = reserveUdpPort();
    return {engine, peer};
}

bool waitUntil(const std::function<bool()> &predicate, int timeoutMs)
{
    QElapsedTimer clock;
    clock.start();
    while (!predicate() && clock.elapsed() < timeoutMs) QTest::qWait(5);
    return predicate();
}

class SonyChainHarness
{
public:
    ~SonyChainHarness()
    {
        m_runtime.reset();
        if (m_device) opennow_streamer_vulkan_device_destroy(m_device);
        stopPeer();
    }

    bool start(ChainPorts ports, quint16 hidDeviceMask)
    {
        const auto routeIp = localRouteIp();
        if (routeIp.isEmpty()) {
            m_failure = QStringLiteral("no routable local IPv4 address is available");
            return false;
        }
        ensureHeadlessAudio();
        if (!m_engineLog.start()) {
            m_failure = QStringLiteral("engine diagnostic capture did not start");
            return false;
        }
        if (!launchPeer(ports, routeIp)) {
            return false;
        }
        if (!startSession(ports, hidDeviceMask, routeIp)) return false;
        QObject::connect(m_runtime.get(), &NativeStreamRuntime::cursorUpdated, m_runtime.get(),
                         [this] { ++m_cursorUpdates; });
        return waitForPeerSession();
    }

    NativeStreamRuntime &runtime() { return *m_runtime; }

    bool waitForPeerSession(int timeoutMs = kChainTimeoutMs)
    {
        return waitUntil(
            [this] {
                return m_lines.contains(QStringLiteral("CONNECTED"))
                    && m_lines.contains(QStringLiteral("OPEN ChannelId(0) control_channel_reliable"));
            },
            timeoutMs);
    }

    bool sawAttach(int timeoutMs = kChainTimeoutMs)
    {
        return waitUntil(
            [this] {
                for (const auto &payload : m_payloads) {
                    if (payload.size() == kDeviceChangeBytes && payload.at(13) == char(0x01)) return true;
                }
                return false;
            },
            timeoutMs);
    }

    bool sawRemoval(int timeoutMs = kChainTimeoutMs)
    {
        return waitUntil(
            [this] {
                for (const auto &payload : m_payloads) {
                    if (payload.size() == kDeviceChangeBytes && payload.at(13) == char(0x03)) return true;
                }
                return false;
            },
            timeoutMs);
    }

    bool sawAttachFrom(int firstIndex, int timeoutMs)
    {
        return waitUntil(
            [this, firstIndex] {
                for (int index = firstIndex; index < m_payloads.size(); ++index) {
                    const auto &payload = m_payloads.at(index);
                    if (payload.size() == kDeviceChangeBytes && payload.at(13) == char(0x01)) {
                        return true;
                    }
                }
                return false;
            },
            timeoutMs);
    }

    int firstAttachLowIdAfter(int firstIndex) const
    {
        for (int index = firstIndex; index < m_payloads.size(); ++index) {
            const auto &payload = m_payloads.at(index);
            if (payload.size() == kDeviceChangeBytes && payload.at(13) == char(0x01)) {
                return quint8(payload.at(12));
            }
        }
        return -1;
    }

    bool sawOrdinaryGamepadMatchingAfter(
        int firstIndex, const std::function<bool(const QByteArray &)> &accept, int timeoutMs)
    {
        return waitUntil(
            [this, firstIndex, &accept] {
                for (int index = firstIndex; index < m_payloads.size(); ++index) {
                    const auto &payload = m_payloads.at(index);
                    if (payload.size() == 4 + 54 && payload.at(4) == char(0x23)
                        && payload.at(13) == char(0x26) && payload.at(17) == char(0x21)) {
                        if (accept(payload.mid(20, 38))) return true;
                    }
                }
                return false;
            },
            timeoutMs);
    }

    bool sawSonyReportFrom(int firstIndex, int timeoutMs)
    {
        return waitUntil(
            [this, firstIndex] {
                for (int index = firstIndex; index < m_payloads.size(); ++index) {
                    if (m_payloads.at(index).size() == kReportOffset + kReportBytes) return true;
                }
                return false;
            },
            timeoutMs);
    }

    bool sawSonyReportMatching(const std::function<bool(const QByteArray &)> &accept,
                               int timeoutMs = kChainTimeoutMs, int firstIndex = 0)
    {
        return waitUntil(
            [this, &accept, firstIndex] {
                for (int index = firstIndex; index < m_payloads.size(); ++index) {
                    const auto &payload = m_payloads.at(index);
                    if (payload.size() != kReportOffset + kReportBytes) continue;
                    if (accept(payload.mid(kReportOffset))) return true;
                }
                return false;
            },
            timeoutMs);
    }

    QByteArray lastSonyReport()
    {
        for (auto it = m_payloads.crbegin(); it != m_payloads.crend(); ++it) {
            if (it->size() == kReportOffset + kReportBytes) return it->mid(kReportOffset);
        }
        return {};
    }

    QByteArray firstOrdinaryGamepadReport()
    {
        for (const auto &payload : m_payloads) {
            if (payload.size() == 4 + 54 && payload.at(4) == char(0x23)
                && payload.at(13) == char(0x26) && payload.at(17) == char(0x21)) {
                return payload.mid(20, 38);
            }
        }
        return {};
    }

    const QList<QByteArray> &payloads() const { return m_payloads; }
    const QStringList &lines() const { return m_lines; }
    int payloadCount() const { return m_payloads.size(); }

    static QString rumbleCommandHex(quint8 lowId, quint8 low, quint8 high)
    {
        QByteArray command;
        command.append(char(0x06)).append(char(0x02));
        command.append(char(0x0d)).append(char(0x00));
        command.append("\x11\x00\x00\x00", 4);
        command.append(char(lowId)).append(char(4)).append(char(0));
        command.append(char(5)).append(char(1)).append(char(0)).append(char(0));
        command.append(char(low)).append(char(high));
        return QString::fromLatin1(command.toHex());
    }

    int engineReceiptCount(const QString &hex) const
    {
        return m_engineLog.countContaining(QStringLiteral("raw=%1").arg(hex));
    }

    bool sendPeerRumble(quint8 lowId, quint8 low, quint8 high)
    {
        if (m_peer.state() != QProcess::Running) return false;
        const auto hex = rumbleCommandHex(lowId, low, high);
        const int receiptsBefore = engineReceiptCount(hex);
        const auto line = QStringLiteral("RUMBLE %1 %2 %3\n").arg(lowId).arg(low).arg(high);
        m_peer.write(line.toUtf8());
        if (!m_peer.waitForBytesWritten(1'000)) return false;
        const auto expected =
            QStringLiteral("SENT RUMBLE low_id=%1 low=%2 high=%3")
                .arg(lowId)
                .arg(quint16(low) << 8)
                .arg(quint16(high) << 8);
        if (!waitUntil(
                [this, expected] {
                    for (const auto &entry : m_lines) {
                        if (entry.startsWith(expected)
                            && entry.endsWith(QStringLiteral("sent=true"))) {
                            return true;
                        }
                    }
                    return false;
                },
                5'000)) {
            return false;
        }
        if (!waitUntil([this, hex, receiptsBefore] {
                return engineReceiptCount(hex) > receiptsBefore;
            }, kChainTimeoutMs)) {
            return false;
        }
        return waitForHapticDrainBarrier();
    }

    bool waitForHapticDrainBarrier()
    {
        if (m_peer.state() != QProcess::Running) return false;
        const int before = m_cursorUpdates;
        m_peer.write("CURSOR 1\nCURSOR 2\n");
        if (!m_peer.waitForBytesWritten(1'000)) return false;
        return waitUntil([this, before] { return m_cursorUpdates >= before + 2; }, 5'000);
    }

    bool claimsPrecedeSonyData() const
    {
        int attachIndex = -1;
        int reportIndex = -1;
        for (int index = 0; index < m_payloads.size(); ++index) {
            const auto &payload = m_payloads.at(index);
            if (attachIndex < 0 && payload.size() == kDeviceChangeBytes
                && payload.at(13) == char(0x01)) {
                attachIndex = index;
            }
            if (reportIndex < 0 && payload.size() == kReportOffset + kReportBytes) {
                reportIndex = index;
            }
        }
        return attachIndex >= 0 && reportIndex > attachIndex;
    }

    bool sawPayloadAfter(int firstIndex, int timeoutMs) const
    {
        return waitUntil([this, firstIndex] { return m_payloads.size() > firstIndex; }, timeoutMs);
    }

    bool hasRichReportAfter(int firstIndex) const
    {
        for (int index = firstIndex; index < m_payloads.size(); ++index) {
            if (m_payloads.at(index).size() == kReportOffset + kReportBytes) return true;
        }
        return false;
    }

    bool hasSonyReportAfter(int firstIndex,
                            const std::function<bool(const QByteArray &)> &accept) const
    {
        for (int index = firstIndex; index < m_payloads.size(); ++index) {
            const auto &payload = m_payloads.at(index);
            if (payload.size() != kReportOffset + kReportBytes) continue;
            if (accept(payload.mid(kReportOffset))) return true;
        }
        return false;
    }

    bool sawOrdinaryGamepadAfter(int firstIndex, int timeoutMs) const
    {
        return waitUntil(
            [this, firstIndex] {
                for (int index = firstIndex; index < m_payloads.size(); ++index) {
                    const auto &payload = m_payloads.at(index);
                    if (payload.size() == 4 + 54 && payload.at(4) == char(0x23)
                        && payload.at(13) == char(0x26) && payload.at(17) == char(0x21)) {
                        return true;
                    }
                }
                return false;
            },
            timeoutMs);
    }

    QByteArray lastSonyReportFrame() const
    {
        for (auto it = m_payloads.crbegin(); it != m_payloads.crend(); ++it) {
            if (it->size() == kReportOffset + kReportBytes) return *it;
        }
        return {};
    }
    QString diagnostics() const
    {
        return QStringLiteral("peer lines=[%1] payloads=%2 stderr=[%3] failure=[%4]")
            .arg(m_lines.join(QLatin1Char('|')))
            .arg(m_payloads.size())
            .arg(m_stderr)
            .arg(m_failure);
    }

private:
    bool launchPeer(ChainPorts ports, const QString &physicalIp)
    {
        const auto candidate = QDir(QCoreApplication::applicationDirPath())
                                   .filePath(QStringLiteral("nvst-peer-probe"));
        if (!QFileInfo::exists(candidate)) {
            m_failure = QStringLiteral("nvst-peer-probe is not deployed next to the test binary");
            return false;
        }
        const QStringList arguments{
            QStringLiteral("--bind-ip"), physicalIp,
            QStringLiteral("--remote-ip"), physicalIp,
            QStringLiteral("--port"), QString::number(ports.peer),
            QStringLiteral("--remote-port"), QString::number(ports.engine),
            QStringLiteral("--local-ufrag"), QStringLiteral("peer-ufrag-01"),
            QStringLiteral("--local-password"), QStringLiteral("peer-password-value-0000000001"),
            QStringLiteral("--remote-ufrag"), QStringLiteral("engn-ufrag-01"),
            QStringLiteral("--remote-password"), QStringLiteral("engine-password-value-00000001"),
            QStringLiteral("--timeout-ms"), QString::number(kChainTimeoutMs + 15'000)};
        m_peer.setProcessChannelMode(QProcess::SeparateChannels);
        QObject::connect(&m_peer, &QProcess::readyReadStandardOutput, &m_peer, [this] {
            while (m_peer.canReadLine()) {
                const auto line = QString::fromUtf8(m_peer.readLine()).trimmed();
                if (line.isEmpty()) continue;
                m_lines.append(line);
                if (line.startsWith(QStringLiteral("DATA "))) {
                    m_payloads.append(QByteArray::fromHex(line.section(QLatin1Char(' '), 2).toLatin1()));
                }
            }
        });
        QObject::connect(&m_peer, &QProcess::readyReadStandardError, &m_peer, [this] {
            m_stderr.append(QString::fromUtf8(m_peer.readAllStandardError()));
        });
        m_peer.start(candidate, arguments);
        if (!m_peer.waitForStarted(5'000)) {
            m_failure = QStringLiteral("the peer process did not start: %1").arg(m_peer.errorString());
            return false;
        }
        if (!waitUntil([this] { return !readyFingerprint().isEmpty(); }, 5'000)) {
            m_failure = QStringLiteral("the peer never reported READY");
            return false;
        }
        return true;
    }

    bool startSession(ChainPorts ports, quint16 hidDeviceMask, const QString &routeIp)
    {
        if (opennow_streamer_vulkan_device_create(&m_device) != OPENNOW_STREAMER_OK) {
            m_device = nullptr;
        }
        m_runtime = std::make_unique<NativeStreamRuntime>(nullptr, m_device);
        if (!m_runtime->start()) {
            m_failure = QStringLiteral("the embedded runtime did not start: %1")
                            .arg(m_runtime->lastError());
            return false;
        }
        const QJsonObject session{
            {QStringLiteral("sessionId"), QStringLiteral("sony-chain-session")},
            {QStringLiteral("serverIp"), QStringLiteral("127-0-0-1.synthetic.invalid")},
            {QStringLiteral("iceServers"), QJsonArray{}},
            {QStringLiteral("mediaConnectionInfo"),
             QJsonObject{{QStringLiteral("ip"), QStringLiteral("127-0-0-1.media.synthetic.invalid")},
                         {QStringLiteral("port"), 18'784},
                         {QStringLiteral("usage"), 17}}}};
        const QJsonObject context{
            {QStringLiteral("session"), session},
            {QStringLiteral("settings"),
             QJsonObject{{QStringLiteral("codec"), QStringLiteral("H264")},
                         {QStringLiteral("fps"), 60},
                         {QStringLiteral("nativeVideoBackend"), QStringLiteral("software")}}},
            {QStringLiteral("shortcuts"),
             QJsonObject{{QStringLiteral("stopStream"), QStringLiteral("Ctrl+Shift+Q")}}},
            {QStringLiteral("nvstVideo"),
             QJsonObject{
                 {QStringLiteral("clientUdpPort"), int(ports.engine)},
                 {QStringLiteral("videoPeerIp"), routeIp},
                 {QStringLiteral("videoPeerPort"), int(ports.peer)},
                 {QStringLiteral("srtpAesKeyHex"),
                  QStringLiteral("000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F")},
                 {QStringLiteral("srtpSaltHex"), QStringLiteral("00000000000000009ECA935E")},
                 {QStringLiteral("codec"), QStringLiteral("H264")},
                 {QStringLiteral("pingPayload"), QStringLiteral("sony-chain")},
                 {QStringLiteral("hidDeviceMask"), int(hidDeviceMask)},
                 {QStringLiteral("remoteDtlsFingerprint"), readyFingerprint()},
                 {QStringLiteral("localIceUsernameFragment"), QStringLiteral("engn-ufrag-01")},
                 {QStringLiteral("localIcePassword"),
                  QStringLiteral("engine-password-value-00000001")},
                 {QStringLiteral("remoteIceUsernameFragment"), QStringLiteral("peer-ufrag-01")},
                 {QStringLiteral("remoteIcePassword"),
                  QStringLiteral("peer-password-value-0000000001")}}}};
        QSignalSpy responses(m_runtime.get(), &NativeStreamRuntime::responseReceived);
        const QJsonObject command{{QStringLiteral("type"), QStringLiteral("start")},
                                  {QStringLiteral("id"), QStringLiteral("sony-chain-start")},
                                  {QStringLiteral("context"), context}};
        if (!m_runtime->send(command)) {
            m_failure = QStringLiteral("the start command was rejected: %1")
                            .arg(m_runtime->lastError());
            return false;
        }
        const auto accepted = [&] {
            for (const auto &record : responses) {
                const auto response = record.at(0).toJsonObject();
                if (response.value(QStringLiteral("id")).toString()
                        == QStringLiteral("sony-chain-start")
                    && response.value(QStringLiteral("type")).toString() == QStringLiteral("ok")) {
                    return true;
                }
            }
            return false;
        };
        if (!waitUntil(accepted, kChainTimeoutMs)) {
            const auto last = responses.isEmpty()
                                  ? QJsonObject{}
                                  : responses.last().at(0).toJsonObject();
            m_failure = QStringLiteral("the session was never accepted: %1")
                            .arg(QString::fromUtf8(
                                QJsonDocument(last).toJson(QJsonDocument::Compact)));
            return false;
        }
        return true;
    }

    void stopPeer()
    {
        if (m_peer.state() == QProcess::NotRunning) return;
        m_peer.closeWriteChannel();
        if (!m_peer.waitForFinished(1'000)) {
            m_peer.kill();
            m_peer.waitForFinished(1'000);
        }
    }

    QString readyFingerprint() const
    {
        for (const auto &line : m_lines) {
            if (line.startsWith(QStringLiteral("READY "))) return line.section(QLatin1Char(' '), 1);
        }
        return {};
    }

    QProcess m_peer;
    EngineLogCapture m_engineLog;
    int m_cursorUpdates = 0;
    OpenNowStreamerVulkanDevice *m_device = nullptr;
    std::unique_ptr<NativeStreamRuntime> m_runtime;
    QStringList m_lines;
    QList<QByteArray> m_payloads;
    QString m_stderr;
    QString m_failure;
};

struct VirtualSonyPad {
    SDL_JoystickID id = 0;
    SDL_Joystick *joystick = nullptr;
    QList<QPair<quint16, quint16>> rumbles;

    static bool SDLCALL onRumble(void *userdata, Uint16 low, Uint16 high)
    {
        auto *pad = static_cast<VirtualSonyPad *>(userdata);
        pad->rumbles.append({low, high});
        return true;
    }

    bool attach(quint16 product, const char *name)
    {
        SDL_VirtualJoystickTouchpadDesc touchpad{2, 0, 0, 0};
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = product;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.ntouchpads = 1;
        descriptor.touchpads = &touchpad;
        descriptor.button_mask = (1u << SDL_GAMEPAD_BUTTON_TOUCHPAD)
            | (1u << SDL_GAMEPAD_BUTTON_SOUTH) | (1u << SDL_GAMEPAD_BUTTON_GUIDE);
        descriptor.name = name;
        descriptor.userdata = this;
        descriptor.Rumble = &VirtualSonyPad::onRumble;
        id = SDL_AttachVirtualJoystick(&descriptor);
        return id != 0;
    }

    bool resolveJoystick()
    {
        joystick = SDL_GetJoystickFromID(id);
        return joystick != nullptr;
    }

    void detach()
    {
        if (id == 0) return;
        SDL_DetachVirtualJoystick(id);
        id = 0;
        joystick = nullptr;
    }
};

struct VirtualSonyController {
    ControllerInput input;
    VirtualSonyPad pad;

    bool attach(quint16 product, const char *name)
    {
        if (!pad.attach(product, name)) return false;
        if (!waitUntil([this] { return input.controllerCount() == 1; }, 5'000)) return false;
        return pad.resolveJoystick();
    }

    void detach()
    {
        pad.detach();
        waitUntil([this] { return input.controllerCount() == 0; }, 5'000);
    }
};

void wireProductionInput(ControllerInput &input, NativeStreamRuntime &runtime)
{
    QObject::connect(&input, &ControllerInput::deviceClaimsChanged, &runtime, [&input, &runtime] {
        runtime.replaceSdlDeviceClaims(input.deviceClaims());
    });
    QObject::connect(&input, &ControllerInput::sonySnapshot, &runtime,
                     [&runtime](const ControllerInput::SonySnapshot &snapshot) {
                         runtime.submitSonySnapshot(openNowWireSonySnapshot(snapshot));
                     });
    QObject::connect(&input, &ControllerInput::gamepadSnapshot, &runtime,
                     [&runtime](quint8 controllerId, quint16 bitmap, quint16 buttons,
                                quint8 leftTrigger, quint8 rightTrigger, qint16 leftStickX,
                                qint16 leftStickY, qint16 rightStickX, qint16 rightStickY) {
                         runtime.submitGamepad(controllerId, bitmap, buttons, leftTrigger,
                                               rightTrigger, leftStickX, leftStickY, rightStickX,
                                               rightStickY);
                     });
    QObject::connect(&input, &ControllerInput::localActionRequested, &runtime,
                     [&runtime](quint32 action) { runtime.submitLocalAction(action); });
    QObject::connect(&runtime, &NativeStreamRuntime::controllerRumbleRequested, &input,
                     &ControllerInput::playRumble);
    QCOMPARE(runtime.replaceSdlDeviceClaims(input.deviceClaims()), OPENNOW_STREAMER_OK);
}

bool setCaptureActive(ControllerInput &input, NativeStreamRuntime &runtime, bool active)
{
    input.setShellCaptureEnabled(!active);
    bool rawInputActive = false;
    return runtime.setCaptureActive(active, false, 0, &rawInputActive) == OPENNOW_STREAMER_OK;
}

bool pressSonyButton(SDL_Joystick *joystick, SDL_GamepadButton button, bool down)
{
    if (!SDL_SetJoystickVirtualButton(joystick, button, down)) return false;
    SDL_UpdateJoysticks();
    return true;
}

bool touchSonyPad(SDL_Joystick *joystick, int finger, bool down, float x, float y)
{
    if (!SDL_SetJoystickVirtualTouchpad(joystick, 0, finger, down, x, y, down ? 1.0f : 0.0f)) {
        return false;
    }
    SDL_UpdateJoysticks();
    return true;
}

bool moveSonyStick(SDL_Joystick *joystick, SDL_GamepadAxis axis, Sint16 value)
{
    if (!SDL_SetJoystickVirtualAxis(joystick, axis, value)) return false;
    SDL_UpdateJoysticks();
    return true;
}

bool clickSonyPad(SDL_JoystickID id, bool down)
{
    SDL_Event click{};
    click.type = down ? SDL_EVENT_GAMEPAD_BUTTON_DOWN : SDL_EVENT_GAMEPAD_BUTTON_UP;
    click.gbutton.which = id;
    click.gbutton.button = SDL_GAMEPAD_BUTTON_TOUCHPAD;
    click.gbutton.down = down;
    if (!SDL_PushEvent(&click)) return false;
    return true;
}

} // namespace

class SonyChainTest : public QObject
{
    Q_OBJECT

private slots:
    void assignedBackgroundPadReachesThePeerWithoutFocusedCapture()
    {
        VirtualSonyController sony;
        QVERIFY2(sony.attach(0x05c4, "OpenNOW background DS4"), SDL_GetError());
        sony.input.setInputControllerId(sony.pad.id);
        sony.input.setBackgroundControllerEnabled(true);
        SonyChainHarness chain;
        QVERIFY2(chain.start(reserveChainPorts(), kHidDeviceMaskRich), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(sony.input, chain.runtime());
        chain.runtime().setControllerInputPolicy(NativeStreamRuntime::ControllerInputPolicy::BackgroundGameplay);
        sony.input.setShellCaptureEnabled(false);
        bool raw = true;
        QCOMPARE(chain.runtime().setCaptureActive(false, false, 0, &raw), OPENNOW_STREAMER_OK);
        QVERIFY(!raw);
        QVERIFY2(chain.sawAttach(), qUtf8Printable(chain.diagnostics()));
        const auto beforePress = chain.payloadCount();
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportMatching([](const QByteArray &report) {
            return (quint8(report.at(5)) & 0x20) != 0;
        }, kChainTimeoutMs, beforePress), qUtf8Printable(chain.diagnostics()));

        const auto beforeSuspension = chain.payloadCount();
        sony.input.setInputSuspended(true);
        chain.runtime().setControllerInputPolicy(NativeStreamRuntime::ControllerInputPolicy::Suspended);
        QVERIFY2(chain.sawSonyReportMatching([](const QByteArray &report) {
            return (quint8(report.at(5)) & 0x20) == 0;
        }, kChainTimeoutMs, beforeSuspension), qUtf8Printable(chain.diagnostics()));
        const auto whileSuspended = chain.payloadCount();
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY(!chain.sawSonyReportMatching([](const QByteArray &report) {
            return (quint8(report.at(5)) & 0x20) != 0;
        }, 300, whileSuspended));

        const auto beforeResume = chain.payloadCount();
        chain.runtime().setControllerInputPolicy(NativeStreamRuntime::ControllerInputPolicy::BackgroundGameplay);
        sony.input.setInputSuspended(false);
        QVERIFY2(chain.sawSonyReportMatching([](const QByteArray &report) {
            return (quint8(report.at(5)) & 0x20) != 0;
        }, kChainTimeoutMs, beforeResume), qUtf8Printable(chain.diagnostics()));
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
    }

    void virtualSonyControllerReachesTheBoundPeerWithButtonsTouchAndBothYSigns()
    {
        VirtualSonyController sony;
        QVERIFY2(sony.attach(0x05c4, "OpenNOW integration DS4"), SDL_GetError());
        SonyChainHarness chain;
        const auto ports = reserveChainPorts();
        QVERIFY2(chain.start(ports, kHidDeviceMaskRich), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(sony.input, chain.runtime());
        QVERIFY(setCaptureActive(sony.input, chain.runtime(), true));
        QVERIFY2(chain.sawAttach(), qUtf8Printable(chain.diagnostics()));

        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) != 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));
        const auto pressed = chain.lastSonyReport();
        QCOMPARE(quint8(pressed.at(0)), quint8(0x01));
        QCOMPARE(quint8(pressed.at(7)) & 0x02u, 0u);
        QCOMPARE(quint8(pressed.at(35)) & 0x80u, 0x80u);

        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) == 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));

        QVERIFY(touchSonyPad(sony.pad.joystick, 0, true, 0.25f, 0.75f));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(35)) & 0x80u) == 0 && report.at(35) != char(0);
                     }),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(clickSonyPad(sony.pad.id, true));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(7)) & 0x02u) != 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(clickSonyPad(sony.pad.id, false));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(7)) & 0x02u) == 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(touchSonyPad(sony.pad.joystick, 0, false, 0.25f, 0.75f));

        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) != 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));

        const auto beforeStick = chain.payloads().size();
        QVERIFY(moveSonyStick(sony.pad.joystick, SDL_GAMEPAD_AXIS_LEFTY, SDL_JOYSTICK_AXIS_MIN));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return quint8(report.at(2)) == 0u;
                     },
                     kChainTimeoutMs, beforeStick),
                 qUtf8Printable(chain.diagnostics()));
        const auto beforeStickDown = chain.payloads().size();
        QVERIFY(moveSonyStick(sony.pad.joystick, SDL_GAMEPAD_AXIS_LEFTY, SDL_JOYSTICK_AXIS_MAX));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return quint8(report.at(2)) == 255u;
                     },
                     kChainTimeoutMs, beforeStickDown),
                 qUtf8Printable(chain.diagnostics()));
        const auto beforeStickCenter = chain.payloads().size();
        QVERIFY(moveSonyStick(sony.pad.joystick, SDL_GAMEPAD_AXIS_LEFTY, 0));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return quint8(report.at(2)) == 128u;
                     },
                     kChainTimeoutMs, beforeStickCenter),
                 qUtf8Printable(chain.diagnostics()));
    }

    void negotiatedOrdinaryFallbackReachesThePeerAsAnOrdinaryGamepad()
    {
        VirtualSonyController sony;
        QVERIFY2(sony.attach(0x09cc, "OpenNOW integration DS4 ordinary"), SDL_GetError());
        SonyChainHarness chain;
        const auto ports = reserveChainPorts();
        QVERIFY2(chain.start(ports, kHidDeviceMaskOrdinary), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(sony.input, chain.runtime());
        QVERIFY(setCaptureActive(sony.input, chain.runtime(), true));
        QVERIFY(!chain.sawAttach(1'500));

        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(waitUntil([&chain] { return !chain.firstOrdinaryGamepadReport().isEmpty(); },
                           kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        const auto gamepad = chain.firstOrdinaryGamepadReport();
        QCOMPARE(gamepad.size(), 38);
        QCOMPARE(quint8(gamepad.at(0)), quint8(0x0c));
        QCOMPARE(quint16(gamepad.at(6)) | (quint16(gamepad.at(7)) << 8), quint16(0));
        QCOMPARE(quint16(gamepad.at(8)) | (quint16(gamepad.at(9)) << 8), quint16(0x0101));
        for (const auto &payload : chain.payloads()) {
            QVERIFY(payload.size() != kReportOffset + kReportBytes);
        }
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));

        const auto beforeFallbackStick = chain.payloadCount();
        QVERIFY(moveSonyStick(sony.pad.joystick, SDL_GAMEPAD_AXIS_LEFTY, SDL_JOYSTICK_AXIS_MAX));
        QVERIFY2(chain.sawOrdinaryGamepadMatchingAfter(
                     beforeFallbackStick,
                     [](const QByteArray &gamepad) {
                         const auto leftX = qint16(quint8(gamepad.at(16))
                                                   | (quint16(quint8(gamepad.at(17))) << 8));
                         const auto leftY = qint16(quint8(gamepad.at(18))
                                                   | (quint16(quint8(gamepad.at(19))) << 8));
                         return leftX == 0 && (leftY == -32767 || leftY == -32768);
                     },
                     kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(moveSonyStick(sony.pad.joystick, SDL_GAMEPAD_AXIS_LEFTY, 0));
    }

    void capturePauseAndResumeDeliversUndrainedNeutralRelease()
    {
        VirtualSonyController sony;
        QVERIFY2(sony.attach(0x0ce6, "OpenNOW integration DualSense"), SDL_GetError());
        SonyChainHarness chain;
        const auto ports = reserveChainPorts();
        QVERIFY2(chain.start(ports, kHidDeviceMaskRich), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(sony.input, chain.runtime());
        QVERIFY(setCaptureActive(sony.input, chain.runtime(), true));
        QVERIFY2(chain.sawAttach(), qUtf8Printable(chain.diagnostics()));

        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) != 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));

        const auto before = chain.payloads().size();
        for (int cycle = 0; cycle < 3; ++cycle) {
            QVERIFY(setCaptureActive(sony.input, chain.runtime(), false));
            QVERIFY2(waitUntil(
                         [&chain, before] {
                             int neutrals = 0;
                             for (int index = before; index < chain.payloads().size(); ++index) {
                                 const auto payload = chain.payloads().at(index);
                                 if (payload.size() != kReportOffset + kReportBytes) continue;
                                 const auto report = payload.mid(kReportOffset);
                                 if ((quint8(report.at(5)) & 0x20) == 0
                                     && (quint8(report.at(7)) & 0x02) == 0) {
                                     ++neutrals;
                                 }
                             }
                             return neutrals >= 1;
                         },
                         kChainTimeoutMs),
                     qUtf8Printable(chain.diagnostics()));
            QVERIFY(setCaptureActive(sony.input, chain.runtime(), true));
            QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
            QVERIFY2(chain.sawSonyReportMatching(
                         [](const QByteArray &report) {
                             return (quint8(report.at(5)) & 0x20) != 0;
                         }),
                     qUtf8Printable(chain.diagnostics()));
            QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        }
    }

    void detachedSourceIsRetiredBeforeItsReplacementIsAdmitted()
    {
        VirtualSonyController sony;
        QVERIFY2(sony.attach(0x0ba0, "OpenNOW integration DS4 detach"), SDL_GetError());
        SonyChainHarness chain;
        const auto ports = reserveChainPorts();
        QVERIFY2(chain.start(ports, kHidDeviceMaskRich), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(sony.input, chain.runtime());
        QVERIFY(setCaptureActive(sony.input, chain.runtime(), true));
        QVERIFY2(chain.sawAttach(), qUtf8Printable(chain.diagnostics()));

        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) != 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));

        sony.detach();
        QVERIFY2(chain.sawRemoval(), qUtf8Printable(chain.diagnostics()));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) == 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));

        const auto beforeReplacement = chain.payloadCount();
        QVERIFY(sony.attach(0x0ba0, "OpenNOW integration DS4 replacement"));
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawOrdinaryGamepadAfter(beforeReplacement, kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(!chain.hasRichReportAfter(beforeReplacement));
        QVERIFY(!chain.sawAttachFrom(beforeReplacement, 1'500));
        QVERIFY(pressSonyButton(sony.pad.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
    }

    void selectedSecondSourceOwnsLogicalSlotZero()
    {
        VirtualSonyPad first;
        QVERIFY2(first.attach(0x05c4, "OpenNOW integration DS4 first"), SDL_GetError());
        VirtualSonyPad second;
        QVERIFY2(second.attach(0x0ce6, "OpenNOW integration DualSense second"), SDL_GetError());
        ControllerInput input;
        QVERIFY2(waitUntil([&input] { return input.controllerCount() == 2; }, 5'000),
                 SDL_GetError());
        QVERIFY(first.resolveJoystick());
        QVERIFY(second.resolveJoystick());
        input.setInputControllerId(second.id);
        QCOMPARE(input.inputControllerId(), second.id);

        SonyChainHarness chain;
        const auto ports = reserveChainPorts();
        QVERIFY2(chain.start(ports, kHidDeviceMaskRich), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(input, chain.runtime());
        QVERIFY(setCaptureActive(input, chain.runtime(), true));
        QVERIFY2(chain.sawAttach(), qUtf8Printable(chain.diagnostics()));

        auto claims = input.deviceClaims();
        QCOMPARE(claims.size(), 1);
        QCOMPARE(int(claims.first().slot), 0);
        QCOMPARE(claims.first().vendor, quint16(0x054c));
        QCOMPARE(claims.first().product, quint16(0x0ce6));
        QVERIFY(claims.first().incarnation != 0);

        QVERIFY(pressSonyButton(second.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) != 0;
                     }),
                 qUtf8Printable(chain.diagnostics()));
        QCOMPARE(quint8(chain.lastSonyReportFrame().at(9)), quint8(6));
        QCOMPARE(chain.firstAttachLowIdAfter(0), int(chain.lastSonyReportFrame().at(9)));
        QVERIFY(chain.claimsPrecedeSonyData());
        QVERIFY(pressSonyButton(second.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));

        const auto beforeUnselected = chain.payloadCount();
        QVERIFY(pressSonyButton(first.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawPayloadAfter(beforeUnselected, kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(!chain.hasSonyReportAfter(beforeUnselected, [](const QByteArray &report) {
            return (quint8(report.at(5)) & 0x20) != 0;
        }));
        QVERIFY(pressSonyButton(first.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));

        const auto beforeReselected = chain.payloadCount();
        QVERIFY(pressSonyButton(second.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawSonyReportFrom(beforeReselected, kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        const auto beforeRelease = chain.payloadCount();
        QVERIFY(pressSonyButton(second.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        QVERIFY2(chain.sawSonyReportMatching(
                     [](const QByteArray &report) {
                         return (quint8(report.at(5)) & 0x20) == 0;
                     },
                     kChainTimeoutMs, beforeRelease),
                 qUtf8Printable(chain.diagnostics()));

        const auto beforeSwitch = chain.payloadCount();
        input.setInputControllerId(first.id);
        QCOMPARE(input.inputControllerId(), first.id);
        const auto switched = input.deviceClaims();
        QCOMPARE(switched.size(), 1);
        QCOMPARE(int(switched.first().slot), 0);
        QCOMPARE(switched.first().product, quint16(0x05c4));
        QVERIFY(pressSonyButton(first.joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY2(chain.sawOrdinaryGamepadAfter(beforeSwitch, kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(!chain.sawAttachFrom(beforeSwitch, 1'500));
        QVERIFY(!chain.hasSonyReportAfter(beforeSwitch, [](const QByteArray &report) {
            return (quint8(report.at(5)) & 0x20) != 0;
        }));
        QVERIFY(pressSonyButton(first.joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
    }

    void peerRumbleReachesTheSelectedSourceThroughNativeAdmission()
    {
        VirtualSonyPad first;
        QVERIFY2(first.attach(0x05c4, "OpenNOW integration DS4 rumble first"), SDL_GetError());
        VirtualSonyPad second;
        QVERIFY2(second.attach(0x0ce6, "OpenNOW integration DualSense rumble second"),
                 SDL_GetError());
        ControllerInput input;
        QVERIFY2(waitUntil([&input] { return input.controllerCount() == 2; }, 5'000),
                 SDL_GetError());
        QVERIFY(first.resolveJoystick());
        QVERIFY(second.resolveJoystick());
        input.setInputControllerId(second.id);

        SonyChainHarness chain;
        const auto ports = reserveChainPorts();
        QVERIFY2(chain.start(ports, kHidDeviceMaskRich), qUtf8Printable(chain.diagnostics()));
        wireProductionInput(input, chain.runtime());
        QVERIFY(setCaptureActive(input, chain.runtime(), true));
        QVERIFY2(chain.sawAttach(), qUtf8Printable(chain.diagnostics()));

        QVERIFY(chain.sendPeerRumble(6, 0x40, 0x80));
        QVERIFY2(waitUntil([&second] { return second.rumbles.contains({0x4000, 0x8000}); },
                           kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));
        QVERIFY(first.rumbles.isEmpty());

        QVERIFY(setCaptureActive(input, chain.runtime(), false));
        const auto beforeInactive = second.rumbles.size();
        QVERIFY(chain.sendPeerRumble(6, 0x11, 0x22));
        QCOMPARE(second.rumbles.size(), beforeInactive);
        QVERIFY(!second.rumbles.contains({0x1100, 0x2200}));

        QVERIFY(setCaptureActive(input, chain.runtime(), true));
        QVERIFY(chain.sendPeerRumble(6, 0x33, 0x44));
        QVERIFY2(waitUntil([&second] { return second.rumbles.contains({0x3300, 0x4400}); },
                           kChainTimeoutMs),
                 qUtf8Printable(chain.diagnostics()));

        const auto beforeRetired = second.rumbles.size();
        second.detach();
        QVERIFY2(chain.sawRemoval(), qUtf8Printable(chain.diagnostics()));
        QVERIFY(chain.sendPeerRumble(6, 0x55, 0x66));
        QCOMPARE(second.rumbles.size(), beforeRetired);
        QVERIFY(!second.rumbles.contains({0x5500, 0x6600}));
    }
};

QTEST_MAIN(SonyChainTest)
#include "tst_sonychain.moc"
