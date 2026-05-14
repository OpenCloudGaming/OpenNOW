#pragma once

#include <string>
#include <functional>
#include <vector>
#include <Foundation/Foundation.h>
#include "../common/OPNAuthTypes.h"
#include "../common/OPNGameTypes.h"
#include "../streaming/OPNStreamTypes.h"

namespace OPN {

using PanelCallback = std::function<void(bool success, const std::vector<PanelResult> &panels,
                                           const std::string &error)>;
using CatalogCallback = std::function<void(bool success, const std::vector<GameInfo> &games,
                                               const std::string &error)>;
using CatalogBrowseCallback = std::function<void(bool success, const CatalogBrowseResult &result,
                                                 const std::string &error)>;
using LaunchCallback = std::function<void(bool success, const SessionInfo &session,
                                             const std::string &offerSdp,
                                             const std::string &error)>;
using LaunchProgressCallback = std::function<void(const std::string &message, const SessionInfo &session)>;
using SubscriptionCallback = std::function<void(bool success, const SubscriptionInfo &subscription,
                                                const std::string &error)>;

class GameService {
public:
    static GameService &Shared();

    void SetAccessToken(const std::string &token);
    void SetVpcId(const std::string &id);
    void SetUserId(const std::string &id);
    void SetStreamingBaseUrl(const std::string &url);

    void FetchMainPanels(PanelCallback completion);
    void BrowseCatalogGames(const std::string &searchQuery,
                            const std::string &sortId,
                            const std::vector<std::string> &filterIds,
                            int fetchCount,
                            CatalogBrowseCallback completion);
    void FetchCatalogGames(CatalogCallback completion);
    void FetchPublicGames(CatalogCallback completion);
    void FetchLibraryGames(CatalogCallback completion);
    void FetchSubscriptionInfo(const std::string &userId, SubscriptionCallback completion);

    void LaunchGame(const std::string &appId,
                    const std::string &internalTitle,
                    const StreamSettings &settings,
                    bool recoveryMode,
                    LaunchProgressCallback progress,
                    LaunchCallback completion);

    static std::string OptimizeImageURL(const std::string &url, int width = 272);

private:
    GameService();

    void postGraphQL(const std::string &operationName,
                     const std::string &queryHash,
                     NSDictionary *variables,
                     std::function<void(NSDictionary *, NSString *)> completion);

    void postGraphQlJson(const std::string &query,
                         NSDictionary *variables,
                         std::function<void(NSDictionary *, NSString *)> completion);

    GameInfo parseGameItem(NSDictionary *item);
    NSDictionary *baseHeaders();

    std::string m_accessToken;
    std::string m_vpcId;
    std::string m_userId;
    std::string m_graphqlURL;
    std::string m_streamingBaseUrl;
};

}
