#pragma once

#include <string>
#include <vector>
#include <functional>

namespace OPN {

struct StreamSettings {
    std::string resolution = "1920x1080";
    int fps = 60;
    std::string codec = "H264";
    std::string colorQuality = "8bit_420";
    int maxBitrateMbps = 50;
    bool enableCloudGsync = false;
    bool enableL4S = false;
    bool enableReflex = true;
    std::string microphoneMode = "disabled";
    std::string microphoneDeviceId;
    int microphonePushToTalkKeyCode = 9;
    int microphonePushToTalkModifierMask = 0;
    double gameVolume = 1.0;
    double microphoneVolume = 1.0;
    std::string keyboardLayout = "us";
    std::string gameLanguage = "en_US";
    bool accountLinked = true;
    std::string selectedStore;
};

struct IceServer {
    std::vector<std::string> urls;
    std::string username;
    std::string credential;
};

struct MediaConnectionInfo {
    std::string ip;
    int port = 0;
};

struct NegotiatedStreamProfile {
    std::string resolution;
    int fps = 0;
    std::string codec;
    std::string colorQuality;
    int bitDepth = -1;
    int chromaFormat = -1;
};

struct SessionAdMediaFile {
    std::string mediaFileUrl;
    std::string encodingProfile;
};

struct SessionAdInfo {
    std::string adId;
    int adState = -1;
    std::string adUrl;
    std::string mediaUrl;
    std::vector<SessionAdMediaFile> adMediaFiles;
    std::string clickThroughUrl;
    int adLengthInSeconds = 0;
    int durationMs = 0;
    std::string title;
    std::string description;
};

struct SessionAdState {
    bool isAdsRequired = false;
    bool sessionAdsRequired = false;
    bool isQueuePaused = false;
    bool serverSentEmptyAds = false;
    int gracePeriodSeconds = 0;
    std::string message;
    std::vector<SessionAdInfo> sessionAds;
};

struct SessionInfo {
    std::string sessionId;
    int status = 0;
    int queuePosition = 0;
    int seatSetupStep = 0;
    std::string zone;
    std::string streamingBaseUrl;
    std::string serverIp;
    std::string signalingServer;
    std::string signalingUrl;
    std::string gpuType;
    std::vector<IceServer> iceServers;
    MediaConnectionInfo mediaConnectionInfo;
    NegotiatedStreamProfile negotiatedStreamProfile;
    SessionAdState adState;
    std::string clientId;
    std::string deviceId;
};

struct IceCandidatePayload {
    std::string candidate;
    std::string sdpMid;
    int sdpMLineIndex = 0;
    std::string usernameFragment;
};

struct SendAnswerRequest {
    std::string sdp;
    std::string nvstSdp;
};

using SessionCreateCallback = std::function<void(bool success, const SessionInfo &info, const std::string &error)>;
using SessionPollCallback = std::function<void(bool success, const SessionInfo &info, const std::string &error)>;

}
