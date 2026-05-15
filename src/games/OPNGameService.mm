#include "OPNGameService.h"
#include "streaming/OPNSessionManager.h"
#include "streaming/OPNSignalingClient.h"
#include "streaming/OPNStreamSession.h"
#include <CommonCrypto/CommonCrypto.h>
#include <memory>
#include <unordered_map>
#include <unordered_set>

namespace OPN {


static NSString *kPanelsHash = @"f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
static NSString *kLibraryWithTimeHash = @"039e8c0d553972975485fee56e59f2549d2fdb518e247a42ab5022056a74406f";
static NSString *kAppMetaDataHash = @"cf8b620dfd03617017ba7c858cee65197e1ace5180e41be194b39227227ced63";


static NSString *kNvClientId = @"ec7e38d4-03af-4b58-b131-cfb0495903ab";
static NSString *kNvClientVersion = @"2.0.80.173";
static constexpr const char *kDefaultSubscriptionVpcId = "NP-AMS-08";
static constexpr int kDefaultCatalogFetchCount = 96;
static constexpr int kMaxCatalogPages = 3;
static void GetServerVpcId(const std::string &token,
                           std::function<void(const std::string &vpcId)> completion);

GameService &GameService::Shared() {
    static GameService instance;
    return instance;
}

GameService::GameService() {
    m_graphqlURL = "https://games.geforce.com/graphql";
}

void GameService::SetAccessToken(const std::string &token) {
    m_accessToken = token;
}

void GameService::SetVpcId(const std::string &id) {
    m_vpcId = id;
}

void GameService::SetUserId(const std::string &id) {
    m_userId = id;
}

void GameService::SetStreamingBaseUrl(const std::string &url) {
    m_streamingBaseUrl = url;
    SessionManager::Shared().SetStreamingBaseUrl(url);
}





NSDictionary *GameService::baseHeaders() {
    NSMutableDictionary *h = [NSMutableDictionary dictionary];
    h[@"Origin"] = @"https://play.geforcenow.com";
    h[@"Referer"] = @"https://play.geforcenow.com/";
    h[@"Accept"] = @"application/json, text/plain, */*";
    h[@"nv-client-id"] = kNvClientId;
    h[@"nv-client-type"] = @"NATIVE";
    h[@"nv-client-version"] = kNvClientVersion;
    h[@"nv-client-streamer"] = @"NVIDIA-CLASSIC";
    h[@"nv-device-os"] = @"MACOS";
    h[@"nv-device-type"] = @"DESKTOP";
    h[@"nv-device-make"] = @"UNKNOWN";
    h[@"nv-device-model"] = @"UNKNOWN";
    h[@"nv-browser-type"] = @"CHROME";
    h[@"User-Agent"] = @"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 GFN-PC/2.0.80.173";

    if (!m_accessToken.empty()) {
        h[@"Authorization"] = [NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()];
    }
    return h;
}





void GameService::postGraphQL(const std::string &operationName,
                               const std::string &queryHash,
                               NSDictionary *variables,
                               std::function<void(NSDictionary *, NSString *)> completion) {

    NSString *varStr = @"{}";
    if (variables) {
        NSData *jsonData = [NSJSONSerialization dataWithJSONObject:variables options:0 error:nil];
        varStr = [[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding];
    }

    NSDictionary *extDict = @{
        @"persistedQuery": @{
            @"sha256Hash": [NSString stringWithUTF8String:queryHash.c_str()]
        }
    };
    NSData *extData = [NSJSONSerialization dataWithJSONObject:extDict options:0 error:nil];
    NSString *extStr = [[NSString alloc] initWithData:extData encoding:NSUTF8StringEncoding];


    NSString *huId = [NSString stringWithFormat:@"%lx%@",
        (unsigned long)[[NSDate date] timeIntervalSince1970] * 1000,
        [[[[NSUUID UUID] UUIDString] stringByReplacingOccurrencesOfString:@"-" withString:@""] substringToIndex:8]];


    NSString *encodedRequestType = [[NSString stringWithUTF8String:operationName.c_str()]
        stringByAddingPercentEncodingWithAllowedCharacters:NSCharacterSet.URLQueryAllowedCharacterSet];
    NSString *encodedExtensions = [extStr stringByAddingPercentEncodingWithAllowedCharacters:
        NSCharacterSet.URLQueryAllowedCharacterSet];
    NSString *encodedVariables = [varStr stringByAddingPercentEncodingWithAllowedCharacters:
        NSCharacterSet.URLQueryAllowedCharacterSet];

    NSString *urlStr = [NSString stringWithFormat:@"https://games.geforce.com/graphql?requestType=%@&extensions=%@&huId=%@&variables=%@",
        encodedRequestType, encodedExtensions, huId, encodedVariables];

    NSURL *url = [NSURL URLWithString:urlStr];
    if (!url) {
        completion(nil, @"Invalid URL");
        return;
    }
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:url];
    req.HTTPMethod = @"GET";
    req.timeoutInterval = 20.0;


    NSDictionary *hdrs = baseHeaders();
    NSMutableDictionary *allHeaders = [hdrs mutableCopy];
    allHeaders[@"Content-Type"] = @"application/graphql";
    for (NSString *key in allHeaders) {
        [req setValue:allHeaders[key] forHTTPHeaderField:key];
    }

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (error) {
                    completion(nil, [error localizedDescription]);
                    return;
                }
                NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                if (http.statusCode != 200 || !json) {
                    NSString *msg = [NSString stringWithFormat:@"GraphQL error (%ld)", (long)http.statusCode];
                    completion(json, msg);
                    return;
                }


                NSArray *errors = json[@"errors"];
                if (errors && [errors isKindOfClass:[NSArray class]] && errors.count > 0) {
                    NSDictionary *err = errors[0];
                    NSString *errMsg = err[@"message"];
                    completion(json, errMsg ? errMsg : @"GraphQL error");
                    return;
                }

                NSDictionary *d = json[@"data"];
                if (!d) {
                    completion(json, @"No data in GraphQL response");
                    return;
                }
                completion(d, @"");
            });
        }];
    [task resume];
}





static NSString *SafeStr(id value) {
    if (!value || [value isKindOfClass:[NSNull class]]) return nil;
    if (![value isKindOfClass:[NSString class]]) return nil;
    return (NSString *)value;
}

static NSString *FirstSafeString(NSDictionary *dictionary, NSArray<NSString *> *keys) {
    if (![dictionary isKindOfClass:[NSDictionary class]]) return nil;
    for (NSString *key in keys) {
        NSString *value = SafeStr(dictionary[key]);
        if (value.length > 0) return value;
    }
    return nil;
}

static NSString *FirstLandscapeImageString(NSDictionary *images) {
    return FirstSafeString(images, @[
        @"TV_BANNER",
        @"HERO_IMAGE",
        @"WIDE_ART",
        @"LANDSCAPE_IMAGE",
        @"MARQUEE_IMAGE",
        @"KEY_ART"
    ]);
}

static NSString *FirstPosterImageString(NSDictionary *images) {
    return FirstSafeString(images, @[
        @"GAME_BOX_ART",
        @"BOX_ART",
        @"POSTER_IMAGE",
        @"POSTER",
        @"KEY_ART"
    ]);
}

static double SafeMinutesAsHours(id value) {
    if ([value isKindOfClass:[NSNumber class]]) {
        return [(NSNumber *)value doubleValue] / 60.0;
    }
    if ([value isKindOfClass:[NSString class]]) {
        double minutes = [(NSString *)value doubleValue];
        return minutes / 60.0;
    }
    return 0.0;
}

static int SafeInt(id value) {
    if ([value isKindOfClass:[NSNumber class]]) return [(NSNumber *)value intValue];
    if ([value isKindOfClass:[NSString class]]) return [(NSString *)value intValue];
    return 0;
}

static void AppendUniqueString(std::vector<std::string> &values, NSString *value) {
    if (value.length == 0) return;
    std::string text = [value UTF8String];
    if (std::find(values.begin(), values.end(), text) == values.end()) values.push_back(text);
}

static void AppendStringValues(std::vector<std::string> &values, id rawValue) {
    if ([rawValue isKindOfClass:[NSString class]]) {
        AppendUniqueString(values, rawValue);
        return;
    }
    if (![rawValue isKindOfClass:[NSArray class]]) return;
    for (id item in (NSArray *)rawValue) {
        if ([item isKindOfClass:[NSString class]]) {
            AppendUniqueString(values, item);
        } else if ([item isKindOfClass:[NSDictionary class]]) {
            NSDictionary *dictionary = (NSDictionary *)item;
            NSString *text = FirstSafeString(dictionary, @[@"name", @"label", @"value", @"rating", @"control", @"type"]);
            AppendUniqueString(values, text);
        }
    }
}

static bool HasVisibleVariants(const GameInfo &game) {
    return !game.variants.empty();
}

GameInfo GameService::parseGameItem(NSDictionary *app) {
    GameInfo g;
    if (!app) return g;

    { NSString *v = SafeStr(app[@"id"]); g.id = v ? [v UTF8String] : ""; g.uuid = g.id; }
    { NSString *v = SafeStr(app[@"title"]); g.title = v ? [v UTF8String] : ""; }
    { NSString *v = SafeStr(app[@"shortName"]); g.shortName = v ? [v UTF8String] : ""; }
    { NSString *v = FirstSafeString(app, @[@"description", @"longDescription", @"shortDescription", @"summary"]); g.description = v ? [v UTF8String] : ""; }
    { NSString *v = SafeStr(app[@"developerName"]); g.developerName = v ? [v UTF8String] : ""; }
    { NSString *v = SafeStr(app[@"publisherName"]); g.publisherName = v ? [v UTF8String] : ""; }
    g.maxLocalPlayers = SafeInt(app[@"maxLocalPlayers"]);
    g.maxOnlinePlayers = SafeInt(app[@"maxOnlinePlayers"]);
    AppendStringValues(g.supportedControls, app[@"supportedControls"]);
    AppendStringValues(g.contentRatings, app[@"contentRatings"]);
    AppendStringValues(g.nvidiaTech, app[@"nvidiaTech"]);

    NSDictionary *itemMetadata = app[@"itemMetadata"];
    if (g.description.empty() && [itemMetadata isKindOfClass:[NSDictionary class]]) {
        NSString *v = FirstSafeString(itemMetadata, @[@"description", @"longDescription", @"shortDescription", @"summary"]);
        if (v) g.description = [v UTF8String];
    }
    NSDictionary *serviceMeta = app[@"gfn"];
    if (serviceMeta && [serviceMeta isKindOfClass:[NSDictionary class]]) {
        { NSString *v = SafeStr(serviceMeta[@"playabilityState"]); g.playabilityState = v ? [v UTF8String] : ""; }
        { NSString *v = SafeStr(serviceMeta[@"minimumMembershipTierLabel"]); g.membershipTierLabel = v ? [v UTF8String] : ""; }
        { NSString *v = SafeStr(serviceMeta[@"playType"]); g.playType = v ? [v UTF8String] : ""; }
    }

    NSDictionary *images = app[@"images"];
    if (images && [images isKindOfClass:[NSDictionary class]]) {
        NSString *landscape = FirstLandscapeImageString(images);
        NSString *poster = FirstPosterImageString(images);
        NSString *primary = landscape ?: poster;
        if (landscape) {
            g.heroImageUrl = OptimizeImageURL([landscape UTF8String], 1200);
        }
        if (primary) {
            g.imageUrl = OptimizeImageURL([primary UTF8String], 900);
        }
        id screenshots = images[@"SCREENSHOTS"];
        if ([screenshots isKindOfClass:[NSArray class]]) {
            for (id screenshot in (NSArray *)screenshots) {
                NSString *url = SafeStr(screenshot);
                if (url.length == 0) continue;
                std::string optimizedUrl = OptimizeImageURL([url UTF8String], 720);
                if (std::find(g.screenshotUrls.begin(), g.screenshotUrls.end(), optimizedUrl) == g.screenshotUrls.end()) {
                    g.screenshotUrls.push_back(optimizedUrl);
                }
            }
        } else if ([screenshots isKindOfClass:[NSString class]]) {
            NSString *url = SafeStr(screenshots);
            if (url.length > 0) g.screenshotUrls.push_back(OptimizeImageURL([url UTF8String], 720));
        }
    }

    NSArray *variants = app[@"variants"];
    if (variants && [variants isKindOfClass:[NSArray class]]) {
        std::unordered_set<std::string> seenStores;
        for (NSDictionary *v in variants) {
            if (![v isKindOfClass:[NSDictionary class]]) continue;
            GameVariant gv;
            { NSString *s = SafeStr(v[@"id"]); gv.id = s ? [s UTF8String] : ""; }
            { NSString *s = SafeStr(v[@"appStore"]); gv.appStore = s ? [s UTF8String] : ""; }

            NSDictionary *variantService = v[@"gfn"];
            if (variantService && [variantService isKindOfClass:[NSDictionary class]]) {
                NSDictionary *lib = variantService[@"library"];
                if (lib && [lib isKindOfClass:[NSDictionary class]]) {
                    { NSString *s = SafeStr(lib[@"status"]); gv.serviceStatus = s ? [s UTF8String] : ""; }
                    NSNumber *sel = lib[@"selected"];
                    gv.librarySelected = [sel isKindOfClass:[NSNumber class]] ? [sel boolValue] : false;
                    if (gv.librarySelected) gv.inLibrary = true;
                }
                { NSString *s = SafeStr(variantService[@"status"]); if (gv.serviceStatus.empty() && s) gv.serviceStatus = [s UTF8String]; }
            }

            if (!gv.appStore.empty() && gv.appStore != "UNKNOWN" && gv.appStore != "NONE") {
                if (seenStores.find(gv.appStore) == seenStores.end()) {
                    seenStores.insert(gv.appStore);
                    g.availableStores.push_back(gv.appStore);
                    g.variants.push_back(gv);
                }
            }
        }
    }


    {
        std::string firstNumericVariant;
        for (auto &v : g.variants) {
            if (v.inLibrary && !v.serviceStatus.empty()) {
                g.isInLibrary = true;
            }
            bool isNumeric = !v.id.empty() && v.id.find_first_not_of("0123456789") == std::string::npos;
            if (isNumeric) {
                if (v.librarySelected) {
                    g.launchAppId = v.id;
                } else if (firstNumericVariant.empty()) {
                    firstNumericVariant = v.id;
                }
            }
        }
        if (g.launchAppId.empty()) {
            g.launchAppId = firstNumericVariant;
        }
    }


    NSArray *genres = app[@"genres"];
    if (genres && [genres isKindOfClass:[NSArray class]]) {
        for (id item in genres) {
            if ([item isKindOfClass:[NSString class]]) {
                AppendUniqueString(g.genres, item);
            } else if ([item isKindOfClass:[NSDictionary class]]) {
                NSString *name = SafeStr(((NSDictionary *)item)[@"name"]);
                AppendUniqueString(g.genres, name);
            }
        }
    }


    NSArray *features = app[@"featureLabels"] ? app[@"featureLabels"] : app[@"features"];
    if (features && [features isKindOfClass:[NSArray class]]) {
        for (id item in features) {
            if ([item isKindOfClass:[NSString class]]) {
                AppendUniqueString(g.featureLabels, item);
            }
        }
    }

    return g;
}





void GameService::postGraphQlJson(const std::string &query,
                                   NSDictionary *variables,
                                   std::function<void(NSDictionary *, NSString *)> completion) {
    NSMutableDictionary *body = [NSMutableDictionary dictionary];
    body[@"query"] = [NSString stringWithUTF8String:query.c_str()];
    if (variables) {
        body[@"variables"] = variables;
    }

    NSData *jsonBody = [NSJSONSerialization dataWithJSONObject:body options:0 error:nil];
    NSURL *url = [NSURL URLWithString:[NSString stringWithUTF8String:m_graphqlURL.c_str()]];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:url];
    req.HTTPMethod = @"POST";
    req.timeoutInterval = 20.0;
    [req setHTTPBody:jsonBody];

    NSDictionary *hdrs = baseHeaders();
    for (NSString *key in hdrs) {
        [req setValue:hdrs[key] forHTTPHeaderField:key];
    }
    [req setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (error) {
                    completion(nil, [error localizedDescription]);
                    return;
                }
                NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                if (http.statusCode != 200 || !json) {
                    NSString *msg = [NSString stringWithFormat:@"GraphQL error (%ld)", (long)http.statusCode];
                    completion(json, msg);
                    return;
                }

                NSArray *errors = json[@"errors"];
                if (errors && [errors isKindOfClass:[NSArray class]] && errors.count > 0) {
                    NSDictionary *err = errors[0];
                    NSString *errMsg = err[@"message"];
                    completion(json, errMsg ? errMsg : @"GraphQL error");
                    return;
                }

                NSDictionary *d = json[@"data"];
                if (!d) {
                    completion(json, @"No data in GraphQL response");
                    return;
                }
                completion(d, @"");
            });
        }];
    [task resume];
}





void GameService::BrowseCatalogGames(const std::string &searchQuery,
                                     const std::string &sortId,
                                     const std::vector<std::string> &filterIds,
                                     int fetchCount,
                                     CatalogBrowseCallback completion) {
    std::string token = m_accessToken;
    CatalogBrowseCallback callback = completion;

    GetServerVpcId(token, [this, callback, searchQuery, sortId, filterIds, fetchCount](const std::string &vpcId) {
        NSString *vpcIdObj = [NSString stringWithUTF8String:vpcId.c_str()];
        std::string requestedSearch = searchQuery;
        std::string requestedSortId = sortId.empty() ? "last_played" : sortId;
        std::vector<std::string> requestedFilterIds = filterIds;
        int requestedFetchCount = std::max(24, std::min(fetchCount > 0 ? fetchCount : kDefaultCatalogFetchCount, 200));

        std::string definitionsQuery = R"(
        query GetFilterGroupAndSortOrderDefinitions($locale: String!) {
            filterGroupDefinitions(language: $locale) {
                id
                label
                filters { id label filters }
            }
            sortOrderDefinitions(language: $locale) { id label orderBy }
        }
        )";

        postGraphQlJson(definitionsQuery, @{@"locale": @"en_US"},
            [this, callback, vpcIdObj, requestedSearch, requestedSortId, requestedFilterIds, requestedFetchCount](NSDictionary *definitionsData, NSString *definitionsError) {
                if (definitionsError.length > 0) {
                    callback(false, CatalogBrowseResult{}, [definitionsError UTF8String]);
                    return;
                }

                CatalogBrowseResult result;
                NSMutableDictionary<NSString *, NSDictionary *> *filterPayloadById = [NSMutableDictionary dictionary];
                NSArray *filterGroupsRaw = definitionsData[@"filterGroupDefinitions"];
                if ([filterGroupsRaw isKindOfClass:[NSArray class]]) {
                    for (NSDictionary *groupRaw in filterGroupsRaw) {
                        if (![groupRaw isKindOfClass:[NSDictionary class]]) continue;
                        CatalogFilterGroup group;
                        NSString *groupId = SafeStr(groupRaw[@"id"]);
                        NSString *groupLabel = SafeStr(groupRaw[@"label"]);
                        group.id = groupId ? [groupId UTF8String] : "";
                        group.label = groupLabel ? [groupLabel UTF8String] : "";
                        NSArray *filters = groupRaw[@"filters"];
                        if ([filters isKindOfClass:[NSArray class]]) {
                            for (NSDictionary *entry in filters) {
                                if (![entry isKindOfClass:[NSDictionary class]]) continue;
                                NSArray *payloads = entry[@"filters"];
                                NSString *payloadString = [payloads isKindOfClass:[NSArray class]] && payloads.count > 0 ? SafeStr(payloads[0]) : nil;
                                NSString *filterId = SafeStr(entry[@"id"]);
                                NSString *filterLabel = SafeStr(entry[@"label"]);
                                if (filterId.length == 0 || payloadString.length == 0) continue;
                                NSData *payloadData = [payloadString dataUsingEncoding:NSUTF8StringEncoding];
                                NSDictionary *payload = [NSJSONSerialization JSONObjectWithData:payloadData options:0 error:nil];
                                if (![payload isKindOfClass:[NSDictionary class]]) continue;
                                filterPayloadById[filterId] = payload;

                                CatalogFilterOption option;
                                option.id = [filterId UTF8String];
                                option.rawId = option.id;
                                option.label = filterLabel ? [filterLabel UTF8String] : option.id;
                                option.groupId = group.id;
                                option.groupLabel = group.label;
                                group.options.push_back(option);
                            }
                        }
                        if (!group.options.empty()) result.filterGroups.push_back(group);
                    }
                }

                NSArray *sortsRaw = definitionsData[@"sortOrderDefinitions"];
                if ([sortsRaw isKindOfClass:[NSArray class]]) {
                    for (NSDictionary *sortRaw in sortsRaw) {
                        if (![sortRaw isKindOfClass:[NSDictionary class]]) continue;
                        NSString *sid = SafeStr(sortRaw[@"id"]);
                        NSString *label = SafeStr(sortRaw[@"label"]);
                        NSString *orderBy = SafeStr(sortRaw[@"orderBy"]);
                        if (sid.length == 0 || orderBy.length == 0) continue;
                        CatalogSortOption option;
                        option.id = [sid UTF8String];
                        option.label = label ? [label UTF8String] : option.id;
                        option.orderBy = [orderBy UTF8String];
                        result.sortOptions.push_back(option);
                    }
                }

                CatalogSortOption selectedSort;
                selectedSort.id = "relevance";
                selectedSort.label = "Relevance";
                selectedSort.orderBy = "itemMetadata.relevance:DESC,sortName:ASC";
                for (const CatalogSortOption &option : result.sortOptions) {
                    if (option.id == requestedSortId) { selectedSort = option; break; }
                }

                NSMutableDictionary *filters = [NSMutableDictionary dictionary];
                for (const std::string &filterId : requestedFilterIds) {
                    NSString *key = [NSString stringWithUTF8String:filterId.c_str()];
                    NSDictionary *payload = filterPayloadById[key];
                    if (!payload) continue;
                    [filters addEntriesFromDictionary:payload];
                    result.selectedFilterIds.push_back(filterId);
                }

                NSString *trimmedSearch = [[NSString stringWithUTF8String:requestedSearch.c_str()]
                    stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
                result.searchQuery = trimmedSearch.length > 0 ? [trimmedSearch UTF8String] : "";
                result.selectedSortId = selectedSort.id;

        std::string query = R"(
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
                    variants {
                        id
                        appStore
                        supportedControls
                        gfn { status library { status selected } }
                    }
                    gfn { playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG } }
                    itemMetadata { campaignIds }
                }
            }
        }
        )";

                std::string searchQueryText = R"(
        query GetSearchFilterResults(
            $vpcId: String!,
            $locale: String!,
            $sortString: String!,
            $fetchCount: Int!,
            $cursor: String!,
            $searchString: String!,
            $filters: AppFilterFields!
        ) {
            apps(
                vpcId: $vpcId,
                language: $locale,
                orderBy: $sortString,
                first: $fetchCount,
                after: $cursor,
                searchQuery: $searchString,
                filters: $filters
            ) {
                numberReturned
                numberSupported
                pageInfo { hasNextPage endCursor totalCount }
                items {
                    id
                    title
                    images { KEY_ART GAME_BOX_ART TV_BANNER HERO_IMAGE SCREENSHOTS }
                    variants {
                        id
                        appStore
                        supportedControls
                        gfn { status library { status selected } }
                    }
                    gfn { playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG } }
                    itemMetadata { campaignIds }
                }
            }
        }
        )";

                std::string selectedQuery = result.searchQuery.empty() ? query : searchQueryText;
                NSString *sortString = [NSString stringWithUTF8String:selectedSort.orderBy.c_str()];
                NSMutableArray<NSDictionary *> *collectedApps = [NSMutableArray array];
                auto blockResult = std::make_shared<CatalogBrowseResult>(result);
                auto page = std::make_shared<NSInteger>(0);
                auto cursor = std::make_shared<NSString *>(@"");
                GameService *service = this;
                auto fetchPage = std::make_shared<std::function<void(void)>>();
                std::weak_ptr<std::function<void(void)>> weakFetchPage = fetchPage;

                *fetchPage = [=] {
                    NSMutableDictionary *vars = [@{
                        @"vpcId": vpcIdObj,
                        @"locale": @"en_US",
                        @"sortString": sortString,
                        @"fetchCount": @(requestedFetchCount),
                        @"cursor": *cursor ?: @"",
                        @"filters": filters,
                    } mutableCopy];
                    if (trimmedSearch.length > 0) vars[@"searchString"] = trimmedSearch;

                    auto keepPaginationAlive = weakFetchPage.lock();
                    service->postGraphQlJson(selectedQuery, vars, ^(NSDictionary *data, NSString *error) {
                        (void)keepPaginationAlive;
                        if (error.length > 0) {
                            callback(false, CatalogBrowseResult{}, [error UTF8String]);
                            return;
                        }
                        NSDictionary *appsDict = data[@"apps"];
                        if (![appsDict isKindOfClass:[NSDictionary class]]) {
                            callback(false, CatalogBrowseResult{}, "No apps data");
                            return;
                        }
                        NSArray *items = appsDict[@"items"];
                        if ([items isKindOfClass:[NSArray class]]) [collectedApps addObjectsFromArray:items];
                        blockResult->numberReturned += SafeInt(appsDict[@"numberReturned"]);
                        blockResult->numberSupported = SafeInt(appsDict[@"numberSupported"]);
                        NSDictionary *pageInfo = appsDict[@"pageInfo"];
                        BOOL hasNextPage = [pageInfo isKindOfClass:[NSDictionary class]] && [pageInfo[@"hasNextPage"] boolValue];
                        NSString *endCursor = [pageInfo isKindOfClass:[NSDictionary class]] ? SafeStr(pageInfo[@"endCursor"]) : nil;
                        blockResult->totalCount = [pageInfo isKindOfClass:[NSDictionary class]] ? SafeInt(pageInfo[@"totalCount"]) : blockResult->totalCount;
                        blockResult->hasNextPage = hasNextPage;
                        if (endCursor.length > 0) blockResult->endCursor = [endCursor UTF8String];

                        (*page)++;
                        if (hasNextPage && endCursor.length > 0 && *page < kMaxCatalogPages) {
                            *cursor = endCursor;
                            if (auto nextPage = weakFetchPage.lock()) {
                                (*nextPage)();
                                return;
                            }
                        }

                        NSMutableArray<NSString *> *appIdsNeedingMetadata = [NSMutableArray array];
                        NSMutableSet<NSString *> *seenMetadataIds = [NSMutableSet set];
                        for (NSDictionary *app in collectedApps) {
                            if (![app isKindOfClass:[NSDictionary class]]) continue;
                            GameInfo g = service->parseGameItem(app);
                            if (!g.id.empty() && !g.title.empty() && HasVisibleVariants(g)) {
                                if (!g.uuid.empty()) {
                                    NSString *uuid = [NSString stringWithUTF8String:g.uuid.c_str()];
                                    if (![seenMetadataIds containsObject:uuid]) {
                                        [seenMetadataIds addObject:uuid];
                                        [appIdsNeedingMetadata addObject:uuid];
                                    }
                                }
                                blockResult->games.push_back(g);
                            }
                        }
                        blockResult->numberSupported = std::max(blockResult->numberSupported, (int)blockResult->games.size());
                        blockResult->totalCount = std::max(blockResult->totalCount, (int)blockResult->games.size());
                        if (appIdsNeedingMetadata.count == 0) {
                            callback(true, *blockResult, "");
                            return;
                        }

                        NSMutableDictionary<NSString *, NSDictionary *> *metadataById = [NSMutableDictionary dictionary];
                        NSUInteger chunkSize = 40;
                        NSUInteger totalChunks = (appIdsNeedingMetadata.count + chunkSize - 1) / chunkSize;
                        __block NSUInteger completedChunks = 0;
                        auto mergeAndFinish = ^{
                                for (OPN::GameInfo &game : blockResult->games) {
                                    if (game.uuid.empty()) continue;
                                    NSString *uuid = [NSString stringWithUTF8String:game.uuid.c_str()];
                                    NSDictionary *metadataApp = metadataById[uuid];
                                    if (!metadataApp) continue;
                                    GameInfo metadataGame = service->parseGameItem(metadataApp);
                                    if (!metadataGame.description.empty()) {
                                        game.description = metadataGame.description;
                                    }
                                    if (game.genres.empty() && !metadataGame.genres.empty()) {
                                        game.genres = metadataGame.genres;
                                    }
                                    if (game.featureLabels.empty() && !metadataGame.featureLabels.empty()) {
                                        game.featureLabels = metadataGame.featureLabels;
                                    }
                                    if (game.developerName.empty()) game.developerName = metadataGame.developerName;
                                    if (game.publisherName.empty()) game.publisherName = metadataGame.publisherName;
                                    if (!metadataGame.screenshotUrls.empty()) game.screenshotUrls = metadataGame.screenshotUrls;
                                    if (game.maxLocalPlayers <= 0) game.maxLocalPlayers = metadataGame.maxLocalPlayers;
                                    if (game.maxOnlinePlayers <= 0) game.maxOnlinePlayers = metadataGame.maxOnlinePlayers;
                                    if (game.supportedControls.empty()) game.supportedControls = metadataGame.supportedControls;
                                    if (game.contentRatings.empty()) game.contentRatings = metadataGame.contentRatings;
                                    if (game.nvidiaTech.empty()) game.nvidiaTech = metadataGame.nvidiaTech;
                                }
                                callback(true, *blockResult, "");
                        };

                        for (NSUInteger start = 0; start < appIdsNeedingMetadata.count; start += chunkSize) {
                            NSUInteger length = MIN(chunkSize, appIdsNeedingMetadata.count - start);
                            NSArray<NSString *> *chunk = [appIdsNeedingMetadata subarrayWithRange:NSMakeRange(start, length)];
                            NSDictionary *metaVars = @{
                                @"vpcId": vpcIdObj,
                                @"locale": @"en_US",
                                @"appIds": chunk,
                            };
                            service->postGraphQL("appMetaData", [kAppMetaDataHash UTF8String], metaVars,
                                ^(NSDictionary *metaData, NSString *metaError) {
                                    if (metaError.length > 0) {
                                        NSLog(@"[GameService] appMetaData enrichment failed: %@", metaError);
                                    }
                                    NSDictionary *apps = metaData[@"apps"];
                                    NSArray *metadataItems = [apps isKindOfClass:[NSDictionary class]] ? apps[@"items"] : nil;
                                    if ([metadataItems isKindOfClass:[NSArray class]]) {
                                        for (NSDictionary *metadataApp in metadataItems) {
                                            if (![metadataApp isKindOfClass:[NSDictionary class]]) continue;
                                            NSString *appId = SafeStr(metadataApp[@"id"]);
                                            if (appId.length > 0) metadataById[appId] = metadataApp;
                                        }
                                    }
                                    completedChunks++;
                                    if (completedChunks >= totalChunks) mergeAndFinish();
                                });
                        }
                    });
                };
                (*fetchPage)();
            });
    });
}

void GameService::FetchCatalogGames(CatalogCallback completion) {
    BrowseCatalogGames("", "relevance", {}, 200,
        [completion](bool success, const CatalogBrowseResult &result, const std::string &error) {
            completion(success, result.games, error);
        });
}





void GameService::FetchPublicGames(CatalogCallback completion) {
    NSURL *url = [NSURL URLWithString:
        @"https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json"];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:url];
    req.timeoutInterval = 20.0;
            [req setValue:@"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"
        forHTTPHeaderField:@"User-Agent"];

    NSURLSessionDataTask *task = [[NSURLSession sharedSession]
        dataTaskWithRequest:req
        completionHandler:^(NSData *data, NSURLResponse *, NSError *error) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (error) {
                    completion(false, {}, [[error localizedDescription] UTF8String]);
                    return;
                }
                NSArray *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                if (![json isKindOfClass:[NSArray class]]) {
                    completion(false, {}, "Invalid public games JSON");
                    return;
                }

                std::vector<GameInfo> games;
                for (id item in json) {
                    if (![item isKindOfClass:[NSDictionary class]]) continue;
                    NSString *status = item[@"status"];
                    if (![status isEqualToString:@"AVAILABLE"]) continue;
                    NSString *title = item[@"title"];
                    if (![title isKindOfClass:[NSString class]] || title.length == 0) continue;

                    GameInfo g;
                    g.title = [title UTF8String];
                    { NSString *v = FirstSafeString(item, @[@"description", @"longDescription", @"shortDescription", @"summary"]); g.description = v ? [v UTF8String] : ""; }
                    id rawId = item[@"id"];
                    NSString *sid = [rawId isKindOfClass:[NSNumber class]]
                        ? [(NSNumber *)rawId stringValue]
                        : ([rawId isKindOfClass:[NSString class]] ? (NSString *)rawId : title);

                    NSString *steamAppId = nil;
                    NSString *steamUrl = item[@"steamUrl"];
                    if ([steamUrl isKindOfClass:[NSString class]]) {
                        NSArray *parts = [steamUrl componentsSeparatedByString:@"/app/"];
                        if (parts.count >= 2) {
                            steamAppId = [parts[1] componentsSeparatedByString:@"/"][0];
                        }
                    }
                    NSString *finalAppId = (steamAppId && steamAppId.length > 0)
                        ? steamAppId
                        : ([sid length] > 0 ? sid : title);
                    g.id = [finalAppId UTF8String];
                    g.uuid = [sid UTF8String];
                    if ([steamUrl isKindOfClass:[NSString class]]) {
                        NSString *steamAppId = nil;
                        NSArray *parts = [steamUrl componentsSeparatedByString:@"/app/"];
                        if (parts.count >= 2) {
                            steamAppId = [parts[1] componentsSeparatedByString:@"/"][0];
                        }
                        if (steamAppId && steamAppId.length > 0) {
                            NSString *heroUrl = [NSString stringWithFormat:
                                @"https://cdn.cloudflare.steamstatic.com/steam/apps/%@/library_hero.jpg",
                                steamAppId];
                            NSString *imgUrl = [NSString stringWithFormat:
                                @"https://cdn.cloudflare.steamstatic.com/steam/apps/%@/header.jpg",
                                steamAppId];
                            g.heroImageUrl = [heroUrl UTF8String];
                            g.imageUrl = [imgUrl UTF8String];
                        }
                    }

                    games.push_back(g);
                }
                completion(true, games, "");
            });
        }];
    [task resume];
}

void GameService::FetchSubscriptionInfo(const std::string &userId, SubscriptionCallback completion) {
    if (m_accessToken.empty()) {
        completion(false, SubscriptionInfo{}, "No access token");
        return;
    }
    if (userId.empty()) {
        completion(false, SubscriptionInfo{}, "No user ID");
        return;
    }

    std::string token = m_accessToken;
    SubscriptionCallback callback = completion;
    GetServerVpcId(token, [token, userId, callback](const std::string &vpcId) {
        const std::string subscriptionVpcId = vpcId == "GFN-PC" ? kDefaultSubscriptionVpcId : vpcId;
        NSURLComponents *components = [NSURLComponents componentsWithString:@"https://mes.geforcenow.com/v4/subscriptions"];
        components.queryItems = @[
            [NSURLQueryItem queryItemWithName:@"serviceName" value:@"gfn_pc"],
            [NSURLQueryItem queryItemWithName:@"languageCode" value:@"en_US"],
            [NSURLQueryItem queryItemWithName:@"vpcId" value:[NSString stringWithUTF8String:subscriptionVpcId.c_str()]],
            [NSURLQueryItem queryItemWithName:@"userId" value:[NSString stringWithUTF8String:userId.c_str()]],
        ];
        NSURL *url = components.URL;
        if (!url) {
            callback(false, SubscriptionInfo{}, "Invalid subscription URL");
            return;
        }

        NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:url];
        req.HTTPMethod = @"GET";
        req.timeoutInterval = 20.0;
        [req setValue:[NSString stringWithFormat:@"GFNJWT %s", token.c_str()] forHTTPHeaderField:@"Authorization"];
        [req setValue:@"application/json" forHTTPHeaderField:@"Accept"];
        [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
        [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
        [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
        [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
        [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
        [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];

        NSURLSessionDataTask *task = [[NSURLSession sharedSession]
            dataTaskWithRequest:req
            completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    if (error) {
                        callback(false, SubscriptionInfo{}, [[error localizedDescription] UTF8String]);
                        return;
                    }
                    NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
                    NSDictionary *json = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
                    if (http.statusCode != 200 || ![json isKindOfClass:[NSDictionary class]]) {
                        NSString *message = [NSString stringWithFormat:@"Subscription API failed (%ld)", (long)http.statusCode];
                        callback(false, SubscriptionInfo{}, [message UTF8String]);
                        return;
                    }

                    SubscriptionInfo info;
                    NSString *tier = SafeStr(json[@"membershipTier"]);
                    info.membershipTier = tier.length > 0 ? [tier UTF8String] : "Free";
                    NSString *type = SafeStr(json[@"type"]);
                    if (type.length > 0) info.subscriptionType = [type UTF8String];
                    NSString *subType = SafeStr(json[@"subType"]);
                    if (subType.length > 0) info.subscriptionSubType = [subType UTF8String];
                    info.allottedHours = SafeMinutesAsHours(json[@"allottedTimeInMinutes"]);
                    info.purchasedHours = SafeMinutesAsHours(json[@"purchasedTimeInMinutes"]);
                    info.rolledOverHours = SafeMinutesAsHours(json[@"rolledOverTimeInMinutes"]);
                    double fallbackTotal = info.allottedHours + info.purchasedHours + info.rolledOverHours;
                    info.totalHours = SafeMinutesAsHours(json[@"totalTimeInMinutes"]);
                    if (info.totalHours <= 0) info.totalHours = fallbackTotal;
                    info.remainingHours = SafeMinutesAsHours(json[@"remainingTimeInMinutes"]);
                    info.usedHours = std::max(0.0, info.totalHours - info.remainingHours);
                    info.isUnlimited = info.subscriptionSubType == "UNLIMITED";
                    NSDictionary *state = [json[@"currentSubscriptionState"] isKindOfClass:[NSDictionary class]]
                        ? json[@"currentSubscriptionState"]
                        : nil;
                    NSNumber *allowed = [state[@"isGamePlayAllowed"] isKindOfClass:[NSNumber class]] ? state[@"isGamePlayAllowed"] : nil;
                    info.isGamePlayAllowed = allowed ? allowed.boolValue : true;
                    callback(true, info, "");
                });
            }];
        [task resume];
    });
}





static void GetServerVpcId(const std::string &token,
                            std::function<void(const std::string &vpcId)> completion) {
    NSURL *url = [NSURL URLWithString:@"https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo"];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:url];
    [req setValue:@"application/json" forHTTPHeaderField:@"Accept"];
    [req setValue:[NSString stringWithFormat:@"GFNJWT %s", token.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
    [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
    [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
    [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
    [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
    [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
    [req setValue:@"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173" forHTTPHeaderField:@"User-Agent"];

    [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        if (error || !data) {
            completion("GFN-PC");
            return;
        }
        NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
        if (http.statusCode != 200) {
            completion("GFN-PC");
            return;
        }
        NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        NSString *serverId = json[@"requestStatus"][@"serverId"];
        if (![serverId isKindOfClass:[NSString class]] || serverId.length == 0) {
            completion("GFN-PC");
            return;
        }
        completion([serverId UTF8String]);
    }] resume];
}





void GameService::FetchLibraryGames(CatalogCallback completion) {
    std::string token = m_accessToken;
    CatalogCallback callback = completion;

    GetServerVpcId(token, [this, callback, token](const std::string &vpcId) {
        NSString *vpcIdObj = [NSString stringWithUTF8String:vpcId.c_str()];
        NSDictionary *vars = @{
            @"vpcId": vpcIdObj,
            @"locale": @"en_US",
            @"panelNames": @[@"LIBRARY"]
        };

        auto flattenAndEnrich = ^(NSDictionary *data, NSString *error) {
            if (error.length > 0) {
                callback(false, {}, [error UTF8String]);
                return;
            }

            NSArray *rawPanels = data[@"panels"];
            if (![rawPanels isKindOfClass:[NSArray class]]) {
                callback(false, {}, "No panels in library response");
                return;
            }

            std::vector<GameInfo> games;
            for (NSDictionary *p in rawPanels) {
                if (![p isKindOfClass:[NSDictionary class]]) continue;
                NSArray *sections = p[@"sections"];
                if (![sections isKindOfClass:[NSArray class]]) continue;
                for (NSDictionary *sec in sections) {
                    if (![sec isKindOfClass:[NSDictionary class]]) continue;
                    NSArray *items = sec[@"items"];
                    if (![items isKindOfClass:[NSArray class]]) continue;
                    for (NSDictionary *item in items) {
                        if (![item isKindOfClass:[NSDictionary class]]) continue;
                        if (![item[@"__typename"] isEqualToString:@"GameItem"]) continue;
                        NSDictionary *app = item[@"app"];
                        if (!app) continue;
                        GameInfo g = parseGameItem(app);
                        if (!g.id.empty() && !g.title.empty() && HasVisibleVariants(g)) {
                            games.push_back(g);
                        }
                    }
                }
            }

            if (games.empty()) {
                callback(true, games, "");
                return;
            }


            NSMutableArray<NSString *> *uuids = [NSMutableArray array];
            NSMutableSet *seen = [NSMutableSet set];
            for (auto &g : games) {
                if (!g.uuid.empty()) {
                    NSString *uuidStr = [NSString stringWithUTF8String:g.uuid.c_str()];
                    if (![seen containsObject:uuidStr]) {
                        [seen addObject:uuidStr];
                        [uuids addObject:uuidStr];
                    }
                }
            }

            if (uuids.count == 0) {
                callback(true, games, "");
                return;
            }

            NSMutableDictionary *appById = [NSMutableDictionary dictionary];
            NSUInteger chunkSize = 40;
            __block NSUInteger chunksCompleted = 0;
            NSUInteger totalChunks = (uuids.count + chunkSize - 1) / chunkSize;

            for (NSUInteger i = 0; i < uuids.count; i += chunkSize) {
                NSUInteger end = MIN(i + chunkSize, uuids.count);
                NSArray *chunk = [uuids subarrayWithRange:NSMakeRange(i, end - i)];
                NSDictionary *metaVars = @{
                    @"vpcId": vpcIdObj,
                    @"locale": @"en_US",
                    @"appIds": chunk
                };

                postGraphQL("appMetaData", [kAppMetaDataHash UTF8String], metaVars,
                    ^(NSDictionary *metaData, NSString *metaError) {
                        if (metaError.length == 0) {
                            NSDictionary *appsDict = metaData[@"apps"];
                            NSArray *items = [appsDict isKindOfClass:[NSDictionary class]] ? appsDict[@"items"] : nil;
                            if ([items isKindOfClass:[NSArray class]]) {
                                for (NSDictionary *app in items) {
                                    if (![app isKindOfClass:[NSDictionary class]]) continue;
                                    NSString *aid = SafeStr(app[@"id"]);
                                    if (aid) appById[aid] = app;
                                }
                            }
                        }
                        chunksCompleted++;
                        if (chunksCompleted >= totalChunks) {

                            std::vector<GameInfo> enriched;
                            for (auto &g : games) {
                                NSString *uuidStr = [NSString stringWithUTF8String:g.uuid.c_str()];
                                NSDictionary *meta = appById[uuidStr];
                                if (meta) {
                                    GameInfo merged = parseGameItem(meta);
                                    if (merged.imageUrl.empty() && !g.imageUrl.empty())
                                        merged.imageUrl = g.imageUrl;
                                    if (merged.heroImageUrl.empty() && !g.heroImageUrl.empty())
                                        merged.heroImageUrl = g.heroImageUrl;
                                    if (merged.screenshotUrls.empty() && !g.screenshotUrls.empty())
                                        merged.screenshotUrls = g.screenshotUrls;
                                    if (merged.description.empty() && !g.description.empty())
                                        merged.description = g.description;
                                    if (merged.variants.empty())
                                        merged.variants = g.variants;
                                    merged.isInLibrary = g.isInLibrary;
                                    if (HasVisibleVariants(merged)) {
                                        enriched.push_back(merged);
                                    }
                                } else {
                                    enriched.push_back(g);
                                }
                            }


                            std::unordered_map<std::string, GameInfo> byId;
                            for (auto &g : enriched) {
                                auto it = byId.find(g.id);
                                if (it == byId.end()) {
                                    byId[g.id] = g;
                                } else {
                                    GameInfo &existing = it->second;
                                    std::unordered_set<std::string> seenVariants;
                                    for (auto &v : existing.variants)
                                        seenVariants.insert(v.id);
                                    for (auto &v : g.variants) {
                                        if (seenVariants.find(v.id) == seenVariants.end()) {
                                            existing.variants.push_back(v);
                                            seenVariants.insert(v.id);
                                        }
                                    }
                                    if (existing.title.empty()) existing.title = g.title;
                                    if (existing.imageUrl.empty()) existing.imageUrl = g.imageUrl;
                                    if (existing.heroImageUrl.empty()) existing.heroImageUrl = g.heroImageUrl;
                                    if (!g.screenshotUrls.empty()) existing.screenshotUrls = g.screenshotUrls;
                                    if (existing.description.empty()) existing.description = g.description;
                                }
                            }
                            std::vector<GameInfo> finalGames;
                            for (auto &kv : byId) {
                                if (HasVisibleVariants(kv.second)) {
                                    finalGames.push_back(kv.second);
                                }
                            }
                            callback(true, finalGames, "");
                        }
                    });
            }
        };

        postGraphQL("panels/Library", [kLibraryWithTimeHash UTF8String], vars,
            ^(NSDictionary *data, NSString *error) {
                if (error.length == 0) {
                    flattenAndEnrich(data, error);
                } else {
                    postGraphQL("panels/Library", [kPanelsHash UTF8String], vars, flattenAndEnrich);
                }
            });
    });
}





std::string GameService::OptimizeImageURL(const std::string &url, int width) {
    if (url.empty()) return url;
    if (url.find("img.nvidiagrid.net") != std::string::npos) {
        return url + ";f=webp;w=" + std::to_string(width);
    }
    return url;
}





void GameService::FetchMainPanels(PanelCallback completion) {
    std::string token = m_accessToken;
    PanelCallback callback = completion;

    GetServerVpcId(token, [this, callback](const std::string &vpcId) {
        NSString *vpcIdObj = [NSString stringWithUTF8String:vpcId.c_str()];
        NSDictionary *vars = @{
            @"vpcId": vpcIdObj,
            @"locale": @"en_US",
            @"panelNames": @[@"MAIN"]
        };

        postGraphQL("panels/MainV2", [kPanelsHash UTF8String], vars,
            ^(NSDictionary *data, NSString *error) {
                if (error.length > 0) {
                    callback(false, {}, [error UTF8String]);
                    return;
                }

                NSArray *rawPanels = data[@"panels"];
                if (!rawPanels || ![rawPanels isKindOfClass:[NSArray class]]) {
                    callback(false, {}, "No panels in response");
                    return;
                }

                std::vector<PanelResult> panels;
                for (NSDictionary *p in rawPanels) {
                    if (![p isKindOfClass:[NSDictionary class]]) continue;
                    PanelResult pr;
                    { NSString *v = p[@"name"]; pr.title = v ? [v UTF8String] : ""; }
                    { NSString *v = p[@"name"]; pr.id = v ? [v UTF8String] : ""; }

                    NSArray *sections = p[@"sections"];
                    if (!sections || ![sections isKindOfClass:[NSArray class]]) continue;

                    for (NSDictionary *sec in sections) {
                        if (![sec isKindOfClass:[NSDictionary class]]) continue;
                        PanelSection ps;
                        { NSString *v = sec[@"title"]; ps.title = v ? [v UTF8String] : ""; }

                        NSArray *items = sec[@"items"];
                        if (!items || ![items isKindOfClass:[NSArray class]]) continue;

                        for (NSDictionary *item in items) {
                            if (![item isKindOfClass:[NSDictionary class]]) continue;
                            NSString *type = item[@"__typename"];
                            if (![type isEqualToString:@"GameItem"]) continue;

                            NSDictionary *app = item[@"app"];
                            if (!app) continue;

                            GameInfo game = parseGameItem(app);
                            if (!game.id.empty() && !game.title.empty() && HasVisibleVariants(game)) {
                                ps.games.push_back(game);
                            }
                        }

                        if (!ps.games.empty()) {
                            pr.sections.push_back(ps);
                        }
                    }

                    if (!pr.sections.empty()) {
                        panels.push_back(pr);
                    }
                }

                callback(true, panels, "");
            });
    });
}

static bool IsSessionReadyStatus(int status) {
    return status == 2 || status == 3;
}

static bool IsSessionActivelyQueued(const SessionInfo &session) {
    return session.status == 1 && (session.queuePosition > 0 || session.seatSetupStep > 0 || session.adState.isAdsRequired);
}

static std::string ProgressMessageForSession(const SessionInfo &session) {
    if (session.adState.isAdsRequired) {
        if (!session.adState.message.empty()) return session.adState.message;
        return session.adState.isQueuePaused ? "Session queue paused for ads." : "Ad playback is required while waiting in queue.";
    }
    if (session.queuePosition > 0) {
        return "Position #" + std::to_string(session.queuePosition) + " in queue.";
    }
    if (session.seatSetupStep > 0) {
        return "Setting up cloud rig...";
    }
    return "Waiting for cloud session...";
}

static void DispatchLaunchCompletion(const LaunchCallback &completion,
                                     bool success,
                                     const SessionInfo &session,
                                     const std::string &offerSdp,
                                     const std::string &error) {
    if (!completion) return;
    LaunchCallback completionCopy = completion;
    SessionInfo sessionCopy = session;
    std::string offerCopy = offerSdp;
    std::string errorCopy = error;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!completionCopy) return;
        completionCopy(success, sessionCopy, offerCopy, errorCopy);
    });
}

static void ReportLaunchProgress(const LaunchProgressCallback &progress, const std::string &message) {
    if (!progress) return;
    LaunchProgressCallback progressCopy = progress;
    std::string messageCopy = message;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!progressCopy) return;
        progressCopy(messageCopy, SessionInfo{});
    });
}

static void ReportLaunchProgress(const LaunchProgressCallback &progress, const SessionInfo &session) {
    if (!progress) return;
    LaunchProgressCallback progressCopy = progress;
    std::string message = ProgressMessageForSession(session);
    SessionInfo sessionCopy = session;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!progressCopy) return;
        progressCopy(message, sessionCopy);
    });
}

static void PollSessionReady(std::string sessionId,
                              std::string serverIp,
                              LaunchProgressCallback progress,
                              LaunchCallback completion) {
    __block int retries = 0;
    const int maxRetries = 60;
    __block bool lastPollWasActiveQueue = false;

    __block void (^pollBlock)(void);

    void (^poller)(bool, const SessionInfo &, const std::string &) = ^(bool ok, const SessionInfo &pollInfo, const std::string &pollErr) {
        (void)pollErr;

        if (ok && IsSessionReadyStatus(pollInfo.status) && !pollInfo.serverIp.empty()) {
            ReportLaunchProgress(progress, pollInfo);
            DispatchLaunchCompletion(completion, true, pollInfo, "", "");
            return;
        }
        if (ok) {
            ReportLaunchProgress(progress, pollInfo);
            lastPollWasActiveQueue = IsSessionActivelyQueued(pollInfo);
        } else {
            lastPollWasActiveQueue = false;
        }
        if (ok && pollInfo.status == 6) {
            ReportLaunchProgress(progress, "Previous session is ending. Waiting for GeForce NOW to finish cleanup...");
        }

        if (ok && pollInfo.status > 3 && pollInfo.status != 6) {
            DispatchLaunchCompletion(completion, false, SessionInfo{}, "", "Session in terminal error state");
            return;
        }

        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1 * NSEC_PER_SEC)), dispatch_get_main_queue(), pollBlock);
    };

    pollBlock = ^{
        if (retries >= maxRetries && !lastPollWasActiveQueue) {
            DispatchLaunchCompletion(completion, false, SessionInfo{}, "", "Session poll timeout");
            return;
        }
        if (retries >= maxRetries && lastPollWasActiveQueue) {
            retries = 0;
        }
        retries++;
        SessionManager::Shared().PollSession(sessionId, serverIp,
            [poller](bool ok, const SessionInfo &info, const std::string &err) {
                poller(ok, info, err);
            });
    };
    pollBlock();
}

static void WaitForSessionTeardown(std::string sessionId,
                                   int appId,
                                   LaunchProgressCallback progress,
                                   std::function<void(bool, const std::string &)> completion) {
    auto retries = std::make_shared<int>(0);
    auto polling = std::make_shared<bool>(false);
    const int maxRetries = 90;

    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (!timer) {
        dispatch_async(dispatch_get_main_queue(), ^{
            completion(false, "Failed to create teardown wait timer");
        });
        return;
    }

    dispatch_source_set_timer(timer,
                              dispatch_time(DISPATCH_TIME_NOW, 0),
                              (uint64_t)(2 * NSEC_PER_SEC),
                              100 * NSEC_PER_MSEC);
    dispatch_source_set_event_handler(timer, ^{
        if (*polling) return;
        if (*retries >= maxRetries) {
            dispatch_source_cancel(timer);
            dispatch_async(dispatch_get_main_queue(), ^{
                completion(false, "Timed out waiting for previous session cleanup");
            });
            return;
        }
        (*retries)++;
        *polling = true;
        ReportLaunchProgress(progress, "Previous session is ending. Waiting for GeForce NOW to finish cleanup...");
        SessionManager::Shared().GetActiveSessions([sessionId, appId, completion, timer, polling](bool ok, const std::vector<ActiveSessionEntry> &sessions, const std::string &error) {
            *polling = false;
            if (!ok) {
                NSLog(@"[GameService] Teardown wait active-session poll failed: %s", error.c_str());
                return;
            }

            for (const auto &session : sessions) {
                if (session.sessionId == sessionId && session.status == 6) {
                    return;
                }
            }

            for (const auto &session : sessions) {
                if (session.appId == appId && session.status == 6) {
                    return;
                }
            }

            dispatch_source_cancel(timer);
            dispatch_async(dispatch_get_main_queue(), ^{
                completion(true, "");
            });
        });
    });
    dispatch_resume(timer);
}

static bool IsSessionLimitExceededError(const std::string &error) {
    return error.find("SESSION_LIMIT") != std::string::npos ||
           error.find("\"statusCode\":11") != std::string::npos;
}

static bool TryUseExistingSession(const std::vector<ActiveSessionEntry> &sessions,
                                  const std::string &appId,
                                  const std::string &internalTitle,
                                  const StreamSettings &settings,
                                  bool recoveryMode,
                                  const LaunchProgressCallback &progress,
                                  const LaunchCallback &completion) {
    int appIdNum = atoi(appId.c_str());

    for (const auto &s : sessions) {
        if (s.appId == appIdNum && IsSessionReadyStatus(s.status) && !s.serverIp.empty()) {
            NSLog(@"[GameService] Claiming session %s status=%d", s.sessionId.c_str(), s.status);
            SessionManager::Shared().ClaimSession(s.sessionId, s.serverIp, appId, settings, recoveryMode,
                [completion](bool success, const SessionInfo &info, const std::string &err) {
                    DispatchLaunchCompletion(completion, success, info, "", err);
                });
            return true;
        }
    }

    for (const auto &s : sessions) {
        if (IsSessionReadyStatus(s.status) && !s.sessionId.empty() && !s.serverIp.empty()) {
            NSLog(@"[GameService] Claiming existing ready session %s for appId=%d instead of creating a second session", s.sessionId.c_str(), s.appId);
            SessionManager::Shared().ClaimSession(s.sessionId, s.serverIp, appId, settings, recoveryMode,
                [completion](bool success, const SessionInfo &info, const std::string &err) {
                    DispatchLaunchCompletion(completion, success, info, "", err);
                });
            return true;
        }
    }

    for (const auto &s : sessions) {
        if (s.appId == appIdNum && s.status == 6) {
            NSLog(@"[GameService] Waiting for previous session %s to finish teardown", s.sessionId.c_str());
            WaitForSessionTeardown(s.sessionId, appIdNum, progress,
                [appId, internalTitle, settings, recoveryMode, progress, completion](bool teardownComplete, const std::string &teardownError) {
                    if (!teardownComplete) {
                        DispatchLaunchCompletion(completion, false, SessionInfo{}, "", teardownError);
                        return;
                    }
                    GameService::Shared().LaunchGame(appId, internalTitle, settings, recoveryMode, progress, completion);
                });
            return true;
        }
    }

    for (const auto &s : sessions) {
        if (s.appId == appIdNum && s.status == 1 && !s.serverIp.empty()) {
            NSLog(@"[GameService] Polling existing session %s", s.sessionId.c_str());
            PollSessionReady(s.sessionId, s.serverIp, progress, completion);
            return true;
        }
    }

    for (const auto &s : sessions) {
        if (s.status == 1 && !s.sessionId.empty() && !s.serverIp.empty()) {
            NSLog(@"[GameService] Polling existing queued session %s for appId=%d instead of creating a second session", s.sessionId.c_str(), s.appId);
            PollSessionReady(s.sessionId, s.serverIp, progress, completion);
            return true;
        }
    }

    return false;
}

static bool RetryExistingSessionAfterLimitError(const std::string &error,
                                                const std::string &appId,
                                                const std::string &internalTitle,
                                                const StreamSettings &settings,
                                                bool recoveryMode,
                                                const LaunchProgressCallback &progress,
                                                const LaunchCallback &completion) {
    if (!IsSessionLimitExceededError(error)) return false;
    NSLog(@"[GameService] SESSION_LIMIT_EXCEEDED; retrying existing session lookup");
    SessionManager::Shared().GetActiveSessions([appId, internalTitle, settings, recoveryMode, progress, completion, error](bool ok, const std::vector<ActiveSessionEntry> &sessions, const std::string &) {
        if (ok && TryUseExistingSession(sessions, appId, internalTitle, settings, recoveryMode, progress, completion)) {
            return;
        }
        DispatchLaunchCompletion(completion, false, SessionInfo{}, "", error);
    });
    return true;
}

void GameService::LaunchGame(const std::string &appId,
                              const std::string &internalTitle,
                              const StreamSettings &settings,
                              bool recoveryMode,
                              LaunchProgressCallback progress,
                              LaunchCallback completion) {
    NSLog(@"[GameService] LaunchGame called with appId=%s recovery=%d", appId.c_str(), recoveryMode);
    SessionManager::Shared().SetAccessToken(m_accessToken);

    SessionManager::Shared().GetActiveSessions([appId, internalTitle, settings, recoveryMode, progress, completion](bool ok, const std::vector<ActiveSessionEntry> &sessions, const std::string &error) {
        (void)error;
        if (!ok) {
            NSLog(@"[GameService] GetActiveSessions failed, falling through to CreateSession");
            SessionManager::Shared().CreateSession(appId, internalTitle, settings,
                [appId, internalTitle, settings, recoveryMode, progress, completion](bool success, const SessionInfo &info, const std::string &error) {
                    if (!success) {
                        if (RetryExistingSessionAfterLimitError(error, appId, internalTitle, settings, recoveryMode, progress, completion)) {
                            return;
                        }
                        DispatchLaunchCompletion(completion, false, SessionInfo{}, "", error);
                        return;
                    }
                    if (IsSessionReadyStatus(info.status) && !info.serverIp.empty()) {
                        ReportLaunchProgress(progress, info);
                        DispatchLaunchCompletion(completion, true, info, "", "");
                        return;
                    }
                    ReportLaunchProgress(progress, info);
                    PollSessionReady(info.sessionId, info.serverIp, progress, completion);
                });
            return;
        }

        if (TryUseExistingSession(sessions, appId, internalTitle, settings, recoveryMode, progress, completion)) {
            return;
        }

        NSLog(@"[GameService] Creating new session");
        SessionManager::Shared().CreateSession(appId, internalTitle, settings,
            [appId, internalTitle, settings, recoveryMode, progress, completion](bool success, const SessionInfo &info, const std::string &error) {
                if (!success) {
                    if (RetryExistingSessionAfterLimitError(error, appId, internalTitle, settings, recoveryMode, progress, completion)) {
                        return;
                    }
                    DispatchLaunchCompletion(completion, false, SessionInfo{}, "", error);
                    return;
                }
                if (IsSessionReadyStatus(info.status) && !info.serverIp.empty()) {
                    ReportLaunchProgress(progress, info);
                    DispatchLaunchCompletion(completion, true, info, "", "");
                    return;
                }
                ReportLaunchProgress(progress, info);
                PollSessionReady(info.sessionId, info.serverIp, progress, completion);
            });
    });
}

}
