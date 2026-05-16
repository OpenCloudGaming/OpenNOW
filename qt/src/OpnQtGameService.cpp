#include "OpnQtGameService.h"

#include "OpnQtSessionManager.h"

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QDateTime>
#include <QtCore/QSet>
#include <QtCore/QSharedPointer>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtCore/QUrlQuery>
#include <QtCore/QUuid>
#include <QtCore/QVariantMap>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>

#include <algorithm>

namespace OpnQt {
namespace {

constexpr const char *kPanelsHash = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
constexpr const char *kLibraryWithTimeHash = "039e8c0d553972975485fee56e59f2549d2fdb518e247a42ab5022056a74406f";
constexpr const char *kAppMetaDataHash = "cf8b620dfd03617017ba7c858cee65197e1ace5180e41be194b39227227ced63";
constexpr const char *kNvClientId = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
constexpr const char *kNvClientVersion = "2.0.80.173";
constexpr const char *kDefaultUserAgent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 GFN-PC/2.0.80.173";
constexpr const char *kServerInfoUserAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
constexpr int kDefaultCatalogFetchCount = 96;
constexpr int kMaxCatalogPages = 3;

QVariantMap jsonMapFromBytes(const QByteArray &data) {
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(data, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) return {};
    return document.object().toVariantMap();
}

QString safeString(const QVariantMap &map, const QString &key) {
    const QVariant value = map.value(key);
    if (value.typeId() == QMetaType::QString) return value.toString();
    if (value.canConvert<QString>()) return value.toString();
    return {};
}

QString firstString(const QVariantMap &map, const QStringList &keys) {
    for (const QString &key : keys) {
        const QString value = safeString(map, key);
        if (!value.isEmpty()) return value;
    }
    return {};
}

void appendUnique(QStringList &values, const QString &value) {
    const QString trimmed = value.trimmed();
    if (!trimmed.isEmpty() && !values.contains(trimmed, Qt::CaseInsensitive)) values.append(trimmed);
}

QStringList stringValues(const QVariant &raw) {
    QStringList values;
    if (raw.typeId() == QMetaType::QString) {
        appendUnique(values, raw.toString());
        return values;
    }
    const QVariantList list = raw.toList();
    for (const QVariant &item : list) {
        if (item.typeId() == QMetaType::QString) {
            appendUnique(values, item.toString());
        } else {
            const QVariantMap map = item.toMap();
            appendUnique(values, firstString(map, {QStringLiteral("name"), QStringLiteral("label"), QStringLiteral("value"), QStringLiteral("rating"), QStringLiteral("control"), QStringLiteral("type")}));
        }
    }
    return values;
}

QString optimizeImageUrl(QString url, int width) {
    if (url.contains(QStringLiteral("img.nvidiagrid.net")) && !url.contains(QStringLiteral(";f=webp"))) {
        url += QStringLiteral(";f=webp;w=%1").arg(width);
    }
    return url;
}

QString prettyStore(QString store) {
    const QString upper = store.toUpper();
    if (upper.contains(QStringLiteral("STEAM"))) return QStringLiteral("Steam");
    if (upper.contains(QStringLiteral("EPIC")) || upper.contains(QStringLiteral("EGS"))) return QStringLiteral("Epic");
    if (upper.contains(QStringLiteral("UBISOFT")) || upper.contains(QStringLiteral("UPLAY"))) return QStringLiteral("Ubisoft");
    if (upper.contains(QStringLiteral("BATTLE"))) return QStringLiteral("Battle.net");
    if (upper.contains(QStringLiteral("XBOX")) || upper.contains(QStringLiteral("MICROSOFT"))) return QStringLiteral("Xbox");
    if (upper.contains(QStringLiteral("EA")) || upper.contains(QStringLiteral("ORIGIN"))) return QStringLiteral("EA");
    if (upper.contains(QStringLiteral("GOG"))) return QStringLiteral("GOG");
    return store.isEmpty() ? QStringLiteral("Cloud") : store.left(1).toUpper() + store.mid(1).toLower();
}

bool hasVisibleVariants(const GameInfo &game) {
    return !game.variants.isEmpty();
}

QNetworkRequest baseRequest(const QUrl &url, const QString &accessToken) {
    QNetworkRequest request(url);
    request.setRawHeader("Origin", "https://play.geforcenow.com");
    request.setRawHeader("Referer", "https://play.geforcenow.com/");
    request.setRawHeader("Accept", "application/json, text/plain, */*");
    request.setRawHeader("nv-client-id", kNvClientId);
    request.setRawHeader("nv-client-type", "NATIVE");
    request.setRawHeader("nv-client-version", kNvClientVersion);
    request.setRawHeader("nv-client-streamer", "NVIDIA-CLASSIC");
    request.setRawHeader("nv-device-os", "MACOS");
    request.setRawHeader("nv-device-type", "DESKTOP");
    request.setRawHeader("nv-device-make", "UNKNOWN");
    request.setRawHeader("nv-device-model", "UNKNOWN");
    request.setRawHeader("nv-browser-type", "CHROME");
    request.setRawHeader("User-Agent", kDefaultUserAgent);
    if (!accessToken.isEmpty()) request.setRawHeader("Authorization", (QStringLiteral("GFNJWT ") + accessToken).toUtf8());
    return request;
}

QNetworkRequest serverInfoRequest(const QUrl &url, const QString &accessToken) {
    QNetworkRequest request(url);
    request.setRawHeader("Accept", "application/json");
    request.setRawHeader("nv-client-id", kNvClientId);
    request.setRawHeader("nv-client-type", "NATIVE");
    request.setRawHeader("nv-client-version", kNvClientVersion);
    request.setRawHeader("nv-client-streamer", "NVIDIA-CLASSIC");
    request.setRawHeader("nv-device-os", "MACOS");
    request.setRawHeader("nv-device-type", "DESKTOP");
    request.setRawHeader("User-Agent", kServerInfoUserAgent);
    if (!accessToken.isEmpty()) request.setRawHeader("Authorization", (QStringLiteral("GFNJWT ") + accessToken).toUtf8());
    return request;
}

QString graphQlHuId() {
    const QString uuidPrefix = QUuid::createUuid().toString(QUuid::WithoutBraces).remove(QLatin1Char('-')).left(8).toUpper();
    return QString::number(QDateTime::currentMSecsSinceEpoch(), 16) + uuidPrefix;
}

QColor colorForTitle(const QString &title) {
    uint hash = 2166136261u;
    for (QChar c : title) {
        hash ^= c.unicode();
        hash *= 16777619u;
    }
    return QColor::fromHsv(static_cast<int>(hash % 360), 150, 190);
}

} // namespace

GameService &GameService::shared() {
    static GameService service;
    return service;
}

GameService::GameService(QObject *parent) : QObject(parent), m_network(new QNetworkAccessManager(this)) {}

void GameService::setAccessToken(const QString &token) {
    m_accessToken = token;
}

void GameService::setStreamingBaseUrl(const QString &url) {
    m_streamingBaseUrl = url;
    SessionManager::shared().setStreamingBaseUrl(url);
}

void GameService::launchGame(const QString &appId,
                             const QString &internalTitle,
                             const StreamSettings &settings,
                             LaunchProgressCallback progress,
                             LaunchCallback completion) {
    SessionManager::shared().setAccessToken(m_accessToken);
    if (!m_streamingBaseUrl.isEmpty()) SessionManager::shared().setStreamingBaseUrl(m_streamingBaseUrl);
    SessionManager::shared().createSession(appId, internalTitle, settings,
        [this, progress = std::move(progress), completion = std::move(completion)](bool success, const SessionInfo &info, const QString &error) mutable {
            if (!success) {
                completion(false, {}, error);
                return;
            }
            if (progress) progress(QStringLiteral("Waiting for cloud session..."), info);
            if ((info.status == 2 || info.status == 3) && !info.serverIp.isEmpty()) {
                completion(true, info, QString());
                return;
            }
            if (info.sessionId.isEmpty()) {
                completion(false, {}, QStringLiteral("Session response did not include a session id"));
                return;
            }
            pollSessionReady(info.sessionId, info.serverIp, 0, std::move(progress), std::move(completion));
        });
}

void GameService::pollSessionReady(const QString &sessionId,
                                   const QString &serverIp,
                                   int retries,
                                   LaunchProgressCallback progress,
                                   LaunchCallback completion) {
    constexpr int kMaxRetries = 60;
    if (retries >= kMaxRetries) {
        completion(false, {}, QStringLiteral("Session poll timeout"));
        return;
    }
    QTimer::singleShot(retries == 0 ? 0 : 1000, this, [this, sessionId, serverIp, retries, progress = std::move(progress), completion = std::move(completion)]() mutable {
        SessionManager::shared().pollSession(sessionId, serverIp,
            [this, sessionId, serverIp, retries, progress = std::move(progress), completion = std::move(completion)](bool ok, const SessionInfo &info, const QString &error) mutable {
                if (ok && progress) progress(QStringLiteral("Waiting for cloud session..."), info);
                if (ok && (info.status == 2 || info.status == 3) && !info.serverIp.isEmpty()) {
                    completion(true, info, QString());
                    return;
                }
                if (ok && info.status > 3 && info.status != 6) {
                    completion(false, {}, QStringLiteral("Session entered terminal state %1").arg(info.status));
                    return;
                }
                if (!ok && retries + 1 >= kMaxRetries) {
                    completion(false, {}, error.isEmpty() ? QStringLiteral("Session poll failed") : error);
                    return;
                }
                pollSessionReady(sessionId, ok && !info.serverIp.isEmpty() ? info.serverIp : serverIp, retries + 1, std::move(progress), std::move(completion));
            });
    });
}

void GameService::fetchServerVpcId(std::function<void(const QString &vpcId)> completion) {
    QNetworkRequest request = serverInfoRequest(QUrl(QStringLiteral("https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo")), m_accessToken);
    QNetworkReply *reply = m_network->get(request);
    connect(reply, &QNetworkReply::finished, this, [reply, completion]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QVariantMap json = jsonMapFromBytes(data);
        reply->deleteLater();
        if (status != 200 || json.isEmpty()) {
            completion(QStringLiteral("GFN-PC"));
            return;
        }
        const QVariantMap requestStatus = json.value(QStringLiteral("requestStatus")).toMap();
        const QString serverId = requestStatus.value(QStringLiteral("serverId")).toString();
        completion(serverId.isEmpty() ? QStringLiteral("GFN-PC") : serverId);
    });
}

void GameService::postGraphQL(const QString &operationName,
                              const QString &queryHash,
                              const QVariantMap &variables,
                              std::function<void(const QVariantMap &, const QString &)> completion) {
    QVariantMap extensions;
    extensions.insert(QStringLiteral("persistedQuery"), QVariantMap{{QStringLiteral("sha256Hash"), queryHash}});

    QUrl url(QStringLiteral("https://games.geforce.com/graphql"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("requestType"), operationName);
    query.addQueryItem(QStringLiteral("extensions"), QString::fromUtf8(QJsonDocument(QJsonObject::fromVariantMap(extensions)).toJson(QJsonDocument::Compact)));
    query.addQueryItem(QStringLiteral("huId"), graphQlHuId());
    query.addQueryItem(QStringLiteral("variables"), QString::fromUtf8(QJsonDocument(QJsonObject::fromVariantMap(variables)).toJson(QJsonDocument::Compact)));
    url.setQuery(query);

    QNetworkRequest request = baseRequest(url, m_accessToken);
    request.setRawHeader("Content-Type", "application/graphql");
    QNetworkReply *reply = m_network->get(request);
    connect(reply, &QNetworkReply::finished, this, [reply, completion]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        const QVariantMap json = jsonMapFromBytes(data);
        reply->deleteLater();
        if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
            completion({}, !networkError.isEmpty() ? networkError : QStringLiteral("GraphQL error (%1)").arg(status));
            return;
        }
        const QVariantList errors = json.value(QStringLiteral("errors")).toList();
        if (!errors.isEmpty()) {
            const QVariantMap first = errors.first().toMap();
            completion(json, first.value(QStringLiteral("message"), QStringLiteral("GraphQL error")).toString());
            return;
        }
        const QVariantMap dataMap = json.value(QStringLiteral("data")).toMap();
        if (dataMap.isEmpty()) {
            completion(json, QStringLiteral("No data in GraphQL response"));
            return;
        }
        completion(dataMap, QString());
    });
}

void GameService::postGraphQlJson(const QString &queryText,
                                  const QVariantMap &variables,
                                  std::function<void(const QVariantMap &, const QString &)> completion) {
    QVariantMap body;
    body.insert(QStringLiteral("query"), queryText);
    body.insert(QStringLiteral("variables"), variables);

    QNetworkRequest request = baseRequest(QUrl(QStringLiteral("https://games.geforce.com/graphql")), m_accessToken);
    request.setHeader(QNetworkRequest::ContentTypeHeader, QStringLiteral("application/json"));
    QNetworkReply *reply = m_network->post(request, QJsonDocument(QJsonObject::fromVariantMap(body)).toJson(QJsonDocument::Compact));
    connect(reply, &QNetworkReply::finished, this, [reply, completion]() {
        const QByteArray data = reply->readAll();
        const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const QString networkError = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
        const QVariantMap json = jsonMapFromBytes(data);
        reply->deleteLater();
        if (!networkError.isEmpty() || status != 200 || json.isEmpty()) {
            completion({}, !networkError.isEmpty() ? networkError : QStringLiteral("GraphQL error (%1)").arg(status));
            return;
        }
        const QVariantList errors = json.value(QStringLiteral("errors")).toList();
        if (!errors.isEmpty()) {
            completion(json, errors.first().toMap().value(QStringLiteral("message"), QStringLiteral("GraphQL error")).toString());
            return;
        }
        const QVariantMap dataMap = json.value(QStringLiteral("data")).toMap();
        if (dataMap.isEmpty()) {
            completion(json, QStringLiteral("No data in GraphQL response"));
            return;
        }
        completion(dataMap, QString());
    });
}

void GameService::fetchCatalogGames(LibraryCallback completion) {
    if (m_accessToken.isEmpty()) {
        completion(false, {}, QStringLiteral("No access token"));
        return;
    }

    fetchServerVpcId([this, completion](const QString &vpcId) {
        const QString definitionsQuery = QStringLiteral(R"(
        query GetFilterGroupAndSortOrderDefinitions($locale: String!) {
            filterGroupDefinitions(language: $locale) { id label filters { id label filters } }
            sortOrderDefinitions(language: $locale) { id label orderBy }
        }
        )");
        postGraphQlJson(definitionsQuery, {{QStringLiteral("locale"), QStringLiteral("en_US")}},
                        [this, completion, vpcId](const QVariantMap &definitionsData, const QString &definitionsError) {
            if (!definitionsError.isEmpty()) {
                completion(false, {}, definitionsError);
                return;
            }

            QString sortString = QStringLiteral("itemMetadata.relevance:DESC,sortName:ASC");
            const QVariantList sorts = definitionsData.value(QStringLiteral("sortOrderDefinitions")).toList();
            for (const QVariant &value : sorts) {
                const QVariantMap sort = value.toMap();
                if (sort.value(QStringLiteral("id")).toString() == QStringLiteral("last_played")) {
                    const QString orderBy = sort.value(QStringLiteral("orderBy")).toString();
                    if (!orderBy.isEmpty()) sortString = orderBy;
                    break;
                }
            }

            const QString queryText = QStringLiteral(R"(
        query GetFilterBrowseResults(
            $vpcId: String!,
            $locale: String!,
            $sortString: String!,
            $fetchCount: Int!,
            $cursor: String!,
            $filters: AppFilterFields!
        ) {
            apps(
                vpcId: $vpcId,
                language: $locale,
                orderBy: $sortString,
                first: $fetchCount,
                after: $cursor,
                filters: $filters
            ) {
                numberReturned
                numberSupported
                pageInfo { hasNextPage endCursor totalCount }
                items {
                    id
                    title
                    images { KEY_ART GAME_BOX_ART TV_BANNER HERO_IMAGE SCREENSHOTS }
                    variants { id appStore supportedControls gfn { status library { status selected } } }
                    gfn { playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG } }
                    itemMetadata { campaignIds }
                }
            }
        }
        )");

            auto collected = QSharedPointer<QList<GameInfo>>::create();
            auto page = QSharedPointer<int>::create(0);
            auto cursor = QSharedPointer<QString>::create(QString());
            auto fetchPage = QSharedPointer<std::function<void()>>::create();
            QWeakPointer<std::function<void()>> weakFetchPage(fetchPage);
            *fetchPage = [this, completion, vpcId, sortString, queryText, collected, page, cursor, weakFetchPage]() {
                const QVariantMap variables{
                    {QStringLiteral("vpcId"), vpcId},
                    {QStringLiteral("locale"), QStringLiteral("en_US")},
                    {QStringLiteral("sortString"), sortString},
                    {QStringLiteral("fetchCount"), kDefaultCatalogFetchCount},
                    {QStringLiteral("cursor"), *cursor},
                    {QStringLiteral("filters"), QVariantMap{}},
                };
                postGraphQlJson(queryText, variables, [this, completion, vpcId, collected, page, cursor, weakFetchPage](const QVariantMap &data, const QString &error) {
                    if (!error.isEmpty()) {
                        completion(false, {}, error);
                        return;
                    }
                    const QVariantMap apps = data.value(QStringLiteral("apps")).toMap();
                    for (const QVariant &item : apps.value(QStringLiteral("items")).toList()) {
                        const GameInfo game = parseGameItem(item.toMap());
                        if (!game.id.isEmpty() && !game.title.isEmpty() && hasVisibleVariants(game)) collected->append(game);
                    }
                    ++(*page);
                    const QVariantMap pageInfo = apps.value(QStringLiteral("pageInfo")).toMap();
                    const bool hasNextPage = pageInfo.value(QStringLiteral("hasNextPage")).toBool();
                    const QString endCursor = pageInfo.value(QStringLiteral("endCursor")).toString();
                    if (hasNextPage && !endCursor.isEmpty() && *page < kMaxCatalogPages) {
                        *cursor = endCursor;
                        if (auto next = weakFetchPage.toStrongRef()) {
                            (*next)();
                            return;
                        }
                    }
                    fetchMetadata(*collected, vpcId, completion);
                });
            };
            (*fetchPage)();
        });
    });
}

void GameService::fetchLibraryGames(LibraryCallback completion) {
    if (m_accessToken.isEmpty()) {
        completion(false, {}, QStringLiteral("No access token"));
        return;
    }

    fetchServerVpcId([this, completion](const QString &vpcId) {
        const QVariantMap variables{
            {QStringLiteral("vpcId"), vpcId},
            {QStringLiteral("locale"), QStringLiteral("en_US")},
            {QStringLiteral("panelNames"), QVariantList{QStringLiteral("LIBRARY")}},
        };

        const auto parsePanels = [this, vpcId, completion](const QVariantMap &data, const QString &error) {
            if (!error.isEmpty()) {
                completion(false, {}, error);
                return;
            }
            QList<GameInfo> games;
            const QVariantList panels = data.value(QStringLiteral("panels")).toList();
            for (const QVariant &panelValue : panels) {
                const QVariantMap panel = panelValue.toMap();
                const QVariantList sections = panel.value(QStringLiteral("sections")).toList();
                for (const QVariant &sectionValue : sections) {
                    const QVariantMap section = sectionValue.toMap();
                    const QVariantList items = section.value(QStringLiteral("items")).toList();
                    for (const QVariant &itemValue : items) {
                        const QVariantMap item = itemValue.toMap();
                        if (item.value(QStringLiteral("__typename")).toString() != QStringLiteral("GameItem")) continue;
                        const GameInfo game = parseGameItem(item.value(QStringLiteral("app")).toMap());
                        if (!game.id.isEmpty() && !game.title.isEmpty() && hasVisibleVariants(game)) games.append(game);
                    }
                }
            }
            fetchMetadata(games, vpcId, completion);
        };

        postGraphQL(QStringLiteral("panels/Library"), QString::fromLatin1(kLibraryWithTimeHash), variables,
                    [this, variables, parsePanels](const QVariantMap &data, const QString &error) {
            if (error.isEmpty()) {
                parsePanels(data, error);
                return;
            }
            postGraphQL(QStringLiteral("panels/Library"), QString::fromLatin1(kPanelsHash), variables, parsePanels);
        });
    });
}

void GameService::fetchMetadata(const QList<GameInfo> &games, const QString &vpcId, LibraryCallback completion) {
    if (games.isEmpty()) {
        completion(true, {}, QString());
        return;
    }
    QStringList uuids;
    QSet<QString> seen;
    for (const GameInfo &game : games) {
        if (!game.uuid.isEmpty() && !seen.contains(game.uuid)) {
            seen.insert(game.uuid);
            uuids.append(game.uuid);
        }
    }
    if (uuids.isEmpty()) {
        QList<CatalogGame> out;
        for (const GameInfo &game : games) out.append(toCatalogGame(game));
        completion(true, out, QString());
        return;
    }

    auto metadataById = QSharedPointer<QHash<QString, GameInfo>>::create();
    auto remaining = QSharedPointer<int>::create((uuids.size() + 39) / 40);
    for (int i = 0; i < uuids.size(); i += 40) {
        const QStringList chunk = uuids.mid(i, 40);
        QVariantList appIds;
        for (const QString &uuid : chunk) appIds.append(uuid);
        const QVariantMap variables{
            {QStringLiteral("vpcId"), vpcId},
            {QStringLiteral("locale"), QStringLiteral("en_US")},
            {QStringLiteral("appIds"), appIds},
        };
        postGraphQL(QStringLiteral("appMetaData"), QString::fromLatin1(kAppMetaDataHash), variables,
                    [this, games, metadataById, remaining, completion](const QVariantMap &data, const QString &) {
            const QVariantMap apps = data.value(QStringLiteral("apps")).toMap();
            const QVariantList items = apps.value(QStringLiteral("items")).toList();
            for (const QVariant &item : items) {
                const GameInfo parsed = parseGameItem(item.toMap());
                if (!parsed.id.isEmpty()) metadataById->insert(parsed.id, parsed);
            }
            --(*remaining);
            if (*remaining > 0) return;

            QHash<QString, GameInfo> byId;
            for (GameInfo game : games) {
                if (metadataById->contains(game.uuid)) {
                    GameInfo merged = metadataById->value(game.uuid);
                    if (merged.imageUrl.isEmpty()) merged.imageUrl = game.imageUrl;
                    if (merged.heroImageUrl.isEmpty()) merged.heroImageUrl = game.heroImageUrl;
                    if (merged.screenshotUrls.isEmpty()) merged.screenshotUrls = game.screenshotUrls;
                    if (merged.description.isEmpty()) merged.description = game.description;
                    if (merged.variants.isEmpty()) merged.variants = game.variants;
                    merged.isInLibrary = game.isInLibrary;
                    game = merged;
                }
                if (game.id.isEmpty() || !hasVisibleVariants(game)) continue;
                if (!byId.contains(game.id)) {
                    byId.insert(game.id, game);
                    continue;
                }
                GameInfo existing = byId.value(game.id);
                QSet<QString> variantIds;
                for (const GameVariant &variant : existing.variants) variantIds.insert(variant.id);
                for (const GameVariant &variant : game.variants) {
                    if (!variantIds.contains(variant.id)) existing.variants.append(variant);
                }
                if (existing.imageUrl.isEmpty()) existing.imageUrl = game.imageUrl;
                if (existing.heroImageUrl.isEmpty()) existing.heroImageUrl = game.heroImageUrl;
                if (existing.description.isEmpty()) existing.description = game.description;
                byId.insert(game.id, existing);
            }
            QList<CatalogGame> out;
            for (const GameInfo &game : byId) out.append(toCatalogGame(game));
            std::sort(out.begin(), out.end(), [](const CatalogGame &lhs, const CatalogGame &rhs) {
                return QString::localeAwareCompare(lhs.title, rhs.title) < 0;
            });
            completion(true, out, QString());
        });
    }
}

GameInfo GameService::parseGameItem(const QVariantMap &app) const {
    GameInfo game;
    if (app.isEmpty()) return game;
    game.id = safeString(app, QStringLiteral("id"));
    game.uuid = game.id;
    game.title = safeString(app, QStringLiteral("title"));
    game.shortName = safeString(app, QStringLiteral("shortName"));
    game.description = firstString(app, {QStringLiteral("description"), QStringLiteral("longDescription"), QStringLiteral("shortDescription"), QStringLiteral("summary")});

    const QVariantMap itemMetadata = app.value(QStringLiteral("itemMetadata")).toMap();
    if (game.description.isEmpty()) game.description = firstString(itemMetadata, {QStringLiteral("description"), QStringLiteral("longDescription"), QStringLiteral("shortDescription"), QStringLiteral("summary")});

    const QVariantMap gfn = app.value(QStringLiteral("gfn")).toMap();
    game.playabilityState = safeString(gfn, QStringLiteral("playabilityState"));
    game.playType = safeString(gfn, QStringLiteral("playType"));

    const QVariantMap images = app.value(QStringLiteral("images")).toMap();
    const QString landscape = firstString(images, {QStringLiteral("TV_BANNER"), QStringLiteral("HERO_IMAGE"), QStringLiteral("WIDE_ART"), QStringLiteral("LANDSCAPE_IMAGE"), QStringLiteral("MARQUEE_IMAGE"), QStringLiteral("KEY_ART")});
    const QString poster = firstString(images, {QStringLiteral("GAME_BOX_ART"), QStringLiteral("BOX_ART"), QStringLiteral("POSTER_IMAGE"), QStringLiteral("POSTER"), QStringLiteral("KEY_ART")});
    if (!landscape.isEmpty()) game.heroImageUrl = optimizeImageUrl(landscape, 1200);
    if (!landscape.isEmpty() || !poster.isEmpty()) game.imageUrl = optimizeImageUrl(!landscape.isEmpty() ? landscape : poster, 900);
    for (const QString &shot : stringValues(images.value(QStringLiteral("SCREENSHOTS")))) game.screenshotUrls.append(optimizeImageUrl(shot, 720));

    QSet<QString> seenStores;
    const QVariantList variants = app.value(QStringLiteral("variants")).toList();
    for (const QVariant &value : variants) {
        const QVariantMap raw = value.toMap();
        GameVariant variant;
        variant.id = safeString(raw, QStringLiteral("id"));
        variant.appStore = safeString(raw, QStringLiteral("appStore"));
        const QVariantMap variantGfn = raw.value(QStringLiteral("gfn")).toMap();
        const QVariantMap library = variantGfn.value(QStringLiteral("library")).toMap();
        variant.serviceStatus = safeString(library, QStringLiteral("status"));
        variant.librarySelected = library.value(QStringLiteral("selected")).toBool();
        if (variant.librarySelected) variant.inLibrary = true;
        if (variant.serviceStatus.isEmpty()) variant.serviceStatus = safeString(variantGfn, QStringLiteral("status"));
        if (!variant.appStore.isEmpty() && variant.appStore != QStringLiteral("UNKNOWN") && variant.appStore != QStringLiteral("NONE") && !seenStores.contains(variant.appStore)) {
            seenStores.insert(variant.appStore);
            game.availableStores.append(variant.appStore);
            game.variants.append(variant);
        }
    }

    QString firstNumericVariant;
    for (const GameVariant &variant : game.variants) {
        if (variant.inLibrary && !variant.serviceStatus.isEmpty()) game.isInLibrary = true;
        bool numeric = false;
        variant.id.toLongLong(&numeric);
        if (numeric) {
            if (variant.librarySelected) game.launchAppId = variant.id;
            else if (firstNumericVariant.isEmpty()) firstNumericVariant = variant.id;
        }
    }
    if (game.launchAppId.isEmpty()) game.launchAppId = firstNumericVariant;

    game.genres = stringValues(app.value(QStringLiteral("genres")));
    game.featureLabels = stringValues(app.value(QStringLiteral("featureLabels")));
    if (game.featureLabels.isEmpty()) game.featureLabels = stringValues(app.value(QStringLiteral("features")));
    return game;
}

CatalogGame GameService::toCatalogGame(const GameInfo &game) const {
    QString store = game.availableStores.isEmpty() ? QStringLiteral("Cloud") : prettyStore(game.availableStores.first());
    QStringList tags;
    for (const QString &genre : game.genres.mid(0, 3)) appendUnique(tags, genre);
    for (const QString &feature : game.featureLabels) {
        if (tags.size() >= 4) break;
        appendUnique(tags, feature);
    }
    if (tags.isEmpty()) tags.append(QStringLiteral("Controller"));
    const QString subtitle = !game.genres.isEmpty() ? game.genres.mid(0, 2).join(QStringLiteral(", ")) : QStringLiteral("Cloud game");
    CatalogGame catalog;
    catalog.title = game.title;
    catalog.store = store;
    catalog.subtitle = subtitle;
    catalog.tags = tags;
    catalog.accent = colorForTitle(game.title);
    catalog.imageUrl = game.imageUrl;
    catalog.heroImageUrl = !game.heroImageUrl.isEmpty() ? game.heroImageUrl : (!game.screenshotUrls.isEmpty() ? game.screenshotUrls.first() : game.imageUrl);
    catalog.description = game.description;
    catalog.launchAppId = !game.launchAppId.isEmpty() ? game.launchAppId : game.id;
    catalog.internalTitle = !game.shortName.isEmpty() ? game.shortName : game.title;
    catalog.selectedStore = game.availableStores.isEmpty() ? QString() : game.availableStores.first();
    return catalog;
}

} // namespace OpnQt
