#include "OpnQtSessionManager.h"

#include "OpnQtAuthService.h"

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QDebug>
#include <QtCore/QRandomGenerator>
#include <QtCore/QTimeZone>
#include <QtCore/QUrl>
#include <QtCore/QUrlQuery>
#include <QtCore/QUuid>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>

namespace OpnQt {
namespace {

constexpr const char *kNvClientId = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
constexpr const char *kNvClientVersion = "2.0.80.173";
constexpr const char *kDefaultStreamingBaseUrl = "https://prod.cloudmatchbeta.nvidiagrid.net";
constexpr const char *kUserAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

QString normalizedBaseUrl(QString url) {
    if (url.trimmed().isEmpty()) return QString::fromLatin1(kDefaultStreamingBaseUrl);
    while (url.endsWith(QLatin1Char('/'))) url.chop(1);
    return url;
}

bool isZoneHostname(const QString &host) {
    return host.contains(QStringLiteral("cloudmatchbeta.nvidiagrid.net")) || host.contains(QStringLiteral("cloudmatch.nvidiagrid.net"));
}

QString resolveSessionBaseUrl(const QString &streamingBaseUrl, const QString &serverIp) {
    if (serverIp.isEmpty() || isZoneHostname(serverIp)) return normalizedBaseUrl(streamingBaseUrl);
    return QStringLiteral("https://") + serverIp;
}

QString randomUuid() {
    return QUuid::createUuid().toString(QUuid::WithoutBraces);
}

QString hostFromResourcePath(const QString &value) {
    const QUrl url(value);
    return url.host();
}

bool usableEndpointHost(const QString &host) {
    return !host.isEmpty() && !host.startsWith(QLatin1Char('.'));
}

QString signalingUrlFromEndpoint(const QString &host, int port, const QString &resourcePath) {
    if (resourcePath.startsWith(QStringLiteral("wss://"))) return resourcePath;
    if (resourcePath.startsWith(QStringLiteral("rtsps://"))) {
        const QString resourceHost = hostFromResourcePath(resourcePath);
        if (!resourceHost.isEmpty()) return QStringLiteral("wss://%1/nvst/").arg(resourceHost);
    }
    const int resolvedPort = port > 0 ? port : 443;
    const QString path = resourcePath.isEmpty() ? QStringLiteral("/nvst/") : resourcePath;
    return QStringLiteral("wss://%1:%2%3").arg(host).arg(resolvedPort).arg(path);
}

QVariantMap jsonMapFromBytes(const QByteArray &data) {
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(data, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) return {};
    return document.object().toVariantMap();
}

QNetworkRequest sessionRequest(const QUrl &url, const QString &token, const QString &deviceId) {
    QNetworkRequest request(url);
    request.setRawHeader("User-Agent", kUserAgent);
    request.setRawHeader("Authorization", (QStringLiteral("GFNJWT ") + token).toUtf8());
    request.setRawHeader("Content-Type", "application/json");
    request.setRawHeader("nv-client-id", kNvClientId);
    request.setRawHeader("nv-client-type", "NATIVE");
    request.setRawHeader("nv-client-version", kNvClientVersion);
    request.setRawHeader("nv-client-streamer", "NVIDIA-CLASSIC");
    request.setRawHeader("nv-device-os", "MACOS");
    request.setRawHeader("nv-device-type", "DESKTOP");
    request.setRawHeader("nv-device-make", "UNKNOWN");
    request.setRawHeader("nv-device-model", "UNKNOWN");
    request.setRawHeader("nv-browser-type", "CHROME");
    request.setRawHeader("x-device-id", deviceId.toUtf8());
    request.setRawHeader("Origin", "https://play.geforcenow.com");
    return request;
}

int intValue(const QVariantMap &map, const QString &key, int fallback = 0) {
    bool ok = false;
    const int parsed = map.value(key).toInt(&ok);
    return ok ? parsed : fallback;
}

QString stringValue(const QVariantMap &map, const QString &key) {
    return map.value(key).toString();
}

QVariantMap requestedStreamingFeatures(const StreamSettings &settings) {
    int bitDepth = 0;
    int chromaFormat = 0;
    if (settings.colorQuality == QStringLiteral("10bit_420")) bitDepth = 10;
    else if (settings.colorQuality == QStringLiteral("8bit_444")) chromaFormat = 2;
    else if (settings.colorQuality == QStringLiteral("10bit_444")) {
        bitDepth = 10;
        chromaFormat = 2;
    }
    return {
        {QStringLiteral("reflex"), settings.enableReflex},
        {QStringLiteral("bitDepth"), bitDepth},
        {QStringLiteral("cloudGsync"), settings.enableCloudGsync},
        {QStringLiteral("enabledL4S"), settings.enableL4S},
        {QStringLiteral("mouseMovementFlags"), 0},
        {QStringLiteral("trueHdr"), false},
        {QStringLiteral("supportedHidDevices"), 0},
        {QStringLiteral("profile"), 0},
        {QStringLiteral("fallbackToLogicalResolution"), false},
        {QStringLiteral("hidDevices"), QVariant()},
        {QStringLiteral("chromaFormat"), chromaFormat},
        {QStringLiteral("prefilterMode"), 0},
        {QStringLiteral("prefilterSharpness"), 0},
        {QStringLiteral("prefilterNoiseReduction"), 0},
        {QStringLiteral("hudStreamingMode"), 0},
        {QStringLiteral("sdrColorSpace"), 2},
        {QStringLiteral("hdrColorSpace"), 0},
    };
}

void parseStreamProfile(const QVariantMap &session, NegotiatedStreamProfile &profile) {
    const QVariantMap negotiated = session.value(QStringLiteral("negotiatedStreamProfile")).toMap();
    profile.resolution = negotiated.value(QStringLiteral("resolution")).toString();
    profile.codec = negotiated.value(QStringLiteral("codec")).toString();
    profile.fps = negotiated.value(QStringLiteral("fps")).toInt();
    const QVariantMap features = session.value(QStringLiteral("finalizedStreamingFeatures")).toMap();
    if (!features.isEmpty()) {
        profile.bitDepth = features.value(QStringLiteral("bitDepth"), -1).toInt();
        profile.chromaFormat = features.value(QStringLiteral("chromaFormat"), -1).toInt();
        if (profile.bitDepth >= 10 && profile.chromaFormat == 2) profile.colorQuality = QStringLiteral("10bit_444");
        else if (profile.bitDepth >= 10) profile.colorQuality = QStringLiteral("10bit_420");
        else if (profile.chromaFormat == 2) profile.colorQuality = QStringLiteral("8bit_444");
        else profile.colorQuality = QStringLiteral("8bit_420");
    }
}

SessionInfo parseSessionInfo(const QVariantMap &session, const QString &baseUrl, const QString &clientId, const QString &deviceId) {
    SessionInfo info;
    info.sessionId = stringValue(session, QStringLiteral("sessionId"));
    info.status = intValue(session, QStringLiteral("status"));
    info.zone = baseUrl;
    info.streamingBaseUrl = baseUrl;
    info.gpuType = stringValue(session, QStringLiteral("gpuType"));
    info.queuePosition = intValue(session, QStringLiteral("queuePosition"));
    info.clientId = clientId;
    info.deviceId = deviceId;

    for (const QVariant &raw : session.value(QStringLiteral("connectionInfo")).toList()) {
        const QVariantMap conn = raw.toMap();
        const int usage = intValue(conn, QStringLiteral("usage"));
        const int port = intValue(conn, QStringLiteral("port"));
        QString ip = stringValue(conn, QStringLiteral("ip"));
        const QString resourcePath = stringValue(conn, QStringLiteral("resourcePath"));
        if (!usableEndpointHost(ip)) ip.clear();
        if (ip.isEmpty() && !resourcePath.isEmpty()) ip = hostFromResourcePath(resourcePath);
        qInfo() << "[SessionManager] connectionInfo"
                << "usage" << usage
                << "ip" << ip
                << "port" << port
                << "resourcePath" << resourcePath;

        if (usage == 14 && !ip.isEmpty()) {
            info.serverIp = ip;
            info.signalingServer = QStringLiteral("%1:%2").arg(ip).arg(port > 0 ? port : 443);
            info.signalingUrl = signalingUrlFromEndpoint(ip, port, resourcePath);
            if (port > 0) {
                info.mediaConnectionInfo.ip = ip;
                info.mediaConnectionInfo.port = port;
            }
        } else if (usage == 2 && !ip.isEmpty() && port > 0) {
            info.mediaConnectionInfo.ip = ip;
            info.mediaConnectionInfo.port = port;
        } else if (info.signalingUrl.isEmpty() && !resourcePath.isEmpty() && !ip.isEmpty()) {
            const bool looksLikeSignaling = resourcePath.contains(QStringLiteral("/nvst"), Qt::CaseInsensitive)
                || resourcePath.startsWith(QStringLiteral("wss://"))
                || resourcePath.startsWith(QStringLiteral("rtsps://"));
            if (looksLikeSignaling) {
                info.serverIp = ip;
                info.signalingServer = QStringLiteral("%1:%2").arg(ip).arg(port > 0 ? port : 443);
                info.signalingUrl = signalingUrlFromEndpoint(ip, port, resourcePath);
            }
        }
    }

    if (info.serverIp.isEmpty()) {
        info.serverIp = session.value(QStringLiteral("sessionControlInfo")).toMap().value(QStringLiteral("ip")).toString();
    }
    if (info.signalingUrl.isEmpty() && usableEndpointHost(info.serverIp)) {
        info.signalingServer = QStringLiteral("%1:443").arg(info.serverIp);
        info.signalingUrl = QStringLiteral("wss://%1:443/nvst/").arg(info.serverIp);
    }
    qInfo() << "[SessionManager] parsed endpoints"
            << "serverIp" << info.serverIp
            << "signalingServer" << info.signalingServer
            << "signalingUrl" << info.signalingUrl
            << "media" << info.mediaConnectionInfo.ip << info.mediaConnectionInfo.port;

    for (const QVariant &raw : session.value(QStringLiteral("iceServers")).toList()) {
        const QVariantMap iceMap = raw.toMap();
        IceServer ice;
        for (const QVariant &url : iceMap.value(QStringLiteral("urls")).toList()) ice.urls.append(url.toString());
        ice.username = iceMap.value(QStringLiteral("username")).toString();
        ice.credential = iceMap.value(QStringLiteral("credential")).toString();
        info.iceServers.append(ice);
    }

    parseStreamProfile(session, info.negotiatedStreamProfile);
    return info;
}

} // namespace

SessionManager &SessionManager::shared() {
    static SessionManager manager;
    return manager;
}

SessionManager::SessionManager(QObject *parent) : QObject(parent), m_network(new QNetworkAccessManager(this)) {}

void SessionManager::setAccessToken(const QString &token) {
    m_accessToken = token;
}

void SessionManager::setStreamingBaseUrl(const QString &url) {
    m_streamingBaseUrl = normalizedBaseUrl(url);
}

QString SessionManager::streamingBaseUrl() const {
    return normalizedBaseUrl(m_streamingBaseUrl);
}

void SessionManager::createSession(const QString &appId,
                                   const QString &internalTitle,
                                   const StreamSettings &settings,
                                   SessionCreateCallback completion) {
    if (m_accessToken.isEmpty()) {
        completion(false, {}, QStringLiteral("No access token"));
        return;
    }

    const QString baseUrl = streamingBaseUrl();
    const QString clientId = randomUuid();
    const QString deviceId = AuthService::persistentDeviceUuid();
    QStringList resolution = settings.resolution.split(QLatin1Char('x'));
    const int width = resolution.size() == 2 ? resolution[0].toInt() : 1920;
    const int height = resolution.size() == 2 ? resolution[1].toInt() : 1080;
    const int timezoneOffset = -QTimeZone::systemTimeZone().offsetFromUtc(QDateTime::currentDateTimeUtc()) * 1000;

    QVariantMap sessionRequestData{
        {QStringLiteral("appId"), appId},
        {QStringLiteral("internalTitle"), internalTitle},
        {QStringLiteral("availableSupportedControllers"), QVariantList{}},
        {QStringLiteral("networkTestSessionId"), QVariant()},
        {QStringLiteral("parentSessionId"), QVariant()},
        {QStringLiteral("clientIdentification"), QStringLiteral("GFN-PC")},
        {QStringLiteral("deviceHashId"), deviceId},
        {QStringLiteral("clientVersion"), QStringLiteral("30.0")},
        {QStringLiteral("sdkVersion"), QStringLiteral("1.0")},
        {QStringLiteral("streamerVersion"), 1},
        {QStringLiteral("clientPlatformName"), QStringLiteral("windows")},
        {QStringLiteral("clientRequestMonitorSettings"), QVariantList{QVariantMap{
            {QStringLiteral("monitorId"), 0},
            {QStringLiteral("positionX"), 0},
            {QStringLiteral("positionY"), 0},
            {QStringLiteral("widthInPixels"), width},
            {QStringLiteral("heightInPixels"), height},
            {QStringLiteral("framesPerSecond"), settings.fps},
            {QStringLiteral("sdrHdrMode"), 0},
            {QStringLiteral("displayData"), QVariant()},
            {QStringLiteral("hdr10PlusGamingData"), QVariant()},
            {QStringLiteral("dpi"), 100},
        }}},
        {QStringLiteral("useOps"), true},
        {QStringLiteral("audioMode"), 2},
        {QStringLiteral("metaData"), QVariantList{
            QVariantMap{{QStringLiteral("key"), QStringLiteral("SubSessionId")}, {QStringLiteral("value"), randomUuid()}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("wssignaling")}, {QStringLiteral("value"), QStringLiteral("1")}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("GSStreamerType")}, {QStringLiteral("value"), QStringLiteral("WebRTC")}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("networkType")}, {QStringLiteral("value"), QStringLiteral("Unknown")}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("ClientImeSupport")}, {QStringLiteral("value"), QStringLiteral("0")}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("clientPhysicalResolution")}, {QStringLiteral("value"), QStringLiteral("{\"horizontalPixels\":%1,\"verticalPixels\":%2}").arg(width).arg(height)}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("surroundAudioInfo")}, {QStringLiteral("value"), QStringLiteral("2")}},
            QVariantMap{{QStringLiteral("key"), QStringLiteral("store")}, {QStringLiteral("value"), settings.selectedStore.isEmpty() ? QStringLiteral("unknown") : settings.selectedStore}},
        }},
        {QStringLiteral("sdrHdrMode"), 0},
        {QStringLiteral("clientDisplayHdrCapabilities"), QVariant()},
        {QStringLiteral("surroundAudioInfo"), 0},
        {QStringLiteral("remoteControllersBitmap"), 0},
        {QStringLiteral("clientTimezoneOffset"), timezoneOffset},
        {QStringLiteral("enhancedStreamMode"), 1},
        {QStringLiteral("appLaunchMode"), 1},
        {QStringLiteral("secureRTSPSupported"), false},
        {QStringLiteral("partnerCustomData"), QString()},
        {QStringLiteral("accountLinked"), settings.accountLinked},
        {QStringLiteral("enablePersistingInGameSettings"), true},
        {QStringLiteral("userAge"), 26},
        {QStringLiteral("requestedStreamingFeatures"), requestedStreamingFeatures(settings)},
    };

    QUrl url(baseUrl + QStringLiteral("/v2/session"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("keyboardLayout"), settings.keyboardLayout);
    query.addQueryItem(QStringLiteral("languageCode"), settings.gameLanguage);
    url.setQuery(query);
    QNetworkRequest request = sessionRequest(url, m_accessToken, deviceId);
    QNetworkReply *reply = m_network->post(request, QJsonDocument(QJsonObject::fromVariantMap({{QStringLiteral("sessionRequestData"), sessionRequestData}})).toJson(QJsonDocument::Compact));
    connect(reply, &QNetworkReply::finished, this, [reply, completion, baseUrl, clientId, deviceId]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        reply->deleteLater();
        if (!networkError.isEmpty() || status != 200) {
            completion(false, {}, !networkError.isEmpty() ? networkError : QStringLiteral("HTTP %1: %2").arg(status).arg(QString::fromUtf8(data)));
            return;
        }
        const QVariantMap json = jsonMapFromBytes(data);
        const QVariantMap requestStatus = json.value(QStringLiteral("requestStatus")).toMap();
        if (requestStatus.value(QStringLiteral("statusCode")).toInt() != 1) {
            completion(false, {}, QStringLiteral("API error %1: %2").arg(requestStatus.value(QStringLiteral("statusCode")).toString(), requestStatus.value(QStringLiteral("statusDescription")).toString()));
            return;
        }
        const QVariantMap session = json.value(QStringLiteral("session")).toMap();
        if (session.isEmpty()) {
            completion(false, {}, QStringLiteral("No session in response"));
            return;
        }
        completion(true, parseSessionInfo(session, baseUrl, clientId, deviceId), QString());
    });
}

void SessionManager::pollSession(const QString &sessionId,
                                 const QString &serverIp,
                                 SessionPollCallback completion) {
    if (m_accessToken.isEmpty()) {
        completion(false, {}, QStringLiteral("No access token"));
        return;
    }
    const QString baseUrl = resolveSessionBaseUrl(m_streamingBaseUrl, serverIp);
    const QString deviceId = AuthService::persistentDeviceUuid();
    QNetworkReply *reply = m_network->get(sessionRequest(QUrl(QStringLiteral("%1/v2/session/%2").arg(baseUrl, sessionId)), m_accessToken, deviceId));
    connect(reply, &QNetworkReply::finished, this, [reply, completion, baseUrl, deviceId]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        reply->deleteLater();
        if (!networkError.isEmpty() || status != 200) {
            completion(false, {}, !networkError.isEmpty() ? networkError : QStringLiteral("HTTP %1: %2").arg(status).arg(QString::fromUtf8(data)));
            return;
        }
        const QVariantMap session = jsonMapFromBytes(data).value(QStringLiteral("session")).toMap();
        if (session.isEmpty()) {
            completion(false, {}, QStringLiteral("No session in poll response"));
            return;
        }
        completion(true, parseSessionInfo(session, baseUrl, QString(), deviceId), QString());
    });
}

void SessionManager::getActiveSessions(ActiveSessionsCallback completion) {
    completion(false, {}, QStringLiteral("Active session lookup is not ported yet"));
}

} // namespace OpnQt
