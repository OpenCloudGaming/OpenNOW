#include "OPNLibWebRTCStreamSession.h"
#include "OPNStreamPreferences.h"

#import <CoreAudio/CoreAudio.h>
#import <Foundation/Foundation.h>

#include <algorithm>
#include <cctype>
#include <chrono>
#include <climits>
#include <cstdio>
#include <cstdlib>
#include <sstream>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>
#include <objc/message.h>

#if defined(OPN_HAVE_LIBWEBRTC)
#import <WebRTC/WebRTC.h>
#endif

namespace OPN {

static constexpr int OPNPartialReliableInputLifetimeMs = 8;

static AudioDeviceID OPNDefaultAudioDevice(AudioObjectPropertySelector selector) {
    AudioDeviceID device = kAudioObjectUnknown;
    UInt32 size = sizeof(device);
    AudioObjectPropertyAddress address = {
        selector,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, nullptr, &size, &device) != noErr) {
        return kAudioObjectUnknown;
    }
    return device;
}

static OSStatus OPNAudioDevicesChanged(AudioObjectID,
                                       UInt32,
                                       const AudioObjectPropertyAddress *,
                                       void *clientData) {
    auto *session = static_cast<LibWebRTCStreamSession *>(clientData);
    if (!session) return noErr;
    dispatch_async(dispatch_get_main_queue(), ^{
        session->HandleAudioDeviceChange();
    });
    return noErr;
}

[[maybe_unused]] static NSString *OPNStringToNSString(const std::string &value) {
    return [[NSString alloc] initWithBytes:value.data() length:value.size() encoding:NSUTF8StringEncoding] ?: @"";
}

[[maybe_unused]] static std::string OPNNSStringToString(NSString *value) {
    return value ? std::string(value.UTF8String ?: "") : std::string();
}

struct OPNLibWebRTCIceCredentials {
    std::string ufrag;
    std::string pwd;
    std::string fingerprint;
};

[[maybe_unused]] static bool OPNStartsWith(const std::string &value, const char *prefix) {
    const size_t prefixLen = std::char_traits<char>::length(prefix);
    return value.size() >= prefixLen && value.compare(0, prefixLen, prefix) == 0;
}

[[maybe_unused]] static OPNLibWebRTCIceCredentials OPNExtractIceCredentials(const std::string &sdp) {
    OPNLibWebRTCIceCredentials credentials;
    std::istringstream stream(sdp);
    std::string line;
    while (std::getline(stream, line)) {
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (OPNStartsWith(line, "a=ice-ufrag:")) {
            credentials.ufrag = line.substr(12);
        } else if (OPNStartsWith(line, "a=ice-pwd:")) {
            credentials.pwd = line.substr(10);
        } else if (OPNStartsWith(line, "a=fingerprint:")) {
            credentials.fingerprint = line.substr(14);
        }
    }
    return credentials;
}

[[maybe_unused]] static std::vector<std::string> OPNSplitSdpLines(const std::string &sdp) {
    std::vector<std::string> lines;
    std::stringstream stream(sdp);
    std::string line;
    while (std::getline(stream, line)) {
        if (!line.empty() && line.back() == '\r') line.pop_back();
        lines.push_back(line);
    }
    return lines;
}

[[maybe_unused]] static std::string OPNJoinSdpLines(const std::vector<std::string> &lines, const std::string &lineEnding) {
    std::string out;
    for (size_t i = 0; i < lines.size(); i++) {
        out += lines[i];
        if (i + 1 < lines.size()) out += lineEnding;
    }
    return out;
}

[[maybe_unused]] static std::string OPNJoinSdpLinesLike(const std::vector<std::string> &lines, const std::string &originalSdp) {
    const std::string lineEnding = originalSdp.find("\r\n") != std::string::npos ? "\r\n" : "\n";
    std::string out = OPNJoinSdpLines(lines, lineEnding);
    if (!originalSdp.empty() && originalSdp.back() == '\n') {
        out += lineEnding;
    }
    return out;
}

[[maybe_unused]] static int OPNPayloadTypeFromAttribute(const std::string &line, const char *prefix) {
    if (!OPNStartsWith(line, prefix)) return -1;
    size_t pos = strlen(prefix);
    size_t end = line.find_first_of(" \t:", pos);
    if (end == std::string::npos || end <= pos) return -1;
    std::string payload = line.substr(pos, end - pos);
    for (char c : payload) {
        if (!std::isdigit((unsigned char)c)) return -1;
    }
    return atoi(payload.c_str());
}

[[maybe_unused]] static int OPNAptFromFmtp(const std::string &line) {
    size_t pos = line.find("apt=");
    if (pos == std::string::npos) return -1;
    pos += strlen("apt=");
    size_t end = pos;
    while (end < line.size() && std::isdigit((unsigned char)line[end])) end++;
    if (end == pos) return -1;
    return atoi(line.substr(pos, end - pos).c_str());
}

[[maybe_unused]] static bool OPNPayloadVectorContains(const std::vector<int> &payloads, int pt) {
    return std::find(payloads.begin(), payloads.end(), pt) != payloads.end();
}

[[maybe_unused]] static bool OPNRtpmapMatchesCodec(const std::string &rtpmapLine, const std::string &normalizedCodec) {
    std::string upper = rtpmapLine;
    std::transform(upper.begin(), upper.end(), upper.begin(), [](unsigned char c) { return (char)std::toupper(c); });
    if (normalizedCodec == "H265") {
        return upper.find(" H265/") != std::string::npos || upper.find(" HEVC/") != std::string::npos;
    }
    if (normalizedCodec == "AV1") return upper.find(" AV1/") != std::string::npos;
    if (normalizedCodec == "H264") return upper.find(" H264/") != std::string::npos;
    return false;
}

[[maybe_unused]] static std::string OPNPayloadVectorToString(const std::vector<int> &payloads) {
    std::ostringstream out;
    for (size_t i = 0; i < payloads.size(); i++) {
        if (i) out << ",";
        out << payloads[i];
    }
    return out.str();
}

[[maybe_unused]] static std::string OPNExtractPublicIp(const std::string &hostOrIp) {
    if (hostOrIp.empty()) return "";

    int dots = 0;
    int digits = 0;
    bool dotted = true;
    for (char c : hostOrIp) {
        if (c == '.') {
            if (digits == 0) {
                dotted = false;
                break;
            }
            dots++;
            digits = 0;
        } else if (std::isdigit((unsigned char)c)) {
            digits++;
            if (digits > 3) {
                dotted = false;
                break;
            }
        } else {
            dotted = false;
            break;
        }
    }
    if (dotted && dots == 3 && digits > 0) return hostOrIp;

    std::string firstLabel = hostOrIp.substr(0, hostOrIp.find('.'));
    std::vector<std::string> parts;
    std::stringstream stream(firstLabel);
    std::string part;
    while (std::getline(stream, part, '-')) {
        if (part.empty()) return "";
        for (char c : part) {
            if (!std::isdigit((unsigned char)c)) return "";
        }
        parts.push_back(part);
    }
    if (parts.size() != 4) return "";
    return parts[0] + "." + parts[1] + "." + parts[2] + "." + parts[3];
}

[[maybe_unused]] static std::string OPNFixServerIpInSdp(const std::string &sdp, const std::string &serverHostOrIp) {
    std::string ip = OPNExtractPublicIp(serverHostOrIp);
    if (ip.empty()) return sdp;

    std::vector<std::string> lines = OPNSplitSdpLines(sdp);
    int connectionRewrites = 0;
    int candidateRewrites = 0;
    for (std::string &line : lines) {
        if (line == "c=IN IP4 0.0.0.0") {
            line = "c=IN IP4 " + ip;
            connectionRewrites++;
            continue;
        }
        if (!OPNStartsWith(line, "a=candidate:")) continue;

        std::vector<std::string> tokens;
        std::stringstream stream(line);
        std::string token;
        while (stream >> token) tokens.push_back(token);
        if (tokens.size() <= 4 || tokens[4] != "0.0.0.0") continue;
        tokens[4] = ip;
        std::string rewritten;
        for (size_t i = 0; i < tokens.size(); i++) {
            if (i) rewritten += ' ';
            rewritten += tokens[i];
        }
        line = rewritten;
        candidateRewrites++;
    }

    if (connectionRewrites > 0 || candidateRewrites > 0) {
        NSLog(@"[LibWebRTC] Fixed server IP in offer SDP ip=%s c-lines=%d candidates=%d",
              ip.c_str(),
              connectionRewrites,
              candidateRewrites);
    }
    return OPNJoinSdpLines(lines, sdp.find("\r\n") != std::string::npos ? "\r\n" : "\n");
}

[[maybe_unused]] static std::string OPNMungeAnswerSdp(const std::string &sdp, int maxBitrateKbps) {
    std::vector<std::string> lines = OPNSplitSdpLines(sdp);
    std::vector<std::string> result;
    result.reserve(lines.size() + 4);
    int bitrateLines = 0;
    int stereoLines = 0;

    for (size_t i = 0; i < lines.size(); i++) {
        std::string line = lines[i];
        if (OPNStartsWith(line, "a=fmtp:") && line.find("minptime=") != std::string::npos && line.find("stereo=1") == std::string::npos) {
            line += ";stereo=1";
            stereoLines++;
        }
        result.push_back(line);

        if (OPNStartsWith(line, "m=video") || OPNStartsWith(line, "m=audio")) {
            const bool nextHasBandwidth = i + 1 < lines.size() && OPNStartsWith(lines[i + 1], "b=");
            if (!nextHasBandwidth) {
                int bitrate = OPNStartsWith(line, "m=video") ? std::max(1000, maxBitrateKbps) : 128;
                result.push_back("b=AS:" + std::to_string(bitrate));
                bitrateLines++;
            }
        }
    }

    if (bitrateLines > 0 || stereoLines > 0) {
        NSLog(@"[LibWebRTC] Munged answer SDP bitrateLines=%d stereoLines=%d videoBitrate=%dkbps",
              bitrateLines,
              stereoLines,
              std::max(1000, maxBitrateKbps));
    }
    return OPNJoinSdpLines(result, sdp.find("\r\n") != std::string::npos ? "\r\n" : "\n");
}

[[maybe_unused]] static void OPNLogVideoSdpSummary(const char *label, const std::string &sdp) {
    bool inVideo = false;
    int logged = 0;
    for (const std::string &line : OPNSplitSdpLines(sdp)) {
        if (OPNStartsWith(line, "m=video")) {
            inVideo = true;
            NSLog(@"[LibWebRTC] %s %s", label, line.c_str());
            logged++;
            continue;
        }
        if (OPNStartsWith(line, "m=") && inVideo) break;
        if (!inVideo) continue;
        if (OPNStartsWith(line, "a=rtpmap:") || OPNStartsWith(line, "a=fmtp:") || OPNStartsWith(line, "a=rtcp-fb:")) {
            NSLog(@"[LibWebRTC] %s %s", label, line.c_str());
            logged++;
            if (logged >= 64) break;
        }
    }
}

[[maybe_unused]] static bool OPNVideoSdpHasMediaCodec(const std::string &sdp) {
    bool inVideo = false;
    for (const std::string &line : OPNSplitSdpLines(sdp)) {
        if (OPNStartsWith(line, "m=video")) {
            inVideo = true;
            continue;
        }
        if (OPNStartsWith(line, "m=") && inVideo) break;
        if (!inVideo || !OPNStartsWith(line, "a=rtpmap:")) continue;

        std::string upper = line;
        std::transform(upper.begin(), upper.end(), upper.begin(), [](unsigned char c) { return (char)std::toupper(c); });
        if (upper.find(" H264/") != std::string::npos ||
            upper.find(" H265/") != std::string::npos ||
            upper.find(" HEVC/") != std::string::npos ||
            upper.find(" AV1/") != std::string::npos ||
            upper.find(" VP8/") != std::string::npos ||
            upper.find(" VP9/") != std::string::npos) {
            return true;
        }
    }
    return false;
}

[[maybe_unused]] static std::string OPNReplaceAll(std::string value, const std::string &from, const std::string &to) {
    if (from.empty()) return value;
    size_t pos = 0;
    while ((pos = value.find(from, pos)) != std::string::npos) {
        value.replace(pos, from.size(), to);
        pos += to.size();
    }
    return value;
}

[[maybe_unused]] static std::string OPNRewriteH265OfferForReceiver(const std::string &sdp,
                                                                   int maxMainLevelId,
                                                                   int maxMain10LevelId,
                                                                   bool supportsHighTier) {
    std::vector<std::string> lines = OPNSplitSdpLines(sdp);
    std::unordered_set<int> h265Payloads;
    bool inVideo = false;

    for (const std::string &line : lines) {
        if (OPNStartsWith(line, "m=")) {
            inVideo = OPNStartsWith(line, "m=video");
            continue;
        }
        if (!inVideo || !OPNStartsWith(line, "a=rtpmap:")) continue;
        int pt = OPNPayloadTypeFromAttribute(line, "a=rtpmap:");
        if (pt >= 0 && OPNRtpmapMatchesCodec(line, "H265")) {
            h265Payloads.insert(pt);
        }
    }

    if (h265Payloads.empty()) return sdp;

    int tierRewrites = 0;
    for (std::string &line : lines) {
        if (!OPNStartsWith(line, "a=fmtp:")) continue;
        int pt = OPNPayloadTypeFromAttribute(line, "a=fmtp:");
        if (pt < 0 || h265Payloads.find(pt) == h265Payloads.end()) continue;

        if (!supportsHighTier && line.find("tier-flag=1") != std::string::npos) {
            line = OPNReplaceAll(line, "tier-flag=1", "tier-flag=0");
            tierRewrites++;
        }

    }

    if (tierRewrites > 0) {
        NSLog(@"[LibWebRTC] Rewrote H265 offer tier for receiver compatibility: tier=%d maxMain=%d maxMain10=%d highTier=%d",
              tierRewrites,
              maxMainLevelId,
              maxMain10LevelId,
              supportsHighTier);
    }
    return OPNJoinSdpLinesLike(lines, sdp);
}

[[maybe_unused]] static std::vector<std::string> OPNSplitResolution(const std::string &resolution) {
    const size_t x = resolution.find('x');
    if (x == std::string::npos) return {"1920", "1080"};
    std::string width = resolution.substr(0, x);
    std::string height = resolution.substr(x + 1);
    if (width.empty() || height.empty()) return {"1920", "1080"};
    return {width, height};
}

[[maybe_unused]] static int OPNStringToPositiveInt(const std::string &value, int fallback) {
    if (value.empty()) return fallback;
    char *end = nullptr;
    long parsed = strtol(value.c_str(), &end, 10);
    if (end == value.c_str() || parsed <= 0 || parsed > INT_MAX) return fallback;
    return (int)parsed;
}

[[maybe_unused]] static uint64_t OPNMonotonicMs() {
    using Clock = std::chrono::steady_clock;
    return (uint64_t)std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now().time_since_epoch()).count();
}

[[maybe_unused]] static bool OPNEnvFlagEnabled(const char *name, bool defaultValue) {
    const char *value = getenv(name);
    if (!value || !*value) return defaultValue;
    std::string normalized(value);
    std::transform(normalized.begin(), normalized.end(), normalized.begin(), [](unsigned char c) { return (char)std::tolower(c); });
    return !(normalized == "0" || normalized == "false" || normalized == "no" || normalized == "off");
}

[[maybe_unused]] static double OPNStatsSecondsToMs(double seconds) {
    return seconds * 1000.0;
}

[[maybe_unused]] static std::string OPNNormalizeStatsCodecName(const std::string &codecId) {
    std::string upper = codecId;
    std::transform(upper.begin(), upper.end(), upper.begin(), [](unsigned char c) { return (char)std::toupper(c); });
    if (upper.find("H264") != std::string::npos) return "H264";
    if (upper.find("H265") != std::string::npos || upper.find("HEVC") != std::string::npos) return "H265";
    if (upper.find("AV1") != std::string::npos) return "AV1";
    if (upper.find("VP9") != std::string::npos || upper.find("VP09") != std::string::npos) return "VP9";
    if (upper.find("VP8") != std::string::npos) return "VP8";
    return codecId;
}

[[maybe_unused]] static std::string OPNNormalizeCodec(std::string codec) {
    std::transform(codec.begin(), codec.end(), codec.begin(), [](unsigned char c) { return (char)std::toupper(c); });
    if (codec == "AUTO") return "H264";
    if (codec == "HEVC") return "H265";
    return codec;
}

[[maybe_unused]] static bool OPNIsSupportedCodecPreference(const std::string &codec) {
    return codec == "H264" || codec == "H265" || codec == "AV1";
}

[[maybe_unused]] static bool OPNCodecCapabilityMatches(RTCRtpCodecCapability *codec, const std::string &normalizedCodec) {
    NSString *name = codec.name ?: @"";
    NSString *mimeType = codec.mimeType ?: @"";
    NSString *combined = [[NSString stringWithFormat:@"%@ %@", name, mimeType] uppercaseString];
    if (normalizedCodec == "H265") return [combined containsString:@"H265"] || [combined containsString:@"HEVC"];
    if (normalizedCodec == "H264") return [combined containsString:@"H264"];
    if (normalizedCodec == "AV1") return [combined containsString:@"AV1"];
    return false;
}

[[maybe_unused]] static bool OPNCodecCapabilityIsTransportSupport(RTCRtpCodecCapability *codec) {
    NSString *name = [codec.name ?: @"" uppercaseString];
    NSString *mimeType = [codec.mimeType ?: @"" uppercaseString];
    return [name isEqualToString:@"RTX"] || [name isEqualToString:@"RED"] ||
           [name isEqualToString:@"ULPFEC"] || [name isEqualToString:@"FLEXFEC-03"] ||
           [mimeType containsString:@"/RTX"] || [mimeType containsString:@"/RED"] ||
           [mimeType containsString:@"/ULPFEC"] || [mimeType containsString:@"/FLEXFEC-03"];
}

[[maybe_unused]] static std::string OPNPreferCodecInOffer(const std::string &sdp, const std::string &normalizedCodec) {
    std::vector<std::string> lines = OPNSplitSdpLines(sdp);
    bool inVideo = false;
    std::vector<int> codecPayloads;
    std::vector<int> keptPayloads;

    for (const std::string &line : lines) {
        if (OPNStartsWith(line, "m=")) {
            inVideo = OPNStartsWith(line, "m=video");
            continue;
        }
        if (!inVideo || !OPNStartsWith(line, "a=rtpmap:")) continue;
        int pt = OPNPayloadTypeFromAttribute(line, "a=rtpmap:");
        if (pt >= 0 && OPNRtpmapMatchesCodec(line, normalizedCodec)) {
            codecPayloads.push_back(pt);
        }
    }

    if (codecPayloads.empty()) {
        NSLog(@"[LibWebRTC] Offer %s preference skipped; no matching payload found", normalizedCodec.c_str());
        return sdp;
    }

    keptPayloads = codecPayloads;
    inVideo = false;
    for (const std::string &line : lines) {
        if (OPNStartsWith(line, "m=")) {
            inVideo = OPNStartsWith(line, "m=video");
            continue;
        }
        if (!inVideo || !OPNStartsWith(line, "a=fmtp:")) continue;
        int pt = OPNPayloadTypeFromAttribute(line, "a=fmtp:");
        int apt = OPNAptFromFmtp(line);
        if (pt >= 0 && apt >= 0 && OPNPayloadVectorContains(codecPayloads, apt) && !OPNPayloadVectorContains(keptPayloads, pt)) {
            keptPayloads.push_back(pt);
        }
    }

    auto keepPayload = [&keptPayloads](int pt) {
        return std::find(keptPayloads.begin(), keptPayloads.end(), pt) != keptPayloads.end();
    };

    std::vector<std::string> filtered;
    filtered.reserve(lines.size());
    inVideo = false;
    int removedPayloadLines = 0;
    for (const std::string &line : lines) {
        if (OPNStartsWith(line, "m=")) {
            inVideo = OPNStartsWith(line, "m=video");
            if (inVideo) {
                std::stringstream stream(line);
                std::vector<std::string> tokens;
                std::string token;
                while (stream >> token) tokens.push_back(token);
                if (tokens.size() > 3) {
                    std::ostringstream mline;
                    mline << tokens[0] << " " << tokens[1] << " " << tokens[2];
                    for (int pt : keptPayloads) mline << " " << pt;
                    filtered.push_back(mline.str());
                    continue;
                }
            }
            filtered.push_back(line);
            continue;
        }

        if (inVideo && (OPNStartsWith(line, "a=rtpmap:") || OPNStartsWith(line, "a=fmtp:") || OPNStartsWith(line, "a=rtcp-fb:"))) {
            const char *prefix = OPNStartsWith(line, "a=rtpmap:") ? "a=rtpmap:" : OPNStartsWith(line, "a=fmtp:") ? "a=fmtp:" : "a=rtcp-fb:";
            int pt = OPNPayloadTypeFromAttribute(line, prefix);
            if (pt >= 0 && !keepPayload(pt)) {
                removedPayloadLines++;
                continue;
            }
        }
        filtered.push_back(line);
    }

    NSLog(@"[LibWebRTC] Preferred %s offer payloads (%zu codec=%s, %zu kept=%s), removed %d non-%s payload lines",
          normalizedCodec.c_str(),
          codecPayloads.size(),
          OPNPayloadVectorToString(codecPayloads).c_str(),
          keptPayloads.size(),
          OPNPayloadVectorToString(keptPayloads).c_str(),
          removedPayloadLines,
          normalizedCodec.c_str());
    return OPNJoinSdpLines(filtered, sdp.find("\r\n") != std::string::npos ? "\r\n" : "\n");
}

[[maybe_unused]] static std::string OPNBuildNvstSdp(const StreamSettings &settings, const OPNLibWebRTCIceCredentials &credentials) {
    std::vector<std::string> resolution = OPNSplitResolution(settings.resolution);
    const int width = OPNStringToPositiveInt(resolution[0], 1920);
    const int height = OPNStringToPositiveInt(resolution[1], 1080);
    const int maxBitrateKbps = std::max(1000, settings.maxBitrateMbps * 1000);
    const int minBitrateKbps = std::max(5000, maxBitrateKbps * 35 / 100);
    const int initialBitrateKbps = std::max(minBitrateKbps, maxBitrateKbps * 70 / 100);
    const int bitDepth = OPNStartsWith(settings.colorQuality, "10bit") ? 10 : 8;
    const std::string codec = OPNNormalizeCodec(settings.codec);
    const bool isAv1 = codec == "AV1";
    const bool isHighFps = settings.fps >= 90;
    const bool is120Fps = settings.fps == 120;
    const bool is240Fps = settings.fps >= 240;

    std::vector<std::string> lines = {
        "v=0",
        "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
        "s=-",
        "t=0 0",
        "a=general.icePassword:" + credentials.pwd,
        "a=general.iceUserNameFragment:" + credentials.ufrag,
        "a=general.dtlsFingerprint:" + credentials.fingerprint,
        "m=video 0 RTP/AVP",
        "a=msid:fbc-video-0",
        "a=vqos.fec.rateDropWindow:10",
        "a=vqos.fec.minRequiredFecPackets:2",
        "a=vqos.fec.repairMinPercent:5",
        "a=vqos.fec.repairPercent:5",
        "a=vqos.fec.repairMaxPercent:35",
        "a=vqos.dynamicStreamingMode:0",
        "a=vqos.drc.enable:0",
        "a=vqos.dfc.enable:0",
        "a=vqos.dfc.adjustResAndFps:0",
        "a=video.dx9EnableNv12:1",
        "a=video.dx9EnableHdr:1",
        "a=vqos.qpg.enable:1",
        "a=vqos.resControl.qp.qpg.featureSetting:7",
        "a=bwe.useOwdCongestionControl:1",
        "a=video.enableRtpNack:1",
        "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200",
        "a=vqos.drc.bitrateIirFilterFactor:18",
        "a=video.packetSize:1140",
        "a=packetPacing.minNumPacketsPerGroup:15",
    };

    if (isHighFps) {
        lines.insert(lines.end(), {
            "a=bwe.iirFilterFactor:8",
            "a=video.encoderFeatureSetting:47",
            "a=video.encoderPreset:6",
            "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600",
            "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9",
            std::string("a=video.fbcDynamicFpsGrabTimeoutMs:") + (is120Fps ? "6" : "18"),
            std::string("a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:") + (is120Fps ? "6000" : "12000"),
        });
    }

    if (is240Fps) {
        lines.insert(lines.end(), {
            "a=video.enableNextCaptureMode:1",
            "a=vqos.maxStreamFpsEstimate:240",
            "a=video.videoSplitEncodeStripsPerFrame:3",
            "a=video.updateSplitEncodeStateDynamically:1",
        });
    }

    lines.insert(lines.end(), {
        "a=vqos.adjustStreamingFpsDuringOutOfFocus:1",
        "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1",
        "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1",
        "a=vqos.resControl.cpmRtc.featureMask:0",
        "a=vqos.resControl.cpmRtc.enable:0",
        "a=vqos.resControl.cpmRtc.minResolutionPercent:100",
        "a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999",
        std::string("a=packetPacing.numGroups:") + (is120Fps ? "3" : "5"),
        "a=packetPacing.maxDelayUs:1000",
        "a=packetPacing.minNumPacketsFrame:10",
        "a=video.rtpNackQueueLength:1024",
        "a=video.rtpNackQueueMaxPackets:512",
        "a=video.rtpNackMaxPacketCount:25",
        "a=vqos.drc.qpMaxResThresholdAdj:4",
        "a=vqos.grc.qpMaxResThresholdAdj:4",
        "a=vqos.drc.iirFilterFactor:100",
    });

    if (isAv1) {
        lines.insert(lines.end(), {
            "a=vqos.drc.minQpHeadroom:20",
            "a=vqos.drc.lowerQpThreshold:100",
            "a=vqos.drc.upperQpThreshold:200",
            "a=vqos.drc.minAdaptiveQpThreshold:180",
            "a=vqos.drc.qpCodecThresholdAdj:0",
            "a=vqos.drc.qpMaxResThresholdAdj:20",
            "a=vqos.dfc.minQpHeadroom:20",
            "a=vqos.dfc.qpLowerLimit:100",
            "a=vqos.dfc.qpMaxUpperLimit:200",
            "a=vqos.dfc.qpMinUpperLimit:180",
            "a=vqos.dfc.qpMaxResThresholdAdj:20",
            "a=vqos.dfc.qpCodecThresholdAdj:0",
            "a=vqos.grc.minQpHeadroom:20",
            "a=vqos.grc.lowerQpThreshold:100",
            "a=vqos.grc.upperQpThreshold:200",
            "a=vqos.grc.minAdaptiveQpThreshold:180",
            "a=vqos.grc.qpMaxResThresholdAdj:20",
            "a=vqos.grc.qpCodecThresholdAdj:0",
            "a=video.minQp:25",
            "a=video.enableAv1RcPrecisionFactor:1",
        });
    }

    lines.insert(lines.end(), {
        "a=video.clientViewportWd:" + std::to_string(width),
        "a=video.clientViewportHt:" + std::to_string(height),
        "a=video.maxFPS:" + std::to_string(settings.fps),
        "a=video.initialBitrateKbps:" + std::to_string(initialBitrateKbps),
        "a=video.initialPeakBitrateKbps:" + std::to_string(maxBitrateKbps),
        "a=vqos.bw.maximumBitrateKbps:" + std::to_string(maxBitrateKbps),
        "a=vqos.bw.minimumBitrateKbps:" + std::to_string(minBitrateKbps),
        "a=vqos.bw.peakBitrateKbps:" + std::to_string(maxBitrateKbps),
        "a=vqos.bw.serverPeakBitrateKbps:" + std::to_string(maxBitrateKbps),
        "a=vqos.bw.enableBandwidthEstimation:1",
        "a=vqos.bw.disableBitrateLimit:0",
        "a=vqos.grc.maximumBitrateKbps:" + std::to_string(maxBitrateKbps),
        "a=vqos.grc.enable:0",
        "a=video.maxNumReferenceFrames:4",
        "a=video.mapRtpTimestampsToFrames:1",
        "a=video.encoderCscMode:3",
        "a=video.dynamicRangeMode:0",
        "a=video.bitDepth:" + std::to_string(bitDepth),
        std::string("a=video.scalingFeature1:") + (isAv1 ? "1" : "0"),
        "a=video.prefilterParams.prefilterModel:0",
        "m=audio 0 RTP/AVP",
        "a=msid:audio",
        "m=mic 0 RTP/AVP",
        "a=msid:mic",
        "a=rtpmap:0 PCMU/8000",
        "m=application 0 RTP/AVP",
        "a=msid:input_1",
        "a=ri.partialReliableThresholdMs:" + std::to_string(OPNPartialReliableInputLifetimeMs),
        "a=ri.hidDeviceMask:4294967295",
        "a=ri.enablePartiallyReliableTransferGamepad:15",
        "a=ri.enablePartiallyReliableTransferHid:4294967295",
        "",
    });

    std::string result;
    for (const std::string &line : lines) {
        result += line;
        result += '\n';
    }
    return result;
}

#if defined(OPN_HAVE_LIBWEBRTC)
class LibWebRTCStreamSession;
}

@interface OPNLibWebRTCSessionImpl : NSObject <RTCPeerConnectionDelegate, RTCDataChannelDelegate>
- (instancetype)initWithOwner:(OPN::LibWebRTCStreamSession *)owner;
@property(nonatomic, assign) OPN::LibWebRTCStreamSession *owner;
@property(nonatomic, strong) RTCPeerConnectionFactory *factory;
@property(nonatomic, strong) RTCPeerConnection *peerConnection;
@property(nonatomic, strong) RTCDataChannel *reliableInputChannel;
@property(nonatomic, strong) RTCDataChannel *partialInputChannel;
@property(nonatomic, strong) RTCVideoTrack *remoteVideoTrack;
@property(nonatomic, strong) NSView *remoteVideoView;
@property(nonatomic, strong) id<RTCVideoRenderer> remoteVideoRenderer;
@property(nonatomic, strong) RTCAudioTrack *remoteAudioTrack;
@property(nonatomic, strong) RTCAudioTrack *localMicrophoneTrack;
@property(nonatomic, strong) RTCRtpSender *localMicrophoneSender;
@end

@interface OPNPacedVideoRenderer : NSView <RTCVideoRenderer>
- (instancetype)initWithFrame:(NSRect)frame targetFps:(int)targetFps owner:(OPN::LibWebRTCStreamSession *)owner;
@end

@interface OPNPacedVideoRenderer ()
@property(nonatomic, strong) RTCMTLNSVideoView *videoView;
@property(nonatomic, strong) NSTimer *displayTimer;
@property(nonatomic, strong) RTCVideoFrame *pendingFrame;
@property(nonatomic, assign) BOOL hasPendingFrame;
@property(nonatomic, assign) int targetFps;
@property(nonatomic, assign) OPN::LibWebRTCStreamSession *owner;
@end

@implementation OPNPacedVideoRenderer

- (instancetype)initWithFrame:(NSRect)frame targetFps:(int)targetFps owner:(OPN::LibWebRTCStreamSession *)owner {
    self = [super initWithFrame:frame];
    if (self) {
        _targetFps = MAX(30, MIN(targetFps, 120));
        _owner = owner;
        self.wantsLayer = YES;
        self.layer.backgroundColor = NSColor.blackColor.CGColor;
        _videoView = [[RTCMTLNSVideoView alloc] initWithFrame:self.bounds];
        _videoView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        _videoView.wantsLayer = YES;
        _videoView.layer.backgroundColor = NSColor.blackColor.CGColor;
        [self addSubview:_videoView];

        NSTimeInterval interval = 1.0 / (NSTimeInterval)_targetFps;
        _displayTimer = [NSTimer timerWithTimeInterval:interval target:self selector:@selector(displayTimerFired:) userInfo:nil repeats:YES];
        [NSRunLoop.mainRunLoop addTimer:_displayTimer forMode:NSRunLoopCommonModes];
    }
    return self;
}

- (void)dealloc {
    [_displayTimer invalidate];
}

- (void)layout {
    [super layout];
    self.videoView.frame = self.bounds;
}

- (void)setSize:(CGSize)size {
    dispatch_async(dispatch_get_main_queue(), ^{
        [self.videoView setSize:size];
    });
}

- (void)renderFrame:(RTCVideoFrame *)frame {
    if (frame && self.owner) {
        self.owner->HandleVideoFrame((__bridge void *)frame);
    }
    @synchronized (self) {
        self.pendingFrame = frame;
        self.hasPendingFrame = frame != nil;
    }
    if (!frame) {
        dispatch_async(dispatch_get_main_queue(), ^{
            [self.videoView renderFrame:nil];
        });
    }
}

- (void)displayTimerFired:(NSTimer *)timer {
    (void)timer;
    RTCVideoFrame *frame = nil;
    @synchronized (self) {
        if (!self.hasPendingFrame) return;
        frame = self.pendingFrame;
        self.pendingFrame = nil;
        self.hasPendingFrame = NO;
    }
    [self.videoView renderFrame:frame];
}

@end

namespace OPN {
static OPNLibWebRTCSessionImpl *OPNImplFromOpaque(void *opaque) {
    return (__bridge OPNLibWebRTCSessionImpl *)opaque;
}

static bool OPNLibWebRTCSupportsCodec(RTCPeerConnectionFactory *factory, const std::string &normalizedCodec) {
    if (!factory || !OPNIsSupportedCodecPreference(normalizedCodec)) return false;
    RTCRtpCapabilities *capabilities = [factory rtpReceiverCapabilitiesForKind:kRTCMediaStreamTrackKindVideo];
    if (!capabilities) return false;

    NSMutableArray<NSString *> *codecNames = [NSMutableArray array];
    for (RTCRtpCodecCapability *codec in capabilities.codecs) {
        NSString *name = codec.name ?: @"";
        NSString *mimeType = codec.mimeType ?: @"";
        NSString *combined = [[NSString stringWithFormat:@"%@ %@", name, mimeType] uppercaseString];
        if (combined.length > 1) [codecNames addObject:combined];
        if (OPNCodecCapabilityMatches(codec, normalizedCodec)) return true;
    }

    NSLog(@"[LibWebRTC] Receiver codec capabilities do not include %s; available=%@", normalizedCodec.c_str(), [codecNames componentsJoinedByString:@", "]);
    return false;
}

static bool OPNLibWebRTCH265ReceiverSupport(RTCPeerConnectionFactory *factory,
                                            int &maxMainLevelId,
                                            int &maxMain10LevelId,
                                            bool &supportsHighTier) {
    maxMainLevelId = 0;
    maxMain10LevelId = 0;
    supportsHighTier = false;
    if (!factory) return false;

    RTCRtpCapabilities *capabilities = [factory rtpReceiverCapabilitiesForKind:kRTCMediaStreamTrackKindVideo];
    if (!capabilities) return false;

    bool hasH265 = false;
    for (RTCRtpCodecCapability *codec in capabilities.codecs) {
        if (!OPNCodecCapabilityMatches(codec, "H265")) continue;
        hasH265 = true;
        NSDictionary<NSString *, NSString *> *parameters = codec.parameters ?: @{};
        NSString *tierFlag = parameters[@"tier-flag"];
        if (tierFlag && tierFlag.integerValue == 1) supportsHighTier = true;

        NSInteger profileId = parameters[@"profile-id"].integerValue;
        NSInteger levelId = parameters[@"level-id"].integerValue;
        if (levelId <= 0) continue;
        if (profileId == 2) {
            maxMain10LevelId = std::max(maxMain10LevelId, (int)levelId);
        } else {
            maxMainLevelId = std::max(maxMainLevelId, (int)levelId);
        }
    }
    return hasH265;
}

static bool OPNApplyVideoCodecPreference(RTCPeerConnectionFactory *factory,
                                         RTCPeerConnection *peerConnection,
                                         const std::string &normalizedCodec) {
    if (!factory || !peerConnection || !OPNIsSupportedCodecPreference(normalizedCodec)) return false;

    RTCRtpCapabilities *capabilities = [factory rtpReceiverCapabilitiesForKind:kRTCMediaStreamTrackKindVideo];
    if (!capabilities) return false;

    NSMutableArray<RTCRtpCodecCapability *> *preferredCodecs = [NSMutableArray array];
    for (RTCRtpCodecCapability *codec in capabilities.codecs) {
        if (OPNCodecCapabilityMatches(codec, normalizedCodec)) {
            [preferredCodecs addObject:codec];
        }
    }
    if (preferredCodecs.count == 0) return false;
    for (RTCRtpCodecCapability *codec in capabilities.codecs) {
        if (OPNCodecCapabilityIsTransportSupport(codec)) {
            [preferredCodecs addObject:codec];
        }
    }

    bool applied = false;
    for (RTCRtpTransceiver *transceiver in peerConnection.transceivers) {
        if (transceiver.mediaType != RTCRtpMediaTypeVideo || transceiver.isStopped) continue;
        NSError *codecError = nil;
        if ([transceiver setCodecPreferences:preferredCodecs error:&codecError]) {
            applied = true;
            NSLog(@"[LibWebRTC] Applied %s codec preference to video transceiver mid=%@ (%lu codecs)",
                  normalizedCodec.c_str(),
                  transceiver.mid ?: @"(none)",
                  (unsigned long)preferredCodecs.count);
        } else {
            NSLog(@"[LibWebRTC] Failed to apply %s codec preference to video transceiver mid=%@: %@",
                  normalizedCodec.c_str(),
                  transceiver.mid ?: @"(none)",
                  codecError.localizedDescription ?: @"unknown error");
        }
    }
    return applied;
}

static NSNumber *OPNRTCStatsNumberForKey(NSDictionary<NSString *, NSObject *> *values, NSString *key) {
    NSObject *value = values[key];
    return [value isKindOfClass:NSNumber.class] ? (NSNumber *)value : nil;
}

static NSString *OPNRTCStatsStringForKey(NSDictionary<NSString *, NSObject *> *values, NSString *key) {
    NSObject *value = values[key];
    return [value isKindOfClass:NSString.class] ? (NSString *)value : nil;
}

static bool OPNRTCStatsIsAudio(RTCStatistics *stat) {
    NSString *mediaType = OPNRTCStatsStringForKey(stat.values, @"mediaType");
    NSString *kind = OPNRTCStatsStringForKey(stat.values, @"kind");
    NSString *trackKind = OPNRTCStatsStringForKey(stat.values, @"trackKind");
    if ([mediaType isEqualToString:@"audio"] || [kind isEqualToString:@"audio"] || [trackKind isEqualToString:@"audio"]) return true;
    NSString *idString = [stat.id lowercaseString];
    return [idString containsString:@"audio"] || [idString containsString:@"mic"];
}

static double OPNMicrophoneLevelFromStatsReport(RTCStatisticsReport *report) {
    double bestLevel = -1.0;
    for (RTCStatistics *stat in report.statistics.allValues) {
        if (!OPNRTCStatsIsAudio(stat)) continue;
        NSNumber *audioLevel = OPNRTCStatsNumberForKey(stat.values, @"audioLevel");
        if (!audioLevel) audioLevel = OPNRTCStatsNumberForKey(stat.values, @"totalAudioEnergy");
        if (!audioLevel) continue;
        double level = audioLevel.doubleValue;
        if (level > 1.0) level = sqrt(level);
        bestLevel = std::max(bestLevel, std::max(0.0, std::min(level, 1.0)));
    }
    return bestLevel;
}

static const char *OPNRTCRtpTransceiverDirectionName(RTCRtpTransceiverDirection direction) {
    switch (direction) {
        case RTCRtpTransceiverDirectionSendRecv: return "sendrecv";
        case RTCRtpTransceiverDirectionSendOnly: return "sendonly";
        case RTCRtpTransceiverDirectionRecvOnly: return "recvonly";
        case RTCRtpTransceiverDirectionInactive: return "inactive";
        case RTCRtpTransceiverDirectionStopped: return "stopped";
    }
    return "unknown";
}

static RTCRtpTransceiver *OPNFindMicrophoneTransceiver(RTCPeerConnection *peerConnection) {
    RTCRtpTransceiver *firstAvailableAudio = nil;
    RTCRtpTransceiver *firstSendableAudio = nil;
    for (RTCRtpTransceiver *transceiver in peerConnection.transceivers) {
        if (transceiver.mediaType != RTCRtpMediaTypeAudio || transceiver.isStopped) continue;
        if ([transceiver.mid isEqualToString:@"3"]) return transceiver;
        if (!firstAvailableAudio && !transceiver.sender.track) firstAvailableAudio = transceiver;
        if (!firstSendableAudio &&
            (transceiver.direction == RTCRtpTransceiverDirectionSendRecv ||
             transceiver.direction == RTCRtpTransceiverDirectionRecvOnly ||
             transceiver.direction == RTCRtpTransceiverDirectionInactive)) {
            firstSendableAudio = transceiver;
        }
    }
    return firstAvailableAudio ?: firstSendableAudio;
}

static bool OPNAttachMicrophoneTrack(OPNLibWebRTCSessionImpl *impl, RTCAudioTrack *audioTrack) {
    if (!impl.peerConnection || !audioTrack) return false;

    RTCRtpTransceiver *transceiver = OPNFindMicrophoneTransceiver(impl.peerConnection);
    if (transceiver) {
        NSError *directionError = nil;
        RTCRtpTransceiverDirection targetDirection = transceiver.direction;
        if (transceiver.direction == RTCRtpTransceiverDirectionRecvOnly) {
            targetDirection = RTCRtpTransceiverDirectionSendRecv;
        } else if (transceiver.direction == RTCRtpTransceiverDirectionInactive) {
            targetDirection = RTCRtpTransceiverDirectionSendOnly;
        }
        if (targetDirection != transceiver.direction) {
            [transceiver setDirection:targetDirection error:&directionError];
            if (directionError) {
                NSLog(@"[LibWebRTC] failed to set microphone transceiver direction: %@", directionError.localizedDescription);
            }
        }
        transceiver.sender.track = audioTrack;
        transceiver.sender.streamIds = @[@"mic"];
        impl.localMicrophoneSender = transceiver.sender;
        NSLog(@"[LibWebRTC] local microphone track attached to transceiver mid=%@ direction=%s target=%s enabled=%d volume=%.2f",
              transceiver.mid ?: @"(none)",
              OPNRTCRtpTransceiverDirectionName(transceiver.direction),
              OPNRTCRtpTransceiverDirectionName(targetDirection),
              audioTrack.isEnabled,
              audioTrack.source.volume);
        return true;
    }

    RTCRtpSender *sender = [impl.peerConnection addTrack:audioTrack streamIds:@[@"mic"]];
    if (!sender) return false;
    impl.localMicrophoneSender = sender;
    NSLog(@"[LibWebRTC] local microphone track added without negotiated transceiver; renegotiation may be required");
    return true;
}
#endif

bool LibWebRTCStreamSession::IsAvailable() {
#if defined(OPN_HAVE_LIBWEBRTC)
    return NSClassFromString(@"RTCPeerConnectionFactory") != nil;
#else
    return false;
#endif
}

std::string LibWebRTCStreamSession::AvailabilityDescription() {
#if defined(OPN_HAVE_LIBWEBRTC)
    return IsAvailable() ? "WebRTC.framework loaded" : "WebRTC.framework linked but RTCPeerConnectionFactory missing";
#else
    return "build without OPN_HAVE_LIBWEBRTC";
#endif
}

LibWebRTCStreamSession::LibWebRTCStreamSession() = default;

LibWebRTCStreamSession::~LibWebRTCStreamSession() {
    Stop();
}

void LibWebRTCStreamSession::Start(const SessionInfo &session,
                                   const std::string &offerSdp,
                                   const StreamSettings &settings,
                                   StreamStateCallback onState) {
    Stop();
    m_settings = settings;
    m_onState = std::move(onState);
    {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        m_latestStats = StreamStats{};
        m_latestStats.gpuType = session.gpuType;
        m_latestStats.zone = session.zone;
        m_latestStats.resolution = settings.resolution;
        m_latestStats.codec = settings.codec;
        m_latestStats.fps = settings.fps;
        m_latestStats.videoDecoder = "libwebrtc";
        m_latestStats.videoSink = OPNEnvFlagEnabled("OPN_ENABLE_PACED_WEBRTC_RENDERER", true)
            ? "OPNPacedVideoRenderer"
            : "RTCMTLNSVideoView";
        m_latestStats.videoPipelineMode = "libwebrtc negotiating";
        m_statsRequestInFlight = false;
        m_previousStatsTimestampMs = 0;
        m_previousBytesReceived = 0;
        m_previousPacketsReceived = 0;
        m_previousFramesDecoded = 0;
        m_previousPacketsLost = 0;
    }
    if (settings.microphoneMode != "disabled" && !m_microphoneEnabled) {
        m_microphoneEnabled = settings.microphoneMode == "voice-activity";
    }

#if defined(OPN_HAVE_LIBWEBRTC)
    if (!IsAvailable()) {
        const std::string error = AvailabilityDescription();
        if (m_onState) m_onState(false, error);
        return;
    }

    auto *impl = [[OPNLibWebRTCSessionImpl alloc] initWithOwner:this];
    impl.factory = [[RTCPeerConnectionFactory alloc] init];

    RTCConfiguration *configuration = [[RTCConfiguration alloc] init];
    NSMutableArray<RTCIceServer *> *iceServers = [NSMutableArray array];
    for (const IceServer &server : session.iceServers) {
        NSMutableArray<NSString *> *urls = [NSMutableArray array];
        for (const std::string &url : server.urls) {
            [urls addObject:OPNStringToNSString(url)];
        }
        if (urls.count == 0) continue;
        RTCIceServer *iceServer = [[RTCIceServer alloc] initWithURLStrings:urls
                                                                  username:server.username.empty() ? nil : OPNStringToNSString(server.username)
                                                                credential:server.credential.empty() ? nil : OPNStringToNSString(server.credential)];
        [iceServers addObject:iceServer];
    }
    configuration.iceServers = iceServers;
    configuration.sdpSemantics = RTCSdpSemanticsUnifiedPlan;
    configuration.bundlePolicy = RTCBundlePolicyMaxBundle;
    configuration.rtcpMuxPolicy = RTCRtcpMuxPolicyRequire;
    configuration.tcpCandidatePolicy = RTCTcpCandidatePolicyDisabled;
    configuration.continualGatheringPolicy = RTCContinualGatheringPolicyGatherOnce;
    configuration.iceConnectionReceivingTimeout = 30000;

    RTCMediaConstraints *constraints = [[RTCMediaConstraints alloc] initWithMandatoryConstraints:nil optionalConstraints:nil];
    impl.peerConnection = [impl.factory peerConnectionWithConfiguration:configuration constraints:constraints delegate:impl];
    if (!impl.peerConnection) {
        const std::string error = "failed to create libwebrtc peer connection";
        if (m_onState) m_onState(false, error);
        return;
    }

    m_impl = (__bridge_retained void *)impl;
    StartAudioDeviceMonitoring();
    CreateInputChannel();

    std::string processedOfferSdp = offerSdp;
    if (offerSdp.find("0.0.0.0") != std::string::npos) {
        std::string mediaIp = OPNExtractPublicIp(!session.mediaConnectionInfo.ip.empty() ? session.mediaConnectionInfo.ip : session.serverIp);
        NSLog(@"[LibWebRTC] Offer contains 0.0.0.0 placeholders; leaving SDP unchanged for native parser compatibility (mediaIp=%s)",
              mediaIp.empty() ? "unknown" : mediaIp.c_str());
    }
    std::string requestedCodec = OPNNormalizeCodec(settings.codec);
    bool requestedCodecSupported = OPNLibWebRTCSupportsCodec(impl.factory, requestedCodec);
    if (requestedCodec == "H265" && requestedCodecSupported && OPNEnvFlagEnabled("OPN_ENABLE_LIBWEBRTC_H265_OFFER_REWRITE", true)) {
        int maxMainLevelId = 0;
        int maxMain10LevelId = 0;
        bool supportsHighTier = false;
        if (OPNLibWebRTCH265ReceiverSupport(impl.factory, maxMainLevelId, maxMain10LevelId, supportsHighTier)) {
            processedOfferSdp = OPNRewriteH265OfferForReceiver(processedOfferSdp, maxMainLevelId, maxMain10LevelId, supportsHighTier);
        }
    } else if (requestedCodec == "H265" && requestedCodecSupported) {
        NSLog(@"[LibWebRTC] OPN_ENABLE_LIBWEBRTC_H265_OFFER_REWRITE=0; retaining original H265 offer parameters");
    }
    if (OPNIsSupportedCodecPreference(requestedCodec) && requestedCodecSupported && OPNEnvFlagEnabled("OPN_ENABLE_LIBWEBRTC_CODEC_FILTER", false)) {
        processedOfferSdp = OPNPreferCodecInOffer(processedOfferSdp, requestedCodec);
    } else if (OPNIsSupportedCodecPreference(requestedCodec) && !requestedCodecSupported) {
        NSLog(@"[LibWebRTC] Requested codec %s is not supported by this WebRTC.framework; retaining full offer so libwebrtc can negotiate a supported fallback", requestedCodec.c_str());
    } else if (OPNIsSupportedCodecPreference(requestedCodec)) {
        NSLog(@"[LibWebRTC] OPN_ENABLE_LIBWEBRTC_CODEC_FILTER=0; retaining all video payloads for requested codec %s", requestedCodec.c_str());
    } else {
        NSLog(@"[LibWebRTC] Unsupported requested codec preference '%s'; retaining all video payloads", settings.codec.c_str());
    }
    OPNLogVideoSdpSummary("offer-video", processedOfferSdp);

    __weak OPNLibWebRTCSessionImpl *weakImpl = impl;
    NSString *processedOfferString = OPNStringToNSString(processedOfferSdp);
    NSString *originalOfferString = OPNStringToNSString(offerSdp);
    const bool canRetryOriginalOffer = processedOfferSdp != offerSdp;
    void (^handleRemoteDescriptionSet)(void) = ^{
        OPNLibWebRTCSessionImpl *strongImpl = weakImpl;
        if (!strongImpl) return;

        std::string answerCodecPreference = OPNNormalizeCodec(this->m_settings.codec);
        if (OPNIsSupportedCodecPreference(answerCodecPreference)) {
            if (!OPNApplyVideoCodecPreference(strongImpl.factory, strongImpl.peerConnection, answerCodecPreference)) {
                NSLog(@"[LibWebRTC] No video transceiver accepted %s codec preference before answer", answerCodecPreference.c_str());
            }
        }

        if (this->m_settings.microphoneMode != "disabled" && !strongImpl.localMicrophoneTrack) {
            RTCMediaConstraints *audioConstraints = [[RTCMediaConstraints alloc] initWithMandatoryConstraints:nil optionalConstraints:nil];
            RTCAudioSource *audioSource = [strongImpl.factory audioSourceWithConstraints:audioConstraints];
            audioSource.volume = this->m_microphoneVolumeLevel;
            RTCAudioTrack *audioTrack = [strongImpl.factory audioTrackWithSource:audioSource trackId:@"opennow-microphone"];
            audioTrack.isEnabled = this->m_microphoneEnabled;
            if (OPNAttachMicrophoneTrack(strongImpl, audioTrack)) {
                strongImpl.localMicrophoneTrack = audioTrack;
                this->StartMicrophoneLevelPolling();
            } else {
                NSLog(@"[LibWebRTC] failed to attach local microphone track");
            }
        }

        RTCMediaConstraints *answerConstraints = [[RTCMediaConstraints alloc] initWithMandatoryConstraints:nil optionalConstraints:nil];
        [strongImpl.peerConnection answerForConstraints:answerConstraints completionHandler:^(RTCSessionDescription *answer, NSError *answerError) {
            OPNLibWebRTCSessionImpl *answerImpl = weakImpl;
            if (!answerImpl) return;
            if (answerError || !answer) {
                const std::string message = "createAnswer failed: " + OPNNSStringToString(answerError.localizedDescription);
                this->HandleConnectionState(false, message);
                return;
            }

            const std::string rawAnswerSdp = OPNNSStringToString(answer.sdp);
            OPNLogVideoSdpSummary("answer-raw-video", rawAnswerSdp);
            const bool enableAnswerMunging = OPNEnvFlagEnabled("OPN_ENABLE_LIBWEBRTC_ANSWER_MUNGE", false);
            const std::string localAnswerSdp = enableAnswerMunging
                ? OPNMungeAnswerSdp(rawAnswerSdp, std::max(1000, this->m_settings.maxBitrateMbps * 1000))
                : rawAnswerSdp;
            if (!enableAnswerMunging) {
                NSLog(@"[LibWebRTC] OPN_ENABLE_LIBWEBRTC_ANSWER_MUNGE=0; using raw local answer SDP");
            }
            OPNLogVideoSdpSummary("answer-video", localAnswerSdp);
            if (!OPNVideoSdpHasMediaCodec(localAnswerSdp)) {
                const std::string message = "createAnswer produced no negotiated video media codec";
                this->HandleConnectionState(false, message);
                return;
            }
            RTCSessionDescription *localAnswer = [[RTCSessionDescription alloc] initWithType:RTCSdpTypeAnswer sdp:OPNStringToNSString(localAnswerSdp)];

            [answerImpl.peerConnection setLocalDescription:localAnswer completionHandler:^(NSError *localError) {
                if (localError) {
                    const std::string message = "setLocalDescription failed: " + OPNNSStringToString(localError.localizedDescription);
                    this->HandleConnectionState(false, message);
                    return;
                }

                const std::string localSdp = localAnswerSdp;
                SendAnswerRequest request;
                request.sdp = localSdp;
                request.nvstSdp = OPNBuildNvstSdp(this->m_settings, OPNExtractIceCredentials(localSdp));
                {
                    std::lock_guard<std::mutex> lock(this->m_statsMutex);
                    this->m_latestStats.videoPipelineMode = "libwebrtc answer sent";
                }
                if (this->m_onAnswer) this->m_onAnswer(request);
            }];
        }];
    };

    RTCSessionDescription *offer = [[RTCSessionDescription alloc] initWithType:RTCSdpTypeOffer sdp:processedOfferString];
    [impl.peerConnection setRemoteDescription:offer completionHandler:^(NSError *error) {
        OPNLibWebRTCSessionImpl *strongImpl = weakImpl;
        if (!strongImpl) return;
        if (!error) {
            handleRemoteDescriptionSet();
            return;
        }
        if (!canRetryOriginalOffer) {
            const std::string message = "setRemoteDescription failed: " + OPNNSStringToString(error.localizedDescription);
            this->HandleConnectionState(false, message);
            return;
        }

        NSLog(@"[LibWebRTC] filtered offer rejected (%@); retrying original GFN offer", error.localizedDescription);
        RTCSessionDescription *originalOffer = [[RTCSessionDescription alloc] initWithType:RTCSdpTypeOffer sdp:originalOfferString];
        [strongImpl.peerConnection setRemoteDescription:originalOffer completionHandler:^(NSError *retryError) {
            if (retryError) {
                const std::string message = "setRemoteDescription failed: " + OPNNSStringToString(retryError.localizedDescription);
                this->HandleConnectionState(false, message);
                return;
            }
            handleRemoteDescriptionSet();
        }];
    }];
#else
    (void)offerSdp;
    const std::string error = "libwebrtc backend requested in a build without WebRTC.framework";
    if (m_onState) m_onState(false, error);
#endif
}

void LibWebRTCStreamSession::Stop() {
    StopAudioDeviceMonitoring();
    StopStatsPolling();
    StopMicrophoneLevelPolling();
    {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        m_statsRequestInFlight = false;
    }
#if defined(OPN_HAVE_LIBWEBRTC)
    if (m_impl) {
        OPNLibWebRTCSessionImpl *impl = (__bridge_transfer OPNLibWebRTCSessionImpl *)m_impl;
        impl.owner = nullptr;
        impl.reliableInputChannel.delegate = nil;
        impl.partialInputChannel.delegate = nil;
        impl.peerConnection.delegate = nil;
        if (impl.remoteVideoTrack && impl.remoteVideoRenderer) {
            [impl.remoteVideoTrack removeRenderer:impl.remoteVideoRenderer];
        }
        impl.remoteAudioTrack.isEnabled = NO;
        impl.localMicrophoneTrack.isEnabled = NO;
        [impl.remoteVideoView removeFromSuperview];
        [impl.reliableInputChannel close];
        [impl.partialInputChannel close];
        [impl.peerConnection close];
        m_impl = nullptr;
    }
#else
    m_impl = nullptr;
#endif
    StopInputHeartbeat();
    m_inputReady = false;
    m_reliableOpen = false;
    m_partialOpen = false;
}

void LibWebRTCStreamSession::AddRemoteIceCandidate(const IceCandidatePayload &candidate) {
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.peerConnection || candidate.candidate.empty()) return;
    RTCIceCandidate *rtcCandidate = [[RTCIceCandidate alloc] initWithSdp:OPNStringToNSString(candidate.candidate)
                                                           sdpMLineIndex:candidate.sdpMLineIndex
                                                                  sdpMid:candidate.sdpMid.empty() ? nil : OPNStringToNSString(candidate.sdpMid)];
    [impl.peerConnection addIceCandidate:rtcCandidate completionHandler:^(NSError *error) {
        if (error) NSLog(@"[LibWebRTC] addIceCandidate failed: %@", error.localizedDescription);
    }];
#else
    (void)candidate;
#endif
}

void LibWebRTCStreamSession::OnAnswerReady(std::function<void(const SendAnswerRequest &)> cb) {
    m_onAnswer = std::move(cb);
}

void LibWebRTCStreamSession::OnIceCandidateReady(std::function<void(const IceCandidatePayload &)> cb) {
    m_onIceCandidate = std::move(cb);
}

void LibWebRTCStreamSession::SendInput(const uint8_t *data, size_t len) {
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.reliableInputChannel || impl.reliableInputChannel.readyState != RTCDataChannelStateOpen || !data || len == 0) return;
    NSData *payload = [NSData dataWithBytes:data length:len];
    RTCDataBuffer *buffer = [[RTCDataBuffer alloc] initWithData:payload isBinary:YES];
    [impl.reliableInputChannel sendData:buffer];
#else
    (void)data;
    (void)len;
#endif
}

void LibWebRTCStreamSession::SendInputPartiallyReliable(const uint8_t *data, size_t len) {
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.partialInputChannel || impl.partialInputChannel.readyState != RTCDataChannelStateOpen || !data || len == 0) return;
    NSData *payload = [NSData dataWithBytes:data length:len];
    RTCDataBuffer *buffer = [[RTCDataBuffer alloc] initWithData:payload isBinary:YES];
    [impl.partialInputChannel sendData:buffer];
#else
    (void)data;
    (void)len;
#endif
}

void LibWebRTCStreamSession::CreateInputChannel() {
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.peerConnection || impl.reliableInputChannel || impl.partialInputChannel) return;

    RTCDataChannelConfiguration *reliableConfig = [[RTCDataChannelConfiguration alloc] init];
    reliableConfig.isOrdered = YES;
    reliableConfig.maxRetransmits = -1;
    reliableConfig.maxPacketLifeTime = -1;
    impl.reliableInputChannel = [impl.peerConnection dataChannelForLabel:@"input_channel_v1" configuration:reliableConfig];
    impl.reliableInputChannel.delegate = impl;

    RTCDataChannelConfiguration *partialConfig = [[RTCDataChannelConfiguration alloc] init];
    partialConfig.isOrdered = NO;
    partialConfig.maxRetransmits = -1;
    partialConfig.maxPacketLifeTime = OPNPartialReliableInputLifetimeMs;
    impl.partialInputChannel = [impl.peerConnection dataChannelForLabel:@"input_channel_partially_reliable" configuration:partialConfig];
    impl.partialInputChannel.delegate = impl;
#endif
}

bool LibWebRTCStreamSession::InputReady() const {
    return m_inputReady;
}

void LibWebRTCStreamSession::SendKeyEvent(uint16_t keycode, uint16_t scancode, uint16_t modifiers, bool down) {
    Input::KeyboardPayload payload;
    payload.keycode = keycode;
    payload.scancode = scancode;
    payload.modifiers = modifiers;
    payload.timestampUs = Input::TimestampUs();
    const std::vector<uint8_t> encoded = down ? m_inputEncoder.EncodeKeyDown(payload) : m_inputEncoder.EncodeKeyUp(payload);
    SendInput(encoded.data(), encoded.size());
}

void LibWebRTCStreamSession::SendMouseMove(int16_t dx, int16_t dy) {
    Input::MouseMovePayload payload;
    payload.dx = dx;
    payload.dy = dy;
    payload.timestampUs = Input::TimestampUs();
    const std::vector<uint8_t> encoded = m_inputEncoder.EncodeMouseMove(payload);
    SendInputPartiallyReliable(encoded.data(), encoded.size());
}

void LibWebRTCStreamSession::SendMouseButton(uint8_t button, bool down) {
    Input::MouseButtonPayload payload;
    payload.button = button;
    payload.timestampUs = Input::TimestampUs();
    const std::vector<uint8_t> encoded = down ? m_inputEncoder.EncodeMouseButtonDown(payload) : m_inputEncoder.EncodeMouseButtonUp(payload);
    SendInput(encoded.data(), encoded.size());
}

void LibWebRTCStreamSession::SendMouseWheel(int16_t delta) {
    Input::MouseWheelPayload payload;
    payload.delta = delta;
    payload.timestampUs = Input::TimestampUs();
    const std::vector<uint8_t> encoded = m_inputEncoder.EncodeMouseWheel(payload);
    SendInput(encoded.data(), encoded.size());
}

void LibWebRTCStreamSession::SendGamepadState(const Input::GamepadState &state, uint16_t bitmap) {
    const std::vector<uint8_t> encoded = m_inputEncoder.EncodeGamepadState(state, bitmap, true);
    SendInputPartiallyReliable(encoded.data(), encoded.size());
}

void LibWebRTCStreamSession::SetMicrophoneEnabled(bool enabled) {
    m_microphoneEnabled = enabled;
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (impl.localMicrophoneTrack) {
        impl.localMicrophoneTrack.isEnabled = enabled ? YES : NO;
    }
    if (enabled && impl.localMicrophoneTrack) {
        StartMicrophoneLevelPolling();
    } else if (!enabled && m_onMicrophoneLevel) {
        m_onMicrophoneLevel(0.0);
    }
#endif
}

void LibWebRTCStreamSession::SetGameVolume(double volume) {
    m_gameVolume = std::max(0.0, std::min(volume, 1.0));
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (impl.remoteAudioTrack) {
        impl.remoteAudioTrack.source.volume = m_gameVolume;
    }
#endif
}

void LibWebRTCStreamSession::SetMicrophoneVolume(double volume) {
    m_microphoneVolumeLevel = std::max(0.0, std::min(volume, 1.0));
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (impl.localMicrophoneTrack) {
        impl.localMicrophoneTrack.source.volume = m_microphoneVolumeLevel;
    }
#endif
}

void LibWebRTCStreamSession::SetMaxBitrateMbps(int mbps) {
    int clampedMbps = std::max(1, std::min(mbps, 250));
    m_settings.maxBitrateMbps = clampedMbps;
    {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        m_latestStats.videoPipelineMode = "libwebrtc bitrate " + std::to_string(clampedMbps) + " Mbps";
    }
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.peerConnection) return;
    NSNumber *maxBitrateBps = @(clampedMbps * 1000 * 1000);
    NSNumber *currentBitrateBps = @(std::max(1, clampedMbps * 7 / 10) * 1000 * 1000);
    NSNumber *minBitrateBps = @(std::max(1, clampedMbps * 35 / 100) * 1000 * 1000);
    BOOL ok = [impl.peerConnection setBweMinBitrateBps:minBitrateBps
                                      currentBitrateBps:currentBitrateBps
                                          maxBitrateBps:maxBitrateBps];
    NSLog(@"[LibWebRTC] Runtime bitrate limit %d Mbps applied=%d", clampedMbps, ok);
#endif
}

void LibWebRTCStreamSession::OnMicrophoneLevel(MicrophoneLevelCallback cb) {
    m_onMicrophoneLevel = std::move(cb);
}

void LibWebRTCStreamSession::OnVideoFrame(VideoFrameCallback cb) {
    m_onVideoFrame = std::move(cb);
}

void LibWebRTCStreamSession::HandleVideoFrame(void *frame) {
    if (m_onVideoFrame) m_onVideoFrame(frame);
}

void LibWebRTCStreamSession::RefreshAudioDevices() {
#if defined(OPN_HAVE_LIBWEBRTC)
    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.peerConnection) return;

    Class audioSessionClass = NSClassFromString(@"RTCAudioSession");
    id audioSession = audioSessionClass ? [audioSessionClass performSelector:@selector(sharedInstance)] : nil;
    if (!audioSession) return;

    const BOOL wasManualAudio = [audioSession respondsToSelector:@selector(useManualAudio)] ? ((BOOL (*)(id, SEL))objc_msgSend)(audioSession, @selector(useManualAudio)) : NO;
    const BOOL wasAudioEnabled = [audioSession respondsToSelector:@selector(isAudioEnabled)] ? ((BOOL (*)(id, SEL))objc_msgSend)(audioSession, @selector(isAudioEnabled)) : YES;

    if ([audioSession respondsToSelector:@selector(setUseManualAudio:)]) {
        ((void (*)(id, SEL, BOOL))objc_msgSend)(audioSession, @selector(setUseManualAudio:), YES);
    }
    if ([audioSession respondsToSelector:@selector(setIsAudioEnabled:)]) {
        ((void (*)(id, SEL, BOOL))objc_msgSend)(audioSession, @selector(setIsAudioEnabled:), NO);
    }
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 80 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
        if (!this->m_impl) return;
        id activeAudioSession = audioSessionClass ? [audioSessionClass performSelector:@selector(sharedInstance)] : nil;
        if (!activeAudioSession) return;
        if ([activeAudioSession respondsToSelector:@selector(setIsAudioEnabled:)]) {
            ((void (*)(id, SEL, BOOL))objc_msgSend)(activeAudioSession, @selector(setIsAudioEnabled:), YES);
        }
        if ([activeAudioSession respondsToSelector:@selector(setUseManualAudio:)]) {
            ((void (*)(id, SEL, BOOL))objc_msgSend)(activeAudioSession, @selector(setUseManualAudio:), wasManualAudio);
        }
        if (wasManualAudio && !wasAudioEnabled) {
            ((void (*)(id, SEL, BOOL))objc_msgSend)(activeAudioSession, @selector(setIsAudioEnabled:), NO);
        }
        OPNLibWebRTCSessionImpl *activeImpl = OPNImplFromOpaque(this->m_impl);
        if (activeImpl.remoteAudioTrack) {
            activeImpl.remoteAudioTrack.isEnabled = YES;
            activeImpl.remoteAudioTrack.source.volume = this->m_gameVolume;
        }
        if (activeImpl.localMicrophoneTrack) {
            activeImpl.localMicrophoneTrack.isEnabled = this->m_microphoneEnabled ? YES : NO;
            activeImpl.localMicrophoneTrack.source.volume = this->m_microphoneVolumeLevel;
        }
        NSLog(@"[LibWebRTC] audio device refresh applied input=%u output=%u",
              this->m_defaultInputDevice,
              this->m_defaultOutputDevice);
    });
#endif
}

void LibWebRTCStreamSession::StartAudioDeviceMonitoring() {
    bool expected = false;
    if (!m_audioDeviceMonitoringActive.compare_exchange_strong(expected, true)) return;

    m_defaultInputDevice = OPNDefaultAudioDevice(kAudioHardwarePropertyDefaultInputDevice);
    m_defaultOutputDevice = OPNDefaultAudioDevice(kAudioHardwarePropertyDefaultOutputDevice);

    AudioObjectPropertyAddress devicesAddress = {
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectPropertyAddress defaultInputAddress = {
        kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectPropertyAddress defaultOutputAddress = {
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };

    OSStatus devicesStatus = AudioObjectAddPropertyListener(kAudioObjectSystemObject, &devicesAddress, OPNAudioDevicesChanged, this);
    OSStatus inputStatus = AudioObjectAddPropertyListener(kAudioObjectSystemObject, &defaultInputAddress, OPNAudioDevicesChanged, this);
    OSStatus outputStatus = AudioObjectAddPropertyListener(kAudioObjectSystemObject, &defaultOutputAddress, OPNAudioDevicesChanged, this);
    NSLog(@"[LibWebRTC] audio device monitoring started devices=%d input=%d output=%d currentInput=%u currentOutput=%u",
          devicesStatus,
          inputStatus,
          outputStatus,
          m_defaultInputDevice,
          m_defaultOutputDevice);
}

void LibWebRTCStreamSession::StopAudioDeviceMonitoring() {
    bool expected = true;
    if (!m_audioDeviceMonitoringActive.compare_exchange_strong(expected, false)) return;

    AudioObjectPropertyAddress devicesAddress = {
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectPropertyAddress defaultInputAddress = {
        kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };
    AudioObjectPropertyAddress defaultOutputAddress = {
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    };

    AudioObjectRemovePropertyListener(kAudioObjectSystemObject, &devicesAddress, OPNAudioDevicesChanged, this);
    AudioObjectRemovePropertyListener(kAudioObjectSystemObject, &defaultInputAddress, OPNAudioDevicesChanged, this);
    AudioObjectRemovePropertyListener(kAudioObjectSystemObject, &defaultOutputAddress, OPNAudioDevicesChanged, this);
    m_defaultInputDevice = kAudioObjectUnknown;
    m_defaultOutputDevice = kAudioObjectUnknown;
    NSLog(@"[LibWebRTC] audio device monitoring stopped");
}

void LibWebRTCStreamSession::HandleAudioDeviceChange() {
    if (!m_audioDeviceMonitoringActive.load()) return;

    const AudioDeviceID inputDevice = OPNDefaultAudioDevice(kAudioHardwarePropertyDefaultInputDevice);
    const AudioDeviceID outputDevice = OPNDefaultAudioDevice(kAudioHardwarePropertyDefaultOutputDevice);
    const bool inputChanged = inputDevice != m_defaultInputDevice;
    const bool outputChanged = outputDevice != m_defaultOutputDevice;
    if (!inputChanged && !outputChanged) return;

    NSLog(@"[LibWebRTC] default audio device changed input=%u->%u output=%u->%u",
          m_defaultInputDevice,
          inputDevice,
          m_defaultOutputDevice,
          outputDevice);
    m_defaultInputDevice = inputDevice;
    m_defaultOutputDevice = outputDevice;
    RefreshAudioDevices();
}

void LibWebRTCStreamSession::StartMicrophoneLevelPolling() {
#if defined(OPN_HAVE_LIBWEBRTC)
    if (m_microphoneLevelTimer) return;
    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (!timer) return;

    m_microphoneLevelTimer = (__bridge_retained void *)timer;
    dispatch_source_set_timer(timer,
                              dispatch_time(DISPATCH_TIME_NOW, 0),
                              100 * NSEC_PER_MSEC,
                              20 * NSEC_PER_MSEC);
    dispatch_source_set_event_handler(timer, ^{
        OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(this->m_impl);
        if (!impl.peerConnection || !impl.localMicrophoneTrack) return;
        if (!this->m_microphoneEnabled || !impl.localMicrophoneTrack.isEnabled) {
            if (this->m_onMicrophoneLevel) this->m_onMicrophoneLevel(0.0);
            return;
        }
        if (this->m_microphoneLevelRequestInFlight) return;
        this->m_microphoneLevelRequestInFlight = true;
        [impl.peerConnection statisticsWithCompletionHandler:^(RTCStatisticsReport *report) {
            this->HandleMicrophoneLevelReport((__bridge void *)report);
        }];
    });
    dispatch_resume(timer);
    NSLog(@"[LibWebRTC] microphone level polling started");
#endif
}

void LibWebRTCStreamSession::StopMicrophoneLevelPolling() {
    if (!m_microphoneLevelTimer) return;
    dispatch_source_t timer = (__bridge_transfer dispatch_source_t)m_microphoneLevelTimer;
    m_microphoneLevelTimer = nullptr;
    dispatch_source_cancel(timer);
    m_microphoneLevelRequestInFlight = false;
    if (m_onMicrophoneLevel) m_onMicrophoneLevel(0.0);
}

void LibWebRTCStreamSession::RequestStats() {
#if defined(OPN_HAVE_LIBWEBRTC)
    if (!OPNEnvFlagEnabled("OPN_ENABLE_WEBRTC_STATS", true)) {
        return;
    }

    OPNLibWebRTCSessionImpl *impl = OPNImplFromOpaque(m_impl);
    if (!impl.peerConnection) return;
    {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        if (m_statsRequestInFlight) return;
        m_statsRequestInFlight = true;
    }

    [impl.peerConnection statisticsWithCompletionHandler:^(RTCStatisticsReport *report) {
        this->HandleStatsReport((__bridge void *)report);
    }];
#endif
}

void LibWebRTCStreamSession::StartStatsPolling() {
#if defined(OPN_HAVE_LIBWEBRTC)
    if (m_statsTimer) return;
    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (!timer) return;

    m_statsTimer = (__bridge_retained void *)timer;
    dispatch_source_set_timer(timer,
                              dispatch_time(DISPATCH_TIME_NOW, 0),
                              500 * NSEC_PER_MSEC,
                              50 * NSEC_PER_MSEC);
    dispatch_source_set_event_handler(timer, ^{
        this->RequestStats();
    });
    dispatch_resume(timer);
    NSLog(@"[LibWebRTC] stats polling started");
#endif
}

void LibWebRTCStreamSession::StopStatsPolling() {
    if (!m_statsTimer) return;
    dispatch_source_t timer = (__bridge_transfer dispatch_source_t)m_statsTimer;
    m_statsTimer = nullptr;
    dispatch_source_cancel(timer);
    std::lock_guard<std::mutex> lock(m_statsMutex);
    m_statsRequestInFlight = false;
}

StreamStats LibWebRTCStreamSession::GetLatestStats() const {
    std::lock_guard<std::mutex> lock(m_statsMutex);
    return m_latestStats;
}

void *LibWebRTCStreamSession::NativeWindowHandle() const {
    return m_nativeWindow;
}

void LibWebRTCStreamSession::SetNativeWindow(void *wnd) {
    m_nativeWindow = wnd;
}

void LibWebRTCStreamSession::HandleLocalIceCandidate(const IceCandidatePayload &candidate) {
    if (m_onIceCandidate) {
        m_onIceCandidate(candidate);
    }
}

void LibWebRTCStreamSession::HandleConnectionState(bool connected, const std::string &error) {
    if (connected) {
        {
            std::lock_guard<std::mutex> lock(m_statsMutex);
            m_latestStats.available = true;
            m_latestStats.videoPipelineMode = "libwebrtc connected";
        }
        StartStatsPolling();
    } else {
        StopStatsPolling();
    }
    if (m_onState) {
        m_onState(connected, error);
    }
}

void LibWebRTCStreamSession::HandleStatsReport(void *report) {
#if defined(OPN_HAVE_LIBWEBRTC)
    RTCStatisticsReport *statsReport = (__bridge RTCStatisticsReport *)report;
    if (!statsReport) {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        m_statsRequestInFlight = false;
        return;
    }

    auto statIsVideo = [&](RTCStatistics *stat) -> bool {
        NSString *mediaType = OPNRTCStatsStringForKey(stat.values, @"mediaType");
        NSString *kind = OPNRTCStatsStringForKey(stat.values, @"kind");
        NSString *trackKind = OPNRTCStatsStringForKey(stat.values, @"trackKind");
        if ([mediaType isEqualToString:@"video"] || [kind isEqualToString:@"video"] || [trackKind isEqualToString:@"video"]) return true;
        return OPNRTCStatsNumberForKey(stat.values, @"framesDecoded") || OPNRTCStatsNumberForKey(stat.values, @"framesReceived");
    };

    std::unordered_map<std::string, std::string> codecs;
    for (RTCStatistics *stat in statsReport.statistics.allValues) {
        if (![stat.type isEqualToString:@"codec"]) continue;
        NSString *mimeType = OPNRTCStatsStringForKey(stat.values, @"mimeType");
        if (mimeType.length == 0) continue;
        codecs[OPNNSStringToString(stat.id)] = OPNNSStringToString(mimeType);
    }

    StreamStats parsed;
    {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        parsed = m_latestStats;
    }
    parsed.available = false;
    parsed.latencyMs = -1.0;
    parsed.jitterMs = -1.0;
    parsed.inboundBitrateMbps = -1.0;
    parsed.packetLossPercent = -1.0;
    parsed.decodeTimeMs = -1.0;
    parsed.renderFps = -1.0;
    parsed.bytesReceived = 0;
    parsed.packetsReceived = 0;
    parsed.packetsLost = 0;
    parsed.framesReceived = 0;
    parsed.framesDecoded = 0;
    parsed.framesDropped = 0;
    parsed.timestampMs = OPNMonotonicMs();
    parsed.videoDecoder = "libwebrtc";
    if (parsed.videoSink.empty()) {
        parsed.videoSink = OPNEnvFlagEnabled("OPN_ENABLE_PACED_WEBRTC_RENDERER", true)
            ? "OPNPacedVideoRenderer"
            : "RTCMTLNSVideoView";
    }

    std::string inboundCodecId;
    uint64_t selectedVideoScore = 0;
    for (RTCStatistics *stat in statsReport.statistics.allValues) {
        if ([stat.type isEqualToString:@"candidate-pair"]) {
            NSNumber *nominated = OPNRTCStatsNumberForKey(stat.values, @"nominated");
            NSString *state = OPNRTCStatsStringForKey(stat.values, @"state");
            NSNumber *rtt = OPNRTCStatsNumberForKey(stat.values, @"currentRoundTripTime") ?: OPNRTCStatsNumberForKey(stat.values, @"roundTripTime");
            if ((!nominated || nominated.boolValue) && (!state || [state isEqualToString:@"succeeded"]) && rtt) {
                parsed.latencyMs = OPNStatsSecondsToMs(rtt.doubleValue);
                parsed.available = true;
            }
            continue;
        }

        if (![stat.type isEqualToString:@"inbound-rtp"] || !statIsVideo(stat)) continue;

        NSNumber *jitter = OPNRTCStatsNumberForKey(stat.values, @"jitter");
        NSNumber *packetsReceived = OPNRTCStatsNumberForKey(stat.values, @"packetsReceived");
        NSNumber *packetsLost = OPNRTCStatsNumberForKey(stat.values, @"packetsLost");
        NSNumber *bytesReceived = OPNRTCStatsNumberForKey(stat.values, @"bytesReceived");
        NSNumber *framesReceived = OPNRTCStatsNumberForKey(stat.values, @"framesReceived");
        NSNumber *framesDecoded = OPNRTCStatsNumberForKey(stat.values, @"framesDecoded");
        NSNumber *framesDropped = OPNRTCStatsNumberForKey(stat.values, @"framesDropped");
        NSNumber *framesPerSecond = OPNRTCStatsNumberForKey(stat.values, @"framesPerSecond");
        NSNumber *totalDecodeTime = OPNRTCStatsNumberForKey(stat.values, @"totalDecodeTime");
        NSString *codecId = OPNRTCStatsStringForKey(stat.values, @"codecId");

        uint64_t videoScore = bytesReceived ? bytesReceived.unsignedLongLongValue : 0;
        if (videoScore == 0 && framesDecoded) videoScore = framesDecoded.unsignedLongLongValue;
        if (videoScore == 0 && framesReceived) videoScore = framesReceived.unsignedLongLongValue;
        if (videoScore < selectedVideoScore) {
            parsed.available = true;
            continue;
        }
        selectedVideoScore = videoScore;

        uint64_t selectedFramesDecoded = framesDecoded ? framesDecoded.unsignedLongLongValue : 0;
        if (jitter) parsed.jitterMs = OPNStatsSecondsToMs(jitter.doubleValue);
        if (packetsReceived) parsed.packetsReceived = packetsReceived.unsignedLongLongValue;
        if (packetsLost) parsed.packetsLost = packetsLost.longLongValue;
        if (bytesReceived) parsed.bytesReceived = bytesReceived.unsignedLongLongValue;
        if (framesReceived) parsed.framesReceived = framesReceived.unsignedLongLongValue;
        if (framesDecoded) parsed.framesDecoded = selectedFramesDecoded;
        if (framesDropped) parsed.framesDropped = framesDropped.unsignedLongLongValue;
        if (framesPerSecond && framesPerSecond.doubleValue > 0) parsed.renderFps = framesPerSecond.doubleValue;
        if (totalDecodeTime && totalDecodeTime.doubleValue > 0 && selectedFramesDecoded > 0) {
            parsed.decodeTimeMs = OPNStatsSecondsToMs(totalDecodeTime.doubleValue) / (double)selectedFramesDecoded;
        }
        if (codecId.length > 0) inboundCodecId = OPNNSStringToString(codecId);
        parsed.available = true;
    }

    if (!inboundCodecId.empty()) {
        auto codec = codecs.find(inboundCodecId);
        parsed.codec = OPNNormalizeStatsCodecName(codec != codecs.end() ? codec->second : inboundCodecId);
    }

    {
        std::lock_guard<std::mutex> lock(m_statsMutex);
        if (parsed.bytesReceived > 0 && m_previousBytesReceived > 0 && parsed.timestampMs > m_previousStatsTimestampMs) {
            uint64_t deltaBytes = parsed.bytesReceived >= m_previousBytesReceived ? parsed.bytesReceived - m_previousBytesReceived : 0;
            uint64_t deltaFramesDecoded = parsed.framesDecoded >= m_previousFramesDecoded ? parsed.framesDecoded - m_previousFramesDecoded : 0;
            double deltaSeconds = (double)(parsed.timestampMs - m_previousStatsTimestampMs) / 1000.0;
            if (deltaSeconds > 0.0) {
                parsed.inboundBitrateMbps = ((double)deltaBytes * 8.0) / (deltaSeconds * 1000000.0);
                if (parsed.renderFps < 0.0) parsed.renderFps = (double)deltaFramesDecoded / deltaSeconds;
            }
        }
        if (m_previousPacketsReceived > 0 || m_previousPacketsLost > 0) {
            uint64_t packetsDelta = parsed.packetsReceived >= m_previousPacketsReceived ? parsed.packetsReceived - m_previousPacketsReceived : 0;
            int64_t lostDelta = parsed.packetsLost - m_previousPacketsLost;
            if (packetsDelta > 0) {
                double totalPackets = (double)packetsDelta + (double)lostDelta;
                parsed.packetLossPercent = totalPackets > 0.0 ? ((double)lostDelta * 100.0) / totalPackets : 0.0;
            }
        }
        if (parsed.bytesReceived > 0) {
            m_previousBytesReceived = parsed.bytesReceived;
            m_previousStatsTimestampMs = parsed.timestampMs;
        }
        if (parsed.packetsReceived > 0 || parsed.packetsLost > 0) {
            m_previousPacketsReceived = parsed.packetsReceived;
            m_previousPacketsLost = parsed.packetsLost;
        }
        if (parsed.framesDecoded > 0) {
            m_previousFramesDecoded = parsed.framesDecoded;
        }
        m_latestStats = parsed;
        m_statsRequestInFlight = false;
    }
#else
    (void)report;
#endif
}

void LibWebRTCStreamSession::HandleMicrophoneLevelReport(void *report) {
#if defined(OPN_HAVE_LIBWEBRTC)
    RTCStatisticsReport *statsReport = (__bridge RTCStatisticsReport *)report;
    double level = statsReport ? OPNMicrophoneLevelFromStatsReport(statsReport) : -1.0;
    m_microphoneLevelRequestInFlight = false;
    if (level >= 0.0 && m_onMicrophoneLevel) {
        m_onMicrophoneLevel(level * m_microphoneVolumeLevel);
    }
#else
    (void)report;
#endif
}

void LibWebRTCStreamSession::HandleDataChannelState(const std::string &label, bool open) {
    if (label == "input_channel_v1") {
        m_reliableOpen = open;
    } else if (label == "input_channel_partially_reliable") {
        m_partialOpen = open;
    }
    if (!open) {
        m_inputReady = false;
        StopInputHeartbeat();
    }
}

void LibWebRTCStreamSession::HandleDataChannelMessage(const std::string &label, const uint8_t *data, size_t len) {
    if (label != "input_channel_v1" || !data || len < 2 || m_inputReady) return;

    const uint16_t firstWord = (uint16_t)data[0] | ((uint16_t)data[1] << 8);
    uint16_t version = 2;
    if (firstWord == 526) {
        if (len >= 4) version = (uint16_t)data[2] | ((uint16_t)data[3] << 8);
        NSLog(@"[LibWebRTC] input handshake detected firstWord=526 version=%u", version);
    } else if (data[0] == 0x0e) {
        version = firstWord;
        NSLog(@"[LibWebRTC] input handshake detected byte[0]=0x0e version=%u", version);
    } else {
        NSLog(@"[LibWebRTC] input channel message before handshake len=%zu firstWord=0x%04x", len, firstWord);
        return;
    }

    m_inputEncoder.SetProtocolVersion(version);
    m_inputReady = m_reliableOpen && m_partialOpen;
    SendInput(data, len);
    StartInputHeartbeat();
    NSLog(@"[LibWebRTC] input handshake complete protocol=v%u inputReady=%d", version, m_inputReady);
}

double LibWebRTCStreamSession::GameVolume() const {
    return m_gameVolume;
}

int LibWebRTCStreamSession::TargetFps() const {
    StreamPreferenceProfile profile = LoadStreamPreferenceProfile();
    return profile.rendererPacingFps > 0 ? profile.rendererPacingFps : (m_settings.fps > 0 ? m_settings.fps : 60);
}

void LibWebRTCStreamSession::SetVideoRendererState(const std::string &sink, const std::string &pipelineMode) {
    std::lock_guard<std::mutex> lock(m_statsMutex);
    m_latestStats.videoSink = sink;
    if (!pipelineMode.empty()) {
        m_latestStats.videoPipelineMode = pipelineMode;
    }
}

void LibWebRTCStreamSession::StartInputHeartbeat() {
    if (m_inputHeartbeat) return;
    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (!timer) return;
    m_inputHeartbeat = (__bridge_retained void *)timer;
    dispatch_source_set_timer(timer, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), 2 * NSEC_PER_SEC, 100 * NSEC_PER_MSEC);
    dispatch_source_set_event_handler(timer, ^{
        if (!m_inputReady) return;
        std::vector<uint8_t> heartbeat = m_inputEncoder.EncodeHeartbeat();
        SendInput(heartbeat.data(), heartbeat.size());
    });
    dispatch_resume(timer);
}

void LibWebRTCStreamSession::StopInputHeartbeat() {
    if (!m_inputHeartbeat) return;
    dispatch_source_t timer = (__bridge_transfer dispatch_source_t)m_inputHeartbeat;
    dispatch_source_cancel(timer);
    m_inputHeartbeat = nullptr;
}

}

#if defined(OPN_HAVE_LIBWEBRTC)
@implementation OPNLibWebRTCSessionImpl

- (instancetype)initWithOwner:(OPN::LibWebRTCStreamSession *)owner {
    self = [super init];
    if (self) {
        _owner = owner;
    }
    return self;
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didChangeSignalingState:(RTCSignalingState)stateChanged {
    (void)peerConnection;
    NSLog(@"[LibWebRTC] signaling state=%ld", (long)stateChanged);
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didAddStream:(RTCMediaStream *)stream {
    (void)peerConnection;
    (void)stream;
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didRemoveStream:(RTCMediaStream *)stream {
    (void)peerConnection;
    (void)stream;
}

- (void)peerConnectionShouldNegotiate:(RTCPeerConnection *)peerConnection {
    (void)peerConnection;
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didChangeIceConnectionState:(RTCIceConnectionState)newState {
    (void)peerConnection;
    NSLog(@"[LibWebRTC] ICE state=%ld", (long)newState);
    if (!_owner) return;
    if (newState == RTCIceConnectionStateConnected || newState == RTCIceConnectionStateCompleted) {
        _owner->HandleConnectionState(true, "");
    } else if (newState == RTCIceConnectionStateFailed || newState == RTCIceConnectionStateClosed) {
        _owner->HandleConnectionState(false, "libwebrtc ICE failed");
    }
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didChangeIceGatheringState:(RTCIceGatheringState)newState {
    (void)peerConnection;
    NSLog(@"[LibWebRTC] ICE gathering state=%ld", (long)newState);
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didGenerateIceCandidate:(RTCIceCandidate *)candidate {
    (void)peerConnection;
    if (!_owner || !candidate) return;
    OPN::IceCandidatePayload payload;
    payload.candidate = OPN::OPNNSStringToString(candidate.sdp);
    payload.sdpMid = OPN::OPNNSStringToString(candidate.sdpMid);
    payload.sdpMLineIndex = candidate.sdpMLineIndex;
    _owner->HandleLocalIceCandidate(payload);
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didRemoveIceCandidates:(NSArray<RTCIceCandidate *> *)candidates {
    (void)peerConnection;
    (void)candidates;
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didOpenDataChannel:(RTCDataChannel *)dataChannel {
    (void)peerConnection;
    dataChannel.delegate = self;
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didChangeConnectionState:(RTCPeerConnectionState)newState {
    (void)peerConnection;
    NSLog(@"[LibWebRTC] peer state=%ld", (long)newState);
    if (!_owner) return;
    if (newState == RTCPeerConnectionStateConnected) {
        _owner->HandleConnectionState(true, "");
    } else if (newState == RTCPeerConnectionStateFailed || newState == RTCPeerConnectionStateClosed) {
        _owner->HandleConnectionState(false, "libwebrtc peer connection failed");
    }
}

- (void)peerConnection:(RTCPeerConnection *)peerConnection didAddReceiver:(RTCRtpReceiver *)rtpReceiver streams:(NSArray<RTCMediaStream *> *)mediaStreams {
    (void)peerConnection;
    (void)mediaStreams;
    if ([rtpReceiver.track.kind isEqualToString:kRTCMediaStreamTrackKindVideo]) {
        NSLog(@"[LibWebRTC] remote video receiver added: %@", rtpReceiver.track.trackId);
        RTCVideoTrack *videoTrack = (RTCVideoTrack *)rtpReceiver.track;
        dispatch_async(dispatch_get_main_queue(), ^{
            if (!_owner) return;
            NSView *parentView = (__bridge NSView *)_owner->NativeWindowHandle();
            if (!parentView) {
                NSLog(@"[LibWebRTC] Cannot attach remote video: native view is missing");
                return;
            }
            if (![RTCMTLNSVideoView isMetalAvailable]) {
                NSLog(@"[LibWebRTC] Cannot attach remote video: Metal renderer is unavailable");
                return;
            }

            if (self.remoteVideoTrack && self.remoteVideoRenderer) {
                [self.remoteVideoTrack removeRenderer:self.remoteVideoRenderer];
            }
            [self.remoteVideoView removeFromSuperview];

            const bool usePacedRenderer = _owner && OPN::OPNEnvFlagEnabled("OPN_ENABLE_PACED_WEBRTC_RENDERER", true);
            NSView *videoView = nil;
            id<RTCVideoRenderer> videoRenderer = nil;
            if (usePacedRenderer) {
                OPNPacedVideoRenderer *pacedRenderer = [[OPNPacedVideoRenderer alloc] initWithFrame:parentView.bounds targetFps:_owner->TargetFps() owner:_owner];
                videoView = pacedRenderer;
                videoRenderer = pacedRenderer;
                _owner->SetVideoRendererState("OPNPacedVideoRenderer", "libwebrtc paced renderer");
            } else {
                RTCMTLNSVideoView *metalView = [[RTCMTLNSVideoView alloc] initWithFrame:parentView.bounds];
                videoView = metalView;
                videoRenderer = metalView;
                _owner->SetVideoRendererState("RTCMTLNSVideoView", "");
            }
            videoView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
            videoView.wantsLayer = YES;
            videoView.layer.backgroundColor = NSColor.blackColor.CGColor;
            [parentView addSubview:videoView positioned:NSWindowBelow relativeTo:nil];
            [videoTrack addRenderer:videoRenderer];

            self.remoteVideoTrack = videoTrack;
            self.remoteVideoView = videoView;
            self.remoteVideoRenderer = videoRenderer;
            NSLog(@"[LibWebRTC] Remote video renderer attached to native view=%p paced=%d", (__bridge void *)parentView, usePacedRenderer);
        });
    } else if ([rtpReceiver.track.kind isEqualToString:kRTCMediaStreamTrackKindAudio]) {
        RTCAudioTrack *audioTrack = (RTCAudioTrack *)rtpReceiver.track;
        audioTrack.isEnabled = YES;
        audioTrack.source.volume = _owner ? _owner->GameVolume() : 1.0;
        self.remoteAudioTrack = audioTrack;
        NSLog(@"[LibWebRTC] remote audio track enabled: %@ volume=%.2f", audioTrack.trackId, audioTrack.source.volume);
    }
}

- (void)dataChannelDidChangeState:(RTCDataChannel *)dataChannel {
    if (!_owner || !dataChannel) return;
    const bool open = dataChannel.readyState == RTCDataChannelStateOpen;
    _owner->HandleDataChannelState(OPN::OPNNSStringToString(dataChannel.label), open);
    NSLog(@"[LibWebRTC] data channel %@ state=%ld inputReady=%d", dataChannel.label, (long)dataChannel.readyState, _owner->InputReady());
}

- (void)dataChannel:(RTCDataChannel *)dataChannel didReceiveMessageWithBuffer:(RTCDataBuffer *)buffer {
    if (!_owner || !dataChannel || !buffer) return;
    _owner->HandleDataChannelMessage(OPN::OPNNSStringToString(dataChannel.label), static_cast<const uint8_t *>(buffer.data.bytes), buffer.data.length);
}

@end
#endif
