#pragma once

#include "OpnQtCatalogView.h"
#include "OpnQtStreamTypes.h"

#include <QtCore/QList>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <functional>

class QNetworkAccessManager;
class QNetworkReply;

namespace OpnQt {

struct GameVariant {
    QString id;
    QString appStore;
    QString serviceStatus;
    bool librarySelected = false;
    bool inLibrary = false;
};

struct GameInfo {
    QString id;
    QString uuid;
    QString launchAppId;
    QString title;
    QString shortName;
    QString description;
    QString playType;
    QString playabilityState;
    QString imageUrl;
    QString heroImageUrl;
    QStringList screenshotUrls;
    QStringList genres;
    QStringList featureLabels;
    QStringList availableStores;
    QList<GameVariant> variants;
    bool isInLibrary = false;
};

using LibraryCallback = std::function<void(bool success, const QList<CatalogGame> &games, const QString &error)>;
using LaunchProgressCallback = std::function<void(const QString &message, const SessionInfo &session)>;
using LaunchCallback = std::function<void(bool success, const SessionInfo &session, const QString &error)>;

class GameService final : public QObject {
public:
    static GameService &shared();

    void setAccessToken(const QString &token);
    void fetchCatalogGames(LibraryCallback completion);
    void fetchLibraryGames(LibraryCallback completion);
    void setStreamingBaseUrl(const QString &url);
    void launchGame(const QString &appId,
                    const QString &internalTitle,
                    const StreamSettings &settings,
                    LaunchProgressCallback progress,
                    LaunchCallback completion);

private:
    explicit GameService(QObject *parent = nullptr);

    void fetchServerVpcId(std::function<void(const QString &vpcId)> completion);
    void postGraphQL(const QString &operationName,
                     const QString &queryHash,
                     const QVariantMap &variables,
                     std::function<void(const QVariantMap &, const QString &)> completion);
    void postGraphQlJson(const QString &query,
                         const QVariantMap &variables,
                         std::function<void(const QVariantMap &, const QString &)> completion);
    void fetchMetadata(const QList<GameInfo> &games, const QString &vpcId, LibraryCallback completion);
    GameInfo parseGameItem(const QVariantMap &app) const;
    CatalogGame toCatalogGame(const GameInfo &game) const;
    void pollSessionReady(const QString &sessionId,
                          const QString &serverIp,
                          int retries,
                          LaunchProgressCallback progress,
                          LaunchCallback completion);

    QNetworkAccessManager *m_network = nullptr;
    QString m_accessToken;
    QString m_streamingBaseUrl;
};

} // namespace OpnQt
