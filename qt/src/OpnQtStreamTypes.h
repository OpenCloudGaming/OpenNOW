#pragma once

#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QVector>

#include <functional>

namespace OpnQt {

struct StreamSettings {
    QString resolution = QStringLiteral("1920x1080");
    int fps = 60;
    QString codec = QStringLiteral("H264");
    QString colorQuality = QStringLiteral("8bit_420");
    int maxBitrateMbps = 50;
    bool enableCloudGsync = false;
    bool enableL4S = false;
    bool enableReflex = true;
    QString microphoneMode = QStringLiteral("disabled");
    QString keyboardLayout = QStringLiteral("us");
    QString gameLanguage = QStringLiteral("en_US");
    bool accountLinked = true;
    QString selectedStore;
};

struct IceServer {
    QStringList urls;
    QString username;
    QString credential;
};

struct MediaConnectionInfo {
    QString ip;
    int port = 0;
};

struct NegotiatedStreamProfile {
    QString resolution;
    int fps = 0;
    QString codec;
    QString colorQuality;
    int bitDepth = -1;
    int chromaFormat = -1;
};

struct SessionInfo {
    QString sessionId;
    int status = 0;
    int queuePosition = 0;
    int seatSetupStep = 0;
    QString zone;
    QString streamingBaseUrl;
    QString serverIp;
    QString signalingServer;
    QString signalingUrl;
    QString gpuType;
    QVector<IceServer> iceServers;
    MediaConnectionInfo mediaConnectionInfo;
    NegotiatedStreamProfile negotiatedStreamProfile;
    QString clientId;
    QString deviceId;
};

struct IceCandidatePayload {
    QString candidate;
    QString sdpMid;
    int sdpMLineIndex = 0;
    QString usernameFragment;
};

struct SendAnswerRequest {
    QString sdp;
    QString nvstSdp;
};

struct ActiveSessionEntry {
    QString sessionId;
    int appId = 0;
    int status = 0;
    QString serverIp;
    QString gpuType;
    QString streamingBaseUrl;
    QString signalingUrl;
};

using SessionCreateCallback = std::function<void(bool success, const SessionInfo &info, const QString &error)>;
using SessionPollCallback = std::function<void(bool success, const SessionInfo &info, const QString &error)>;
using ActiveSessionsCallback = std::function<void(bool success, const QVector<ActiveSessionEntry> &sessions, const QString &error)>;

} // namespace OpnQt
