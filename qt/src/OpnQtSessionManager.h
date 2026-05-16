#pragma once

#include "OpnQtStreamTypes.h"

#include <QtCore/QObject>

class QNetworkAccessManager;

namespace OpnQt {

class SessionManager final : public QObject {
public:
    static SessionManager &shared();

    void setAccessToken(const QString &token);
    void setStreamingBaseUrl(const QString &url);
    QString streamingBaseUrl() const;

    void createSession(const QString &appId,
                       const QString &internalTitle,
                       const StreamSettings &settings,
                       SessionCreateCallback completion);
    void pollSession(const QString &sessionId,
                     const QString &serverIp,
                     SessionPollCallback completion);
    void getActiveSessions(ActiveSessionsCallback completion);

private:
    explicit SessionManager(QObject *parent = nullptr);

    QNetworkAccessManager *m_network = nullptr;
    QString m_accessToken;
    QString m_streamingBaseUrl;
};

} // namespace OpnQt
