#pragma once

#include <string>
#include <vector>
#include <functional>

namespace OPN {

struct StreamAspectOption {
    std::string label;
    int widthRatio = 16;
    int heightRatio = 9;
};

struct StreamResolutionOption {
    int width = 1920;
    int height = 1080;

    std::string Value() const;
    std::string Label() const;
};

struct StreamRegionOption {
    std::string name;
    std::string url;
    int latencyMs = -1;
    bool automatic = false;

    std::string Label() const;
};

struct StreamCodecOption {
    std::string label;
    std::string value;
};

struct StreamBitrateOption {
    std::string label;
    int mbps = 50;
};

struct StreamColorQualityOption {
    std::string label;
    std::string value;
};

struct StreamMicrophoneModeOption {
    std::string label;
    std::string value;
};

struct StreamMicrophoneDeviceOption {
    std::string label;
    std::string uniqueId;
    bool automatic = false;
};

struct StreamPreferenceProfile {
    int aspectIndex = 1;
    int resolutionIndex = 2;
    int fpsIndex = 1;
    int codecIndex = 0;
    int bitrateIndex = 2;
    int colorQualityIndex = 0;
    int rendererPacingIndex = 1;
    int fps = 60;
    int rendererPacingFps = 60;
    int maxBitrateMbps = 50;
    bool enableL4S = false;
    bool enablePowerSaver = false;
    bool suppressInputWhenInactive = true;
    double gameVolume = 1.0;
    double microphoneVolume = 1.0;
    std::string microphoneMode = "disabled";
    std::string microphoneDeviceId;
    int microphonePushToTalkKeyCode = 9;
    int microphonePushToTalkModifierMask = 0;
    std::string microphonePushToTalkKeyLabel = "V";
    std::string microphonePushToTalkComboLabel = "V";
    StreamAspectOption aspect;
    StreamResolutionOption resolution;
    StreamCodecOption codec;
    StreamBitrateOption bitrate;
    StreamColorQualityOption colorQuality;

    double AspectRatio() const;
};

const std::vector<StreamAspectOption> &StreamAspectOptions();
const std::vector<int> &StreamFpsOptions();
const std::vector<StreamCodecOption> &StreamCodecOptions();
const std::vector<StreamBitrateOption> &StreamBitrateOptions();
const std::vector<StreamColorQualityOption> &StreamColorQualityOptions();
const std::vector<int> &StreamRendererPacingOptions();
const std::vector<StreamMicrophoneModeOption> &StreamMicrophoneModeOptions();
std::vector<StreamMicrophoneDeviceOption> LoadMicrophoneDeviceOptions();
std::vector<StreamResolutionOption> StreamResolutionOptionsForAspect(int aspectIndex);

StreamPreferenceProfile LoadStreamPreferenceProfile();
const char *DefaultStreamingBaseUrl();
std::string LoadSelectedStreamRegionUrl();
std::string LoadSelectedStreamingBaseUrl();
void SaveSelectedStreamRegionUrl(const std::string &url);
std::vector<StreamRegionOption> LoadCachedStreamRegions();
void SaveCachedStreamRegions(const std::vector<StreamRegionOption> &regions);
void FetchStreamRegions(const std::string &token,
                        const std::string &providerStreamingBaseUrl,
                        std::function<void(const std::vector<StreamRegionOption> &regions)> completion);
void SaveStreamAspectIndex(int aspectIndex);
void SaveStreamResolutionIndex(int resolutionIndex);
void SaveStreamFpsIndex(int fpsIndex);
void SaveStreamCodecIndex(int codecIndex);
void SaveStreamBitrateIndex(int bitrateIndex);
void SaveStreamColorQualityIndex(int colorQualityIndex);
void SaveStreamRendererPacingIndex(int rendererPacingIndex);
void SaveStreamL4SEnabled(bool enabled);
void SaveStreamPowerSaverEnabled(bool enabled);
void SaveStreamSuppressInputWhenInactive(bool enabled);
void SaveStreamGameVolume(double volume);
void SaveStreamMicrophoneVolume(double volume);
void SaveStreamMicrophoneMode(const std::string &mode);
void SaveStreamMicrophoneDeviceId(const std::string &deviceId);
void SaveStreamMicrophonePushToTalkKeyCode(int keyCode);
void SaveStreamMicrophonePushToTalkModifierMask(int modifierMask);
std::string StreamMicrophonePushToTalkKeyLabel(int keyCode);
std::string StreamMicrophonePushToTalkComboLabel(int keyCode, int modifierMask);

}
