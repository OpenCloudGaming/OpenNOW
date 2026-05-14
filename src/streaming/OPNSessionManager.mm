
#include "OPNSessionManager.h"
#include "OPNStreamTypes.h"
#import <Foundation/Foundation.h>
#import <CommonCrypto/CommonCrypto.h>
#include <cstdlib>
#include <cstring>

static NSString *kNvClientId = @"ec7e38d4-03af-4b58-b131-cfb0495903ab";
static NSString *kNvClientVersion = @"2.0.80.173";

static NSString *GetUserAgent() {
    return @"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
}


static bool IsZoneHostname(const std::string &host) {
    return host.find("cloudmatchbeta.nvidiagrid.net") != std::string::npos
        || host.find("cloudmatch.nvidiagrid.net") != std::string::npos;
}

static int AdActionCode(const std::string &action) {
    if (action == "start") return 1;
    if (action == "pause") return 2;
    if (action == "resume") return 3;
    if (action == "finish") return 4;
    if (action == "cancel") return 5;
    return 0;
}

static std::string ResolveSessionBaseUrl(const std::string &streamingBaseUrl, const std::string &serverIp) {
    if (serverIp.empty() || IsZoneHostname(serverIp)) {
        return streamingBaseUrl.empty() ? "https://prod.cloudmatchbeta.nvidiagrid.net" : streamingBaseUrl;
    }
    return "https://" + serverIp;
}

static bool IsUsableEndpointHost(NSString *host) {
    return [host isKindOfClass:[NSString class]] && host.length > 0 && ![host hasPrefix:@"."];
}

static NSArray *ArrayValue(id value) {
    return [value isKindOfClass:[NSArray class]] ? (NSArray *)value : @[];
}

static NSDictionary *DictionaryValue(id value) {
    return [value isKindOfClass:[NSDictionary class]] ? (NSDictionary *)value : nil;
}

static NSString *StringValue(id value) {
    return [value isKindOfClass:[NSString class]] && [(NSString *)value length] > 0 ? (NSString *)value : nil;
}

static int PositiveIntValue(id value) {
    if ([value isKindOfClass:[NSNumber class]]) {
        int parsed = [(NSNumber *)value intValue];
        return parsed > 0 ? parsed : 0;
    }
    if ([value isKindOfClass:[NSString class]]) {
        int parsed = [(NSString *)value intValue];
        return parsed > 0 ? parsed : 0;
    }
    return 0;
}

static int IntValue(id value, int fallback = 0) {
    if ([value isKindOfClass:[NSNumber class]]) return [(NSNumber *)value intValue];
    if ([value isKindOfClass:[NSString class]]) return [(NSString *)value intValue];
    return fallback;
}

static bool BoolValue(id value, bool fallback = false) {
    if ([value isKindOfClass:[NSNumber class]]) return [(NSNumber *)value boolValue];
    if ([value isKindOfClass:[NSString class]]) {
        NSString *lower = [(NSString *)value lowercaseString];
        if ([lower isEqualToString:@"true"] || [lower isEqualToString:@"1"] || [lower isEqualToString:@"yes"]) return true;
        if ([lower isEqualToString:@"false"] || [lower isEqualToString:@"0"] || [lower isEqualToString:@"no"]) return false;
    }
    return fallback;
}

static bool VerboseSessionHttpLoggingEnabled() {
    const char *value = std::getenv("OPN_VERBOSE_SESSION_HTTP");
    return value && std::strcmp(value, "1") == 0;
}

static std::string NormalizeCloudMatchBaseUrl(const std::string &url) {
    std::string normalized = url;
    while (!normalized.empty() && normalized.back() == '/') {
        normalized.pop_back();
    }
    return normalized;
}

static void StreamColorProfileFields(const OPN::StreamSettings &settings, int &bitDepth, int &chromaFormat) {
    bitDepth = 0;
    chromaFormat = 0;
    if (settings.colorQuality == "10bit_420") {
        bitDepth = 10;
    } else if (settings.colorQuality == "8bit_444") {
        chromaFormat = 2;
    } else if (settings.colorQuality == "10bit_444") {
        bitDepth = 10;
        chromaFormat = 2;
    }
}

static NSDictionary *RequestedStreamingFeatures(const OPN::StreamSettings &settings) {
    int bitDepth = 0;
    int chromaFormat = 0;
    StreamColorProfileFields(settings, bitDepth, chromaFormat);
    return @{
        @"reflex": @(settings.enableReflex),
        @"bitDepth": @(bitDepth),
        @"cloudGsync": @(settings.enableCloudGsync),
        @"enabledL4S": @(settings.enableL4S),
        @"mouseMovementFlags": @0,
        @"trueHdr": @NO,
        @"supportedHidDevices": @0,
        @"profile": @0,
        @"fallbackToLogicalResolution": @NO,
        @"hidDevices": [NSNull null],
        @"chromaFormat": @(chromaFormat),
        @"prefilterMode": @0,
        @"prefilterSharpness": @0,
        @"prefilterNoiseReduction": @0,
        @"hudStreamingMode": @0,
        @"sdrColorSpace": @2,
        @"hdrColorSpace": @0,
    };
}

static std::string ColorQualityFromFeatures(int bitDepth, int chromaFormat) {
    const bool tenBit = bitDepth >= 10;
    const bool fourFourFour = chromaFormat == 2;
    if (tenBit && fourFourFour) return "10bit_444";
    if (tenBit) return "10bit_420";
    if (fourFourFour) return "8bit_444";
    return "8bit_420";
}

static void ParseStreamProfile(NSDictionary *session, OPN::NegotiatedStreamProfile &profile) {
    NSDictionary *negotiated = DictionaryValue(session[@"negotiatedStreamProfile"]);
    if (negotiated) {
        NSString *res = [negotiated[@"resolution"] isKindOfClass:[NSString class]] ? negotiated[@"resolution"] : nil;
        if (res) profile.resolution = [res UTF8String];
        NSString *codec = [negotiated[@"codec"] isKindOfClass:[NSString class]] ? negotiated[@"codec"] : nil;
        if (codec) profile.codec = [codec UTF8String];
        NSNumber *fpsNum = [negotiated[@"fps"] isKindOfClass:[NSNumber class]] ? negotiated[@"fps"] : nil;
        if (fpsNum) profile.fps = [fpsNum intValue];
    }

    NSDictionary *features = DictionaryValue(session[@"finalizedStreamingFeatures"]);
    if (!features) return;

    NSNumber *bitDepth = [features[@"bitDepth"] isKindOfClass:[NSNumber class]] ? features[@"bitDepth"] : nil;
    NSNumber *chromaFormat = [features[@"chromaFormat"] isKindOfClass:[NSNumber class]] ? features[@"chromaFormat"] : nil;
    if (bitDepth) profile.bitDepth = [bitDepth intValue];
    if (chromaFormat) profile.chromaFormat = [chromaFormat intValue];
    if (profile.bitDepth >= 0 || profile.chromaFormat >= 0) {
        profile.colorQuality = ColorQualityFromFeatures(profile.bitDepth, profile.chromaFormat);
        NSLog(@"[SessionManager] Finalized stream features bitDepth=%d chromaFormat=%d color=%s",
              profile.bitDepth,
              profile.chromaFormat,
              profile.colorQuality.c_str());
    }
}

static void ParseQueueProgress(NSDictionary *session, OPN::SessionInfo &info) {
    int queuePosition = PositiveIntValue(session[@"queuePosition"]);
    NSDictionary *seatSetupInfo = DictionaryValue(session[@"seatSetupInfo"]);
    if (queuePosition == 0 && seatSetupInfo) queuePosition = PositiveIntValue(seatSetupInfo[@"queuePosition"]);
    NSDictionary *sessionProgress = DictionaryValue(session[@"sessionProgress"]);
    if (queuePosition == 0 && sessionProgress) queuePosition = PositiveIntValue(sessionProgress[@"queuePosition"]);
    NSDictionary *progressInfo = DictionaryValue(session[@"progressInfo"]);
    if (queuePosition == 0 && progressInfo) queuePosition = PositiveIntValue(progressInfo[@"queuePosition"]);
    info.queuePosition = queuePosition;

    if (seatSetupInfo) {
        info.seatSetupStep = IntValue(seatSetupInfo[@"seatSetupStep"]);
    }
    if (info.seatSetupStep == 0 && sessionProgress) {
        info.seatSetupStep = IntValue(sessionProgress[@"seatSetupStep"]);
    }
    if (info.seatSetupStep == 0 && progressInfo) {
        info.seatSetupStep = IntValue(progressInfo[@"seatSetupStep"]);
    }
}

static int AdMediaProfileRank(const std::string &profile) {
    if (profile == "mp4deinterlaced720p") return 0;
    if (profile == "webm") return 1;
    if (profile == "hlsadaptive") return 2;
    return 100;
}

static OPN::SessionAdInfo ParseSessionAd(NSDictionary *ad, NSUInteger index) {
    OPN::SessionAdInfo out;
    NSString *adId = StringValue(ad[@"adId"]);
    out.adId = adId ? adId.UTF8String : ("ad-" + std::to_string((unsigned long)index + 1));
    out.adState = [ad[@"adState"] isKindOfClass:[NSNumber class]] ? [ad[@"adState"] intValue] : -1;
    if (NSString *value = StringValue(ad[@"adUrl"])) out.adUrl = value.UTF8String;
    if (NSString *value = StringValue(ad[@"mediaUrl"])) out.mediaUrl = value.UTF8String;
    if (out.mediaUrl.empty()) {
        if (NSString *value = StringValue(ad[@"videoUrl"])) out.mediaUrl = value.UTF8String;
    }
    if (out.mediaUrl.empty()) {
        if (NSString *value = StringValue(ad[@"url"])) out.mediaUrl = value.UTF8String;
    }
    if (NSString *value = StringValue(ad[@"clickThroughUrl"])) out.clickThroughUrl = value.UTF8String;
    if (NSString *value = StringValue(ad[@"title"])) out.title = value.UTF8String;
    if (NSString *value = StringValue(ad[@"description"])) out.description = value.UTF8String;
    out.adLengthInSeconds = PositiveIntValue(ad[@"adLengthInSeconds"]);
    out.durationMs = out.adLengthInSeconds > 0 ? out.adLengthInSeconds * 1000 : PositiveIntValue(ad[@"durationMs"]);
    if (out.durationMs == 0) out.durationMs = PositiveIntValue(ad[@"durationInMs"]);

    for (NSDictionary *file in ArrayValue(ad[@"adMediaFiles"])) {
        if (![file isKindOfClass:[NSDictionary class]]) continue;
        OPN::SessionAdMediaFile media;
        if (NSString *value = StringValue(file[@"mediaFileUrl"])) media.mediaFileUrl = value.UTF8String;
        if (NSString *value = StringValue(file[@"encodingProfile"])) media.encodingProfile = value.UTF8String;
        if (!media.mediaFileUrl.empty() || !media.encodingProfile.empty()) {
            out.adMediaFiles.push_back(media);
        }
    }
    std::sort(out.adMediaFiles.begin(), out.adMediaFiles.end(), [](const OPN::SessionAdMediaFile &left, const OPN::SessionAdMediaFile &right) {
        return AdMediaProfileRank(left.encodingProfile) < AdMediaProfileRank(right.encodingProfile);
    });
    if (out.mediaUrl.empty()) {
        for (const OPN::SessionAdMediaFile &file : out.adMediaFiles) {
            if (!file.mediaFileUrl.empty()) {
                out.mediaUrl = file.mediaFileUrl;
                break;
            }
        }
    }
    if (out.mediaUrl.empty() && !out.adUrl.empty()) out.mediaUrl = out.adUrl;
    return out;
}

static void ParseSessionAds(NSDictionary *session, OPN::SessionAdState &adState) {
    NSDictionary *progress = DictionaryValue(session[@"sessionProgress"]);
    NSDictionary *progressInfo = DictionaryValue(session[@"progressInfo"]);
    bool required = BoolValue(session[@"sessionAdsRequired"], false) ||
                    BoolValue(session[@"isAdsRequired"], false) ||
                    BoolValue(progress[@"isAdsRequired"], false) ||
                    BoolValue(progressInfo[@"isAdsRequired"], false);
    NSArray *ads = ArrayValue(session[@"sessionAds"]);
    adState.sessionAdsRequired = required;
    adState.serverSentEmptyAds = session[@"sessionAds"] == nil || [session[@"sessionAds"] isKindOfClass:[NSNull class]];
    adState.sessionAds.clear();

    NSUInteger index = 0;
    for (NSDictionary *ad in ads) {
        if (![ad isKindOfClass:[NSDictionary class]]) continue;
        OPN::SessionAdInfo parsed = ParseSessionAd(ad, index++);
        if (!parsed.adId.empty() || !parsed.mediaUrl.empty() || !parsed.title.empty() || !parsed.description.empty()) {
            adState.sessionAds.push_back(parsed);
        }
    }

    NSDictionary *opportunity = DictionaryValue(session[@"opportunity"]);
    if (opportunity) {
        adState.isQueuePaused = BoolValue(opportunity[@"queuePaused"], adState.isQueuePaused);
        adState.gracePeriodSeconds = PositiveIntValue(opportunity[@"gracePeriodSeconds"]);
        NSString *message = StringValue(opportunity[@"message"]) ?: StringValue(opportunity[@"description"]);
        if (message) adState.message = message.UTF8String;
        NSString *state = StringValue(opportunity[@"state"]);
        if (state && [[state lowercaseString] isEqualToString:@"graceperiodstart"]) adState.isQueuePaused = true;
    }

    adState.isAdsRequired = required || !adState.sessionAds.empty() || adState.isQueuePaused;
    if (adState.message.empty() && adState.isAdsRequired) {
        adState.message = adState.isQueuePaused ? "Resume ads to stay in queue." : "Finish ads to stay in queue.";
    }
}

static void MergeSessionAdState(OPN::SessionAdState &target, const OPN::SessionAdState &previous) {
    if (target.isAdsRequired && target.serverSentEmptyAds && target.sessionAds.empty() && !previous.sessionAds.empty()) {
        target.sessionAds = previous.sessionAds;
    }
}

static void LogPollSessionSummary(NSInteger httpStatus, const OPN::SessionInfo &info) {
    NSLog(@"[PollSession] HTTP %ld session=%s status=%d queue=%d step=%d server=%s signaling=%s gpu=%s color=%s ads=%s",
          (long)httpStatus,
          info.sessionId.empty() ? "(empty)" : info.sessionId.c_str(),
          info.status,
          info.queuePosition,
          info.seatSetupStep,
          info.serverIp.empty() ? "(pending)" : info.serverIp.c_str(),
          info.signalingServer.empty() ? "(pending)" : info.signalingServer.c_str(),
          info.gpuType.empty() ? "(pending)" : info.gpuType.c_str(),
          info.negotiatedStreamProfile.colorQuality.empty() ? "(pending)" : info.negotiatedStreamProfile.colorQuality.c_str(),
          info.adState.isAdsRequired ? "required" : "none");
}

static void ApplyCommonCloudMatchHeaders(NSMutableURLRequest *req, const std::string &accessToken, const std::string &deviceId, bool includeOrigin) {
    [req setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
    [req setValue:[NSString stringWithFormat:@"GFNJWT %s", accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
    [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
    [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
    [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
    [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
    [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
    [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-make"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-model"];
    [req setValue:@"CHROME" forHTTPHeaderField:@"nv-browser-type"];
    [req setValue:[NSString stringWithUTF8String:deviceId.c_str()] forHTTPHeaderField:@"x-device-id"];
    if (includeOrigin) {
        [req setValue:@"https://play.geforcenow.com" forHTTPHeaderField:@"Origin"];
        [req setValue:@"https://play.geforcenow.com/" forHTTPHeaderField:@"Referer"];
    }
}



static std::string ExtractHostFromUrl(const std::string &url) {
    if (url.empty()) return "";
    const char *prefixes[] = {"rtsps://", "rtsp://", "wss://", "https://"};
    std::string afterProto;
    for (const char *p : prefixes) {
        if (url.find(p) == 0) {
            afterProto = url.substr(strlen(p));
            break;
        }
    }
    if (afterProto.empty()) return "";

    size_t colon = afterProto.find(':');
    size_t slash = afterProto.find('/');
    size_t end = std::min(colon, slash);
    std::string host = afterProto.substr(0, end);
    if (host.empty() || host[0] == '.') return "";
    return host;
}

static std::string RandomUUID() {
    uuid_t uuid;
    uuid_generate(uuid);
    char str[37];
    uuid_unparse_lower(uuid, str);
    return std::string(str);
}

static std::string GetStableDeviceId() {
    static std::string s_deviceId;
    if (!s_deviceId.empty()) return s_deviceId;

    NSString *supportDir = [@"~/Library/Application Support/OpenNOW" stringByExpandingTildeInPath];
    NSString *path = [supportDir stringByAppendingPathComponent:@"device-id.plist"];
    NSString *legacyPath = [@"~/Library/Application Support/com.nvidia.gfn-device-id" stringByExpandingTildeInPath];
    NSDictionary *existing = [NSDictionary dictionaryWithContentsOfFile:path];
    if (!existing) {
        existing = [NSDictionary dictionaryWithContentsOfFile:legacyPath];
    }
    NSString *devId = existing[@"deviceId"];
    if ([devId isKindOfClass:[NSString class]] && devId.length > 0) {
        s_deviceId = [devId UTF8String];
        return s_deviceId;
    }

    s_deviceId = RandomUUID();
    NSDictionary *plist = @{@"deviceId": [NSString stringWithUTF8String:s_deviceId.c_str()]};
    [[NSFileManager defaultManager] createDirectoryAtPath:supportDir
                              withIntermediateDirectories:YES
                                               attributes:nil
                                                    error:nil];
    [plist writeToFile:path atomically:YES];
    return s_deviceId;
}

namespace OPN {

SessionManager &SessionManager::Shared() {
    static SessionManager instance;
    return instance;
}

void SessionManager::SetAccessToken(const std::string &token) {
    m_accessToken = token;
}

void SessionManager::SetStreamingBaseUrl(const std::string &url) {
    m_streamingBaseUrl = NormalizeCloudMatchBaseUrl(url);
}

void SessionManager::MergeAndStoreAdState(SessionInfo &info) {
    if (info.sessionId.empty()) return;
    std::lock_guard<std::mutex> lock(m_adStateMutex);
    auto existing = m_adStatesBySessionId.find(info.sessionId);
    if (existing != m_adStatesBySessionId.end()) {
        MergeSessionAdState(info.adState, existing->second);
    }
    m_adStatesBySessionId[info.sessionId] = info.adState;
}

void SessionManager::CreateSession(const std::string &appId,
                                    const std::string &internalTitle,
                                    const StreamSettings &settings,
                                    SessionCreateCallback completion) {
    if (m_accessToken.empty()) {
        completion(false, SessionInfo{}, "No access token");
        return;
    }

    std::string baseUrl = m_streamingBaseUrl.empty()
        ? "https://prod.cloudmatchbeta.nvidiagrid.net"
        : m_streamingBaseUrl;

    std::string clientId = RandomUUID();
    std::string deviceId = GetStableDeviceId();

    int w = 1920, h = 1080;
    sscanf(settings.resolution.c_str(), "%dx%d", &w, &h);

    bool hdrEnabled = false;

    int timezoneOffset = -[[NSTimeZone localTimeZone] secondsFromGMT] * 1000;

    NSLog(@"[SessionManager] CreateSession called with appId=%s codec=%s color=%s bitrate=%dMbps l4s=%s",
          appId.c_str(),
          settings.codec.c_str(),
          settings.colorQuality.c_str(),
          settings.maxBitrateMbps,
          settings.enableL4S ? "on" : "off");

    NSString *appIdStr = [NSString stringWithUTF8String:appId.c_str()];
    NSLog(@"[SessionManager] appIdStr=%@", appIdStr);
    if (!appIdStr) {
        NSLog(@"[SessionManager] WARNING: appIdStr is nil!");
        appIdStr = @"";
    }

    NSString *internalTitleStr = internalTitle.empty() ? @"" : [NSString stringWithUTF8String:internalTitle.c_str()];
    if (!internalTitleStr) internalTitleStr = @"";


    NSDictionary *sessionRequestData = @{
        @"appId": appIdStr,
        @"internalTitle": internalTitleStr,
        @"availableSupportedControllers": @[],
        @"networkTestSessionId": [NSNull null],
        @"parentSessionId": [NSNull null],
        @"clientIdentification": @"GFN-PC",
        @"deviceHashId": [NSString stringWithUTF8String:deviceId.c_str()],
        @"clientVersion": @"30.0",
        @"sdkVersion": @"1.0",
        @"streamerVersion": @1,
        @"clientPlatformName": @"windows",
        @"clientRequestMonitorSettings": @[@{
            @"monitorId": @0,
            @"positionX": @0,
            @"positionY": @0,
            @"widthInPixels": @(w),
            @"heightInPixels": @(h),
            @"framesPerSecond": @(settings.fps),
            @"sdrHdrMode": hdrEnabled ? @1 : @0,
            @"displayData": [NSNull null],
            @"hdr10PlusGamingData": [NSNull null],
            @"dpi": @100,
        }],
        @"useOps": @YES,
        @"audioMode": @2,
        @"metaData": @[
            @{@"key": @"SubSessionId", @"value": [NSString stringWithUTF8String:RandomUUID().c_str()]},
            @{@"key": @"wssignaling", @"value": @"1"},
            @{@"key": @"GSStreamerType", @"value": @"WebRTC"},
            @{@"key": @"networkType", @"value": @"Unknown"},
            @{@"key": @"ClientImeSupport", @"value": @"0"},
            @{@"key": @"clientPhysicalResolution", @"value": [NSString stringWithFormat:@"{\"horizontalPixels\":%d,\"verticalPixels\":%d}", w, h]},
            @{@"key": @"surroundAudioInfo", @"value": @"2"},
            @{@"key": @"store", @"value": settings.selectedStore.empty() ? @"unknown" : [NSString stringWithUTF8String:settings.selectedStore.c_str()]},
        ],
        @"sdrHdrMode": hdrEnabled ? @1 : @0,
        @"clientDisplayHdrCapabilities": [NSNull null],
        @"surroundAudioInfo": @0,
        @"remoteControllersBitmap": @0,
        @"clientTimezoneOffset": @(timezoneOffset),
        @"enhancedStreamMode": @1,
        @"appLaunchMode": @1,
        @"secureRTSPSupported": @NO,
        @"partnerCustomData": @"",
        @"accountLinked": @(settings.accountLinked),
        @"enablePersistingInGameSettings": @YES,
        @"userAge": @26,
        @"requestedStreamingFeatures": RequestedStreamingFeatures(settings),
    };


    NSDictionary *body = @{
        @"sessionRequestData": sessionRequestData,
    };

    NSString *layout = [NSString stringWithUTF8String:settings.keyboardLayout.c_str()];
    NSString *lang = [NSString stringWithUTF8String:settings.gameLanguage.c_str()];
    if (!layout) layout = @"us";
    if (!lang) lang = @"en_US";
    NSString *urlStr = [NSString stringWithFormat:@"%@/v2/session?keyboardLayout=%@&languageCode=%@",
                        [NSString stringWithUTF8String:baseUrl.c_str()], layout, lang];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
    req.HTTPMethod = @"POST";
    [req setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
    [req setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
    [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
    [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
    [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
    [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
    [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
    [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-make"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-model"];
    [req setValue:@"CHROME" forHTTPHeaderField:@"nv-browser-type"];
    [req setValue:[NSString stringWithUTF8String:deviceId.c_str()] forHTTPHeaderField:@"x-device-id"];
    [req setValue:@"https://play.geforcenow.com" forHTTPHeaderField:@"Origin"];

    NSData *bodyData = [NSJSONSerialization dataWithJSONObject:body options:0 error:nil];
    NSString *bodyStr = [[NSString alloc] initWithData:bodyData encoding:NSUTF8StringEncoding];
    if (VerboseSessionHttpLoggingEnabled()) {
        NSLog(@"[SessionManager] HTTP Body: %@", bodyStr);
    }
    req.HTTPBody = bodyData;

    SessionCreateCallback cb = completion;
    NSString *baseUrlStr = [NSString stringWithUTF8String:baseUrl.c_str()];

        [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            if (error || !data) {
                cb(false, SessionInfo{}, [[error localizedDescription] UTF8String]);
                return;
            }
            NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
            if (http.statusCode != 200) {
                NSString *bodyStr = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
                cb(false, SessionInfo{}, [[NSString stringWithFormat:@"HTTP %ld: %@", (long)http.statusCode, bodyStr] UTF8String]);
                return;
            }

            NSString *createBody = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
            if (VerboseSessionHttpLoggingEnabled()) {
                NSLog(@"[SessionManager] CreateSession response: %@", createBody);
            }

            NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
            if (!json) {
                cb(false, SessionInfo{}, "Failed to parse session response");
                return;
            }

            NSDictionary *reqStatus = DictionaryValue(json[@"requestStatus"]);
            NSNumber *statusCode = reqStatus[@"statusCode"];
            if (!statusCode || statusCode.integerValue != 1) {
                NSString *desc = reqStatus[@"statusDescription"] ?: @"unknown";
                cb(false, SessionInfo{}, [[NSString stringWithFormat:@"API error %@: %@", statusCode, desc] UTF8String]);
                return;
            }

            NSDictionary *session = DictionaryValue(json[@"session"]);
            if (!session) {
                cb(false, SessionInfo{}, "No session in response");
                return;
            }

            SessionInfo info;
            NSString *sid = [session[@"sessionId"] isKindOfClass:[NSString class]] ? session[@"sessionId"] : nil;
            info.sessionId = [sid UTF8String] ?: "";
        info.status = [session[@"status"] intValue];
        info.zone = [baseUrlStr UTF8String];
        info.streamingBaseUrl = [baseUrlStr UTF8String];
        NSString *gpu = [session[@"gpuType"] isKindOfClass:[NSString class]] ? session[@"gpuType"] : nil;
        info.gpuType = [gpu UTF8String] ?: "";

        ParseQueueProgress(session, info);





        NSArray *connections = ArrayValue(session[@"connectionInfo"]);
        for (NSDictionary *conn in connections) {
            if (![conn isKindOfClass:[NSDictionary class]]) continue;
            int usage = [conn[@"usage"] intValue];
            NSString *ip = [conn[@"ip"] isKindOfClass:[NSString class]] ? conn[@"ip"] : nil;
            int port = [conn[@"port"] intValue];
            NSString *resourcePath = [conn[@"resourcePath"] isKindOfClass:[NSString class]] ? conn[@"resourcePath"] : nil;

            if (usage == 14) {

                NSString *serverIp = nil;
                if (IsUsableEndpointHost(ip)) {
                    serverIp = ip;
                }

                if (!serverIp && resourcePath.length > 0) {
                    std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                    if (!host.empty()) {
                        serverIp = [NSString stringWithUTF8String:host.c_str()];
                    }
                }
                if (serverIp) {
                    info.serverIp = [serverIp UTF8String];
                    info.signalingServer = [NSString stringWithFormat:@"%@:%d", serverIp, port > 0 ? port : 443].UTF8String;
                    if (resourcePath.length > 0) {
                        if ([resourcePath hasPrefix:@"rtsps://"]) {
                            NSString *host = [[resourcePath substringFromIndex:8] componentsSeparatedByString:@":"].firstObject;
                            info.signalingUrl = [NSString stringWithFormat:@"wss://%@/nvst/", host].UTF8String;
                        } else if ([resourcePath hasPrefix:@"wss://"]) {
                            info.signalingUrl = [resourcePath UTF8String];
                        } else {
                            info.signalingUrl = [NSString stringWithFormat:@"wss://%s:443%@", info.serverIp.c_str(), resourcePath.length > 0 ? resourcePath : @"/nvst/"].UTF8String;
                        }
                    } else {
                        info.signalingUrl = [NSString stringWithFormat:@"wss://%s:443/nvst/", info.serverIp.c_str()].UTF8String;
                    }
                    if (port > 0) {
                        info.mediaConnectionInfo.ip = [serverIp UTF8String];
                        info.mediaConnectionInfo.port = port;
                    }
                }
            }


            if (usage == 2) {
                NSString *mediaIp = nil;
                if (IsUsableEndpointHost(ip)) {
                    mediaIp = ip;
                } else if (resourcePath.length > 0) {
                    std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                    if (!host.empty()) {
                        mediaIp = [NSString stringWithUTF8String:host.c_str()];
                    }
                }
                if (mediaIp && port > 0) {
                    info.mediaConnectionInfo.ip = [mediaIp UTF8String];
                    info.mediaConnectionInfo.port = port;
                }
            }
        }


        NSArray *iceServers = ArrayValue(session[@"iceServers"]);
        for (NSDictionary *ice in iceServers) {
            if (![ice isKindOfClass:[NSDictionary class]]) continue;
            IceServer is;
            NSArray *urls = ArrayValue(ice[@"urls"]);
            for (NSString *u in urls) {
                if ([u isKindOfClass:[NSString class]])
                    is.urls.push_back([u UTF8String]);
            }
            NSString *un = [ice[@"username"] isKindOfClass:[NSString class]] ? ice[@"username"] : nil;
            if (un) is.username = [un UTF8String];
            NSString *cred = [ice[@"credential"] isKindOfClass:[NSString class]] ? ice[@"credential"] : nil;
            if (cred) is.credential = [cred UTF8String];
            info.iceServers.push_back(is);
        }

        ParseStreamProfile(session, info.negotiatedStreamProfile);
        ParseSessionAds(session, info.adState);
        MergeAndStoreAdState(info);


        NSDictionary *ctrlInfo = DictionaryValue(session[@"sessionControlInfo"]);
        NSString *ctrlIp = [ctrlInfo[@"ip"] isKindOfClass:[NSString class]] ? ctrlInfo[@"ip"] : nil;
        if (ctrlIp.length > 0 && info.serverIp.empty()) {
            info.serverIp = [ctrlIp UTF8String];
            NSLog(@"[SessionManager] Using sessionControlInfo zone: %s", info.serverIp.c_str());
        }

        info.clientId = clientId;
        info.deviceId = deviceId;

        cb(true, info, "");
    }] resume];
}

void SessionManager::PollSession(const std::string &sessionId,
                                  const std::string &serverIp,
                                  SessionPollCallback completion) {
    if (m_accessToken.empty()) {
        completion(false, SessionInfo{}, "No access token");
        return;
    }


    std::string base = ResolveSessionBaseUrl(m_streamingBaseUrl, serverIp);
    NSString *urlStr = [NSString stringWithFormat:@"%@/v2/session/%s",
                        [NSString stringWithUTF8String:base.c_str()],
                        sessionId.c_str()];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
    [req setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
    [req setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
    [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
    [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
    [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
    [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
    [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
    [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-make"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-model"];
    [req setValue:@"CHROME" forHTTPHeaderField:@"nv-browser-type"];
    [req setValue:[NSString stringWithUTF8String:GetStableDeviceId().c_str()] forHTTPHeaderField:@"x-device-id"];

    SessionPollCallback cb = completion;
    [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        if (error || !data) {
            cb(false, SessionInfo{}, [[error localizedDescription] UTF8String]);
            return;
        }
        NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
        NSString *rawBody = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        if (VerboseSessionHttpLoggingEnabled()) {
            NSLog(@"[PollSession] Raw response: HTTP %ld body=%@", (long)http.statusCode, rawBody);
        }
        if (http.statusCode != 200) {
            cb(false, SessionInfo{}, [[NSString stringWithFormat:@"HTTP %ld: %@", (long)http.statusCode, rawBody] UTF8String]);
            return;
        }

        NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        if (!json) {
            cb(false, SessionInfo{}, [[NSString stringWithFormat:@"Failed to parse poll response: %@", rawBody] UTF8String]);
            return;
        }
        NSDictionary *session = DictionaryValue(json[@"session"]);
        if (!session) {
            cb(false, SessionInfo{}, "No session in poll response");
            return;
        }

        SessionInfo info;
        NSString *sid = [session[@"sessionId"] isKindOfClass:[NSString class]] ? session[@"sessionId"] : nil;
        info.sessionId = [sid UTF8String] ?: "";
        info.status = [session[@"status"] intValue];
        info.zone = [NSString stringWithUTF8String:base.c_str()].UTF8String;
        info.streamingBaseUrl = [NSString stringWithUTF8String:base.c_str()].UTF8String;
        NSString *gpu = [session[@"gpuType"] isKindOfClass:[NSString class]] ? session[@"gpuType"] : nil;
        info.gpuType = [gpu UTF8String] ?: "";

        ParseQueueProgress(session, info);





        NSArray *connections = ArrayValue(session[@"connectionInfo"]);
        for (NSDictionary *conn in connections) {
            if (![conn isKindOfClass:[NSDictionary class]]) continue;
            int usage = [conn[@"usage"] intValue];
            NSString *ip = [conn[@"ip"] isKindOfClass:[NSString class]] ? conn[@"ip"] : nil;
            int port = [conn[@"port"] intValue];
            NSString *resourcePath = [conn[@"resourcePath"] isKindOfClass:[NSString class]] ? conn[@"resourcePath"] : nil;

            if (usage == 14) {

                NSString *serverIp = nil;
                if (IsUsableEndpointHost(ip)) {
                    serverIp = ip;
                }

                if (!serverIp && resourcePath.length > 0) {
                    std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                    if (!host.empty()) {
                        serverIp = [NSString stringWithUTF8String:host.c_str()];
                    }
                }
                if (serverIp) {
                    info.serverIp = [serverIp UTF8String];
                    info.signalingServer = [NSString stringWithFormat:@"%@:%d", serverIp, port > 0 ? port : 443].UTF8String;
                    if (resourcePath.length > 0) {
                        if ([resourcePath hasPrefix:@"rtsps://"]) {
                            NSString *host = [[resourcePath substringFromIndex:8] componentsSeparatedByString:@":"].firstObject;
                            info.signalingUrl = [NSString stringWithFormat:@"wss://%@/nvst/", host].UTF8String;
                        } else if ([resourcePath hasPrefix:@"wss://"]) {
                            info.signalingUrl = [resourcePath UTF8String];
                        } else {
                            info.signalingUrl = [NSString stringWithFormat:@"wss://%s:443%@", info.serverIp.c_str(), resourcePath.length > 0 ? resourcePath : @"/nvst/"].UTF8String;
                        }
                    } else {
                        info.signalingUrl = [NSString stringWithFormat:@"wss://%s:443/nvst/", info.serverIp.c_str()].UTF8String;
                    }
                    if (port > 0) {
                        info.mediaConnectionInfo.ip = [serverIp UTF8String];
                        info.mediaConnectionInfo.port = port;
                    }
                }
            }


            if (usage == 2) {
                NSString *mediaIp = nil;
                if (IsUsableEndpointHost(ip)) {
                    mediaIp = ip;
                } else if (resourcePath.length > 0) {
                    std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                    if (!host.empty()) {
                        mediaIp = [NSString stringWithUTF8String:host.c_str()];
                    }
                }
                if (mediaIp && port > 0) {
                    info.mediaConnectionInfo.ip = [mediaIp UTF8String];
                    info.mediaConnectionInfo.port = port;
                }
            }
        }

        if (info.serverIp.empty()) {
            NSDictionary *ctrlInfo = DictionaryValue(session[@"sessionControlInfo"]);
            NSString *ctrlIp = [ctrlInfo[@"ip"] isKindOfClass:[NSString class]] ? ctrlInfo[@"ip"] : nil;
            if (ctrlIp.length > 0) {

                info.serverIp = [ctrlIp UTF8String];
            }
        }

        NSArray *iceServers = ArrayValue(session[@"iceServers"]);
        for (NSDictionary *ice in iceServers) {
            if (![ice isKindOfClass:[NSDictionary class]]) continue;
            IceServer is;
            NSArray *urls = ArrayValue(ice[@"urls"]);
            for (NSString *u in urls) {
                if ([u isKindOfClass:[NSString class]])
                    is.urls.push_back([u UTF8String]);
            }
            NSString *un = [ice[@"username"] isKindOfClass:[NSString class]] ? ice[@"username"] : nil;
            if (un) is.username = [un UTF8String];
            NSString *cred = [ice[@"credential"] isKindOfClass:[NSString class]] ? ice[@"credential"] : nil;
            if (cred) is.credential = [cred UTF8String];
            info.iceServers.push_back(is);
        }

        ParseStreamProfile(session, info.negotiatedStreamProfile);
        ParseSessionAds(session, info.adState);
        MergeAndStoreAdState(info);

        LogPollSessionSummary(http.statusCode, info);

        cb(true, info, "");
    }] resume];
}

void SessionManager::StopSession(const std::string &sessionId,
                                 const std::string &serverIp,
                                 std::function<void(bool, const std::string &)> completion) {
    if (m_accessToken.empty()) {
        completion(false, "No access token");
        return;
    }

    NSString *urlStr = [NSString stringWithFormat:@"https://%s/v2/session/%s",
                        serverIp.c_str(), sessionId.c_str()];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
    req.HTTPMethod = @"DELETE";
    [req setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
    [req setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
    [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];

    [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        if (error || !data) {
            completion(false, [[error localizedDescription] UTF8String]);
            return;
        }
        NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
        if (http.statusCode != 200) {
            NSString *bodyStr = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
            completion(false, [[NSString stringWithFormat:@"HTTP %ld: %@", (long)http.statusCode, bodyStr] UTF8String]);
            return;
        }
        completion(true, "");
    }] resume];
}

void SessionManager::ReportSessionAd(const SessionInfo &session,
                                     const std::string &adId,
                                     const std::string &action,
                                     int watchedTimeInMs,
                                     int pausedTimeInMs,
                                     const std::string &cancelReason,
                                     std::function<void(bool, const SessionInfo &, const std::string &)> completion) {
    if (m_accessToken.empty()) {
        completion(false, SessionInfo{}, "No access token");
        return;
    }
    int actionCode = AdActionCode(action);
    if (session.sessionId.empty() || adId.empty() || actionCode == 0) {
        completion(false, SessionInfo{}, "Invalid ad update request");
        return;
    }

    std::string base = ResolveSessionBaseUrl(session.streamingBaseUrl.empty() ? m_streamingBaseUrl : session.streamingBaseUrl, session.serverIp);
    NSString *urlStr = [NSString stringWithFormat:@"%@/v2/session/%s",
                        [NSString stringWithUTF8String:base.c_str()],
                        session.sessionId.c_str()];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
    req.HTTPMethod = @"PUT";
    ApplyCommonCloudMatchHeaders(req, m_accessToken, session.deviceId.empty() ? GetStableDeviceId() : session.deviceId, true);

    NSMutableDictionary *adUpdate = [@{
        @"adId": [NSString stringWithUTF8String:adId.c_str()],
        @"adAction": @(actionCode),
        @"clientTimestamp": @((NSInteger)[[NSDate date] timeIntervalSince1970]),
    } mutableCopy];
    if (watchedTimeInMs >= 0) adUpdate[@"watchedTimeInMs"] = @(watchedTimeInMs);
    if (pausedTimeInMs >= 0) adUpdate[@"pausedTimeInMs"] = @(pausedTimeInMs);
    if (!cancelReason.empty()) adUpdate[@"cancelReason"] = [NSString stringWithUTF8String:cancelReason.c_str()];

    NSDictionary *body = @{
        @"action": @6,
        @"adUpdates": @[adUpdate],
    };
    NSError *jsonError = nil;
    NSData *bodyData = [NSJSONSerialization dataWithJSONObject:body options:0 error:&jsonError];
    if (!bodyData || jsonError) {
        completion(false, SessionInfo{}, "Failed to encode ad update request");
        return;
    }
    req.HTTPBody = bodyData;

    auto cb = completion;
    SessionInfo sessionCopy = session;
    [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        if (error || !data) {
            cb(false, SessionInfo{}, error ? [[error localizedDescription] UTF8String] : "No ad update response");
            return;
        }
        NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
        NSString *bodyStr = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        if (http.statusCode != 200) {
            cb(false, SessionInfo{}, [[NSString stringWithFormat:@"HTTP %ld: %@", (long)http.statusCode, bodyStr] UTF8String]);
            return;
        }
        NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        NSDictionary *requestStatus = DictionaryValue(json[@"requestStatus"]);
        NSNumber *statusCode = [requestStatus[@"statusCode"] isKindOfClass:[NSNumber class]] ? requestStatus[@"statusCode"] : nil;
        if (!json || !statusCode || statusCode.integerValue != 1) {
            cb(false, SessionInfo{}, [[NSString stringWithFormat:@"Ad update API error: %@", bodyStr] UTF8String]);
            return;
        }

        SessionInfo updated = sessionCopy;
        NSDictionary *sessionJson = DictionaryValue(json[@"session"]);
        if (sessionJson) {
            updated.status = [sessionJson[@"status"] intValue];
            ParseQueueProgress(sessionJson, updated);
            ParseStreamProfile(sessionJson, updated.negotiatedStreamProfile);
            ParseSessionAds(sessionJson, updated.adState);
            this->MergeAndStoreAdState(updated);
        }
        cb(true, updated, "");
    }] resume];
}

void SessionManager::GetActiveSessions(std::function<void(bool, const std::vector<ActiveSessionEntry> &, const std::string &)> completion) {
    if (m_accessToken.empty()) {
        completion(false, {}, "No access token");
        return;
    }

    std::string base = m_streamingBaseUrl.empty()
        ? "https://prod.cloudmatchbeta.nvidiagrid.net"
        : m_streamingBaseUrl;

    NSString *urlStr = [NSString stringWithFormat:@"%@/v2/session", [NSString stringWithUTF8String:base.c_str()]];
    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
    [req setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
    [req setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
    [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
    [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
    [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
    [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
    [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-make"];
    [req setValue:@"UNKNOWN" forHTTPHeaderField:@"nv-device-model"];
    [req setValue:@"CHROME" forHTTPHeaderField:@"nv-browser-type"];
    [req setValue:[NSString stringWithUTF8String:GetStableDeviceId().c_str()] forHTTPHeaderField:@"x-device-id"];

    auto cb = completion;
    [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        if (error || !data) {
            cb(false, {}, [[error localizedDescription] UTF8String]);
            return;
        }
        NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
        if (http.statusCode != 200) {
            cb(false, {}, [[NSString stringWithFormat:@"HTTP %ld", (long)http.statusCode] UTF8String]);
            return;
        }

        NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        if (!json) {
            cb(false, {}, "Failed to parse sessions response");
            return;
        }

        NSDictionary *reqStatus = DictionaryValue(json[@"requestStatus"]);
        NSNumber *sc = reqStatus[@"statusCode"];
        if (!sc || sc.integerValue != 1) {
            cb(false, {}, "API error from sessions endpoint");
            return;
        }

        NSArray *sessions = ArrayValue(json[@"sessions"]);
        if (![sessions isKindOfClass:[NSArray class]]) {
            cb(true, {}, "");
            return;
        }

        std::vector<ActiveSessionEntry> result;
        for (NSDictionary *s in sessions) {
            if (![s isKindOfClass:[NSDictionary class]]) continue;
            int status = [s[@"status"] intValue];
            if (status != 1 && status != 2 && status != 3 && status != 6) continue;

            ActiveSessionEntry entry;
            NSString *sid = [s[@"sessionId"] isKindOfClass:[NSString class]] ? s[@"sessionId"] : nil;
            if (sid) entry.sessionId = [sid UTF8String];
            entry.status = status;


            NSDictionary *reqData = DictionaryValue(s[@"sessionRequestData"]);
            if (reqData) {
                id appIdVal = reqData[@"appId"];
                if ([appIdVal isKindOfClass:[NSNumber class]]) {
                    entry.appId = [appIdVal intValue];
                } else if ([appIdVal isKindOfClass:[NSString class]]) {
                    entry.appId = [(NSString *)appIdVal intValue];
                }
            }


            NSString *gpu = [s[@"gpuType"] isKindOfClass:[NSString class]] ? s[@"gpuType"] : nil;
            if (gpu) entry.gpuType = [gpu UTF8String];

            NSArray *conns = ArrayValue(s[@"connectionInfo"]);
            NSString *serverIp = nil;
            NSString *connIp = nil;
            for (NSDictionary *conn in conns) {
                if (![conn isKindOfClass:[NSDictionary class]]) continue;
                int usage = [conn[@"usage"] intValue];
                if (usage == 14) {
                    NSString *ip = [conn[@"ip"] isKindOfClass:[NSString class]] ? conn[@"ip"] : nil;
                    NSString *resourcePath = [conn[@"resourcePath"] isKindOfClass:[NSString class]] ? conn[@"resourcePath"] : nil;
                    if (IsUsableEndpointHost(ip)) {
                        connIp = ip;
                        break;
                    }

                    if (!connIp && resourcePath.length > 0) {
                        std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                        if (!host.empty()) {
                            connIp = [NSString stringWithUTF8String:host.c_str()];
                            break;
                        }
                    }
                }
            }

            NSDictionary *ctrlInfo = DictionaryValue(s[@"sessionControlInfo"]);
            NSString *ctrlIp = [ctrlInfo[@"ip"] isKindOfClass:[NSString class]] ? ctrlInfo[@"ip"] : nil;

            serverIp = connIp ? connIp : ctrlIp;
            if (serverIp) entry.serverIp = [serverIp UTF8String];

            if (connIp) {
                entry.signalingUrl = [NSString stringWithFormat:@"wss://%@:443/nvst/", connIp].UTF8String;
            } else if (ctrlIp) {
                entry.signalingUrl = [NSString stringWithFormat:@"wss://%@:443/nvst/", ctrlIp].UTF8String;
            }

            entry.streamingBaseUrl = base;

            result.push_back(entry);
        }

        cb(true, result, "");
    }] resume];
}

void SessionManager::pollClaimSession(std::string sessionId,
                                        std::string serverIp,
                                        std::string deviceId,
                                        std::string clientId,
                                        NegotiatedStreamProfile initialStreamProfile,
                                        SessionCreateCallback completion) {
    __block int retryCount = 0;
    const int maxRetries = 60;


    NSString *baseUrl;
    if (IsZoneHostname(serverIp)) {
        baseUrl = [NSString stringWithUTF8String:(m_streamingBaseUrl.empty() ? "https://prod.cloudmatchbeta.nvidiagrid.net" : m_streamingBaseUrl.c_str())];
    } else {
        baseUrl = [NSString stringWithFormat:@"https://%s", serverIp.c_str()];
    }

    __block void (^pollBlock)(void);

    void (^poller)(NSData *, NSError *) = ^(NSData *data, NSError *error) {
        if (error || !data) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1 * NSEC_PER_SEC)), dispatch_get_main_queue(), pollBlock);
            return;
        }

        NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        NSDictionary *session = DictionaryValue(json[@"session"]);
        if (!session) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1 * NSEC_PER_SEC)), dispatch_get_main_queue(), pollBlock);
            return;
        }

        int status = [session[@"status"] intValue];

        if (status == 2 || status == 3) {
            SessionInfo info;
            NSString *sid = [session[@"sessionId"] isKindOfClass:[NSString class]] ? session[@"sessionId"] : nil;
            info.sessionId = [sid UTF8String] ?: "";
            info.status = status;
            info.zone = [baseUrl UTF8String];
            info.streamingBaseUrl = [baseUrl UTF8String];
            NSString *gpu = [session[@"gpuType"] isKindOfClass:[NSString class]] ? session[@"gpuType"] : nil;
            info.gpuType = [gpu UTF8String] ?: "";


            NSArray *connections = ArrayValue(session[@"connectionInfo"]);
            for (NSDictionary *conn in connections) {
                if (![conn isKindOfClass:[NSDictionary class]]) continue;
                int usage = [conn[@"usage"] intValue];
                NSString *ip = [conn[@"ip"] isKindOfClass:[NSString class]] ? conn[@"ip"] : nil;
                int port = [conn[@"port"] intValue];
                NSString *resourcePath = [conn[@"resourcePath"] isKindOfClass:[NSString class]] ? conn[@"resourcePath"] : nil;

                if (usage == 14) {
                    NSString *serverIp = nil;
                    if (IsUsableEndpointHost(ip)) {
                        serverIp = ip;
                    }
                    if (!serverIp && resourcePath.length > 0) {
                        std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                        if (!host.empty()) {
                            serverIp = [NSString stringWithUTF8String:host.c_str()];
                        }
                    }
                    if (serverIp) {
                        info.serverIp = [serverIp UTF8String];
                        info.signalingServer = [NSString stringWithFormat:@"%@:%d", serverIp, port > 0 ? port : 443].UTF8String;
                        if (resourcePath.length > 0) {
                            if ([resourcePath hasPrefix:@"rtsps://"]) {
                                NSString *host = [[resourcePath substringFromIndex:8] componentsSeparatedByString:@":"].firstObject;
                                info.signalingUrl = [NSString stringWithFormat:@"wss://%@/nvst/", host].UTF8String;
                            } else if ([resourcePath hasPrefix:@"wss://"]) {
                                info.signalingUrl = [resourcePath UTF8String];
                            } else {
                                info.signalingUrl = [NSString stringWithFormat:@"wss://%s:443%@", info.serverIp.c_str(), resourcePath.length > 0 ? resourcePath : @"/nvst/"].UTF8String;
                            }
                        } else {
                            info.signalingUrl = [NSString stringWithFormat:@"wss://%s:443/nvst/", info.serverIp.c_str()].UTF8String;
                        }
                        if (port > 0) {
                            info.mediaConnectionInfo.ip = [serverIp UTF8String];
                            info.mediaConnectionInfo.port = port;
                        }
                    }
                }
                if (usage == 2) {
                    NSString *mediaIp = nil;
                    if (IsUsableEndpointHost(ip)) {
                        mediaIp = ip;
                    } else if (resourcePath.length > 0) {
                        std::string host = ExtractHostFromUrl([resourcePath UTF8String]);
                        if (!host.empty()) {
                            mediaIp = [NSString stringWithUTF8String:host.c_str()];
                        }
                    }
                    if (mediaIp && port > 0) {
                        info.mediaConnectionInfo.ip = [mediaIp UTF8String];
                        info.mediaConnectionInfo.port = port;
                    }
                }
            }

            NSArray *iceServers = ArrayValue(session[@"iceServers"]);
            for (NSDictionary *ice in iceServers) {
                if (![ice isKindOfClass:[NSDictionary class]]) continue;
                IceServer is;
                NSArray *urls = ArrayValue(ice[@"urls"]);
                for (NSString *u in urls) {
                    if ([u isKindOfClass:[NSString class]])
                        is.urls.push_back([u UTF8String]);
                }
                NSString *un = [ice[@"username"] isKindOfClass:[NSString class]] ? ice[@"username"] : nil;
                if (un) is.username = [un UTF8String];
                NSString *cred = [ice[@"credential"] isKindOfClass:[NSString class]] ? ice[@"credential"] : nil;
                if (cred) is.credential = [cred UTF8String];
                info.iceServers.push_back(is);
            }

            info.clientId = clientId;
            info.deviceId = deviceId;
            info.negotiatedStreamProfile = initialStreamProfile;
            ParseStreamProfile(session, info.negotiatedStreamProfile);

            completion(true, info, "");
        } else if (status == 1 || status == 6) {

            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1 * NSEC_PER_SEC)), dispatch_get_main_queue(), pollBlock);
        } else {

            completion(false, SessionInfo{}, "Session in terminal error state");
        }
    };

    pollBlock = ^{
        if (retryCount >= maxRetries) {
            completion(false, SessionInfo{}, "Timeout polling for session ready");
            return;
        }
        retryCount++;

        NSString *urlStr = [NSString stringWithFormat:@"%@/v2/session/%s", baseUrl, sessionId.c_str()];
        NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:urlStr]];
        [req setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
        [req setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
        [req setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
        [req setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
        [req setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
        [req setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
        [req setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
        [req setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
        [req setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
        [req setValue:[NSString stringWithUTF8String:deviceId.c_str()] forHTTPHeaderField:@"x-device-id"];

        [[[NSURLSession sharedSession] dataTaskWithRequest:req completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
            (void)response;
            poller(data, error);
        }] resume];
    };

    pollBlock();
}

void SessionManager::ClaimSession(const std::string &sessionId,
                                    const std::string &serverIp,
                                    const std::string &appId,
                                    const StreamSettings &settings,
                                    bool recoveryMode,
                                    SessionCreateCallback completion) {
    if (m_accessToken.empty()) {
        completion(false, SessionInfo{}, "No access token");
        return;
    }

    std::string deviceId = GetStableDeviceId();
    std::string clientId = RandomUUID();


    NSString *sid = [NSString stringWithUTF8String:sessionId.c_str()];
    NSString *sip = [NSString stringWithUTF8String:serverIp.c_str()];
    if (!sid) sid = @"";
    if (!sip) sip = @"";

    NSLog(@"[ClaimSession] Starting claim sessionId=%@ serverIp=%@ appId=%s codec=%s color=%s bitrate=%dMbps l4s=%s recovery=%d",
          sid,
          sip,
          appId.c_str(),
          settings.codec.c_str(),
          settings.colorQuality.c_str(),
          settings.maxBitrateMbps,
          settings.enableL4S ? "on" : "off",
          recoveryMode);

    int timezoneOffset = -[[NSTimeZone localTimeZone] secondsFromGMT] * 1000;
    NSString *subSessionId = [NSString stringWithUTF8String:RandomUUID().c_str()];

    NSDictionary *payload = @{
        @"action": @2,
        @"data": @"RESUME",
        @"sessionRequestData": @{
            @"audioMode": @2,
            @"remoteControllersBitmap": @0,
            @"sdrHdrMode": @0,
            @"networkTestSessionId": [NSNull null],
            @"availableSupportedControllers": @[],
            @"clientVersion": @"30.0",
            @"deviceHashId": [NSString stringWithUTF8String:deviceId.c_str()],
            @"internalTitle": [NSNull null],
            @"clientPlatformName": @"windows",
            @"metaData": @[
                @{@"key": @"SubSessionId", @"value": subSessionId},
                @{@"key": @"wssignaling", @"value": @"1"},
                @{@"key": @"GSStreamerType", @"value": @"WebRTC"},
                @{@"key": @"networkType", @"value": @"Unknown"},
                @{@"key": @"ClientImeSupport", @"value": @"0"},
                @{@"key": @"surroundAudioInfo", @"value": @"2"},
                @{@"key": @"store", @"value": settings.selectedStore.empty() ? @"unknown" : [NSString stringWithUTF8String:settings.selectedStore.c_str()]},
            ],
            @"surroundAudioInfo": @0,
            @"clientTimezoneOffset": @(timezoneOffset),
            @"clientIdentification": @"GFN-PC",
            @"parentSessionId": [NSNull null],
            @"appId": @([NSString stringWithUTF8String:appId.c_str()].intValue),
            @"streamerVersion": @1,
            @"appLaunchMode": @1,
            @"sdkVersion": @"1.0",
            @"enhancedStreamMode": @1,
            @"useOps": @YES,
            @"clientDisplayHdrCapabilities": [NSNull null],
            @"accountLinked": @(settings.accountLinked),
            @"partnerCustomData": @"",
            @"enablePersistingInGameSettings": @YES,
            @"secureRTSPSupported": @NO,
            @"userAge": @26,
            @"requestedStreamingFeatures": @{
                @"reflex": @(settings.enableReflex),
                @"enabledL4S": @(settings.enableL4S),
            },
        },
        @"metaData": @[],
    };

    NSString *layout = [NSString stringWithUTF8String:settings.keyboardLayout.c_str()];
    NSString *lang = [NSString stringWithUTF8String:settings.gameLanguage.c_str()];
    if (!layout) layout = @"us";
    if (!lang) lang = @"en_US";

    NSString *claimUrl = [NSString stringWithFormat:@"https://%@/v2/session/%@?keyboardLayout=%@&languageCode=%@",
                          sip, sid, layout, lang];

    if (sip.length == 0) {
        NSLog(@"[ClaimSession] ERROR: serverIp is empty, cannot construct URL");
        completion(false, SessionInfo{}, "No server IP for claim");
        return;
    }

    __block int preClaimStatus = 0;
    NSString *validationUrlStr = [NSString stringWithFormat:@"https://%@/v2/session/%@", sip, sid];
    NSLog(@"[ClaimSession] Validation GET %@", validationUrlStr);

    NSURL *validationURL = [NSURL URLWithString:validationUrlStr];
    if (!validationURL) {
        NSLog(@"[ClaimSession] ERROR: invalid validation URL: %@", validationUrlStr);
        completion(false, SessionInfo{}, "Invalid validation URL");
        return;
    }

    NSMutableURLRequest *validationReq = [NSMutableURLRequest requestWithURL:validationURL];
    validationReq.timeoutInterval = 30;
    [validationReq setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
    [validationReq setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
    [validationReq setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
    [validationReq setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
    [validationReq setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
    [validationReq setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
    [validationReq setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
    [validationReq setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
    [validationReq setValue:[NSString stringWithUTF8String:deviceId.c_str()] forHTTPHeaderField:@"x-device-id"];

    SessionCreateCallback cb = completion;

    [[[NSURLSession sharedSession] dataTaskWithRequest:validationReq completionHandler:^(NSData *vData, NSURLResponse *vResp, NSError *vErr) {
        (void)vResp;
        if (vErr) {
            NSLog(@"[ClaimSession] Validation request failed: %@", vErr.localizedDescription);
        } else if (vData) {
            NSDictionary *vJson = [NSJSONSerialization JSONObjectWithData:vData options:0 error:nil];
            NSDictionary *vSession = DictionaryValue(vJson[@"session"]);
            if (vSession) {
                preClaimStatus = [vSession[@"status"] intValue];
                NSLog(@"[ClaimSession] Pre-claim validation status=%d", preClaimStatus);
            }
        } else {
            NSLog(@"[ClaimSession] Validation request returned no data and no error");
        }

        if (preClaimStatus == 1) {
            this->pollClaimSession([sid UTF8String], [sip UTF8String], deviceId, clientId, NegotiatedStreamProfile{}, cb);
            return;
        }

        if (recoveryMode && (preClaimStatus == 2 || preClaimStatus == 3)) {
            NSLog(@"[ClaimSession] Recovery mode with ready session status=%d; skipping redundant RESUME PUT", preClaimStatus);
            this->pollClaimSession([sid UTF8String], [sip UTF8String], deviceId, clientId, NegotiatedStreamProfile{}, cb);
            return;
        }

        NSLog(@"[ClaimSession] Sending RESUME PUT to %@", claimUrl);
        NSMutableURLRequest *claimReq = [NSMutableURLRequest requestWithURL:[NSURL URLWithString:claimUrl]];
        claimReq.timeoutInterval = 15;
        claimReq.HTTPMethod = @"PUT";
        [claimReq setValue:GetUserAgent() forHTTPHeaderField:@"User-Agent"];
        [claimReq setValue:[NSString stringWithFormat:@"GFNJWT %s", m_accessToken.c_str()] forHTTPHeaderField:@"Authorization"];
        [claimReq setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
        [claimReq setValue:@"https://play.geforcenow.com" forHTTPHeaderField:@"Origin"];
        [claimReq setValue:@"https://play.geforcenow.com/" forHTTPHeaderField:@"Referer"];
        [claimReq setValue:kNvClientId forHTTPHeaderField:@"nv-client-id"];
        [claimReq setValue:@"NVIDIA-CLASSIC" forHTTPHeaderField:@"nv-client-streamer"];
        [claimReq setValue:@"NATIVE" forHTTPHeaderField:@"nv-client-type"];
        [claimReq setValue:kNvClientVersion forHTTPHeaderField:@"nv-client-version"];
        [claimReq setValue:@"MACOS" forHTTPHeaderField:@"nv-device-os"];
        [claimReq setValue:@"DESKTOP" forHTTPHeaderField:@"nv-device-type"];
        [claimReq setValue:[NSString stringWithUTF8String:deviceId.c_str()] forHTTPHeaderField:@"x-device-id"];
        claimReq.HTTPBody = [NSJSONSerialization dataWithJSONObject:payload options:0 error:nil];

        [[[NSURLSession sharedSession] dataTaskWithRequest:claimReq completionHandler:^(NSData *cData, NSURLResponse *cResp, NSError *cErr) {
            if (cErr || !cData) {
                NSString *errDesc = cErr ? [cErr localizedDescription] : @"No data";
                NSLog(@"[ClaimSession] PUT failed: %@", errDesc);
                cb(false, SessionInfo{}, [errDesc UTF8String]);
                return;
            }
            NSHTTPURLResponse *cHttp = (NSHTTPURLResponse *)cResp;
            NSString *cBody = [[NSString alloc] initWithData:cData encoding:NSUTF8StringEncoding];
            if (VerboseSessionHttpLoggingEnabled()) {
                NSLog(@"[ClaimSession] PUT response HTTP %ld body=%@", (long)cHttp.statusCode, cBody);
            } else {
                NSLog(@"[ClaimSession] PUT response HTTP %ld", (long)cHttp.statusCode);
            }
            if (cHttp.statusCode != 200) {
                cb(false, SessionInfo{}, [[NSString stringWithFormat:@"Claim HTTP %ld: %@", (long)cHttp.statusCode, cBody] UTF8String]);
                return;
            }

            NSDictionary *cJson = [NSJSONSerialization JSONObjectWithData:cData options:0 error:nil];
            NSDictionary *cReqStatus = DictionaryValue(cJson[@"requestStatus"]);
            NSNumber *cSc = cReqStatus[@"statusCode"];
            if (!cSc || cSc.integerValue != 1) {
                NSString *desc = cReqStatus[@"statusDescription"] ?: @"unknown";
                NSLog(@"[ClaimSession] PUT API error: %@: %@", cSc, desc);
                cb(false, SessionInfo{}, [[NSString stringWithFormat:@"Claim API error %@: %@", cSc, desc] UTF8String]);
                return;
            }

            NegotiatedStreamProfile claimStreamProfile;
            NSDictionary *cSession = DictionaryValue(cJson[@"session"]);
            if (cSession) {
                ParseStreamProfile(cSession, claimStreamProfile);
            }
            this->pollClaimSession([sid UTF8String], [sip UTF8String], deviceId, clientId, claimStreamProfile, cb);
        }] resume];
    }] resume];
}

}
