#pragma once

#include "OPNStreamTypes.h"
#include "OPNInputProtocol.h"
#include <string>
#include <functional>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <atomic>
#include <vector>

namespace OPN {

class SignalingClient;
struct StreamStatsState;

using StreamStateCallback = std::function<void(bool connected, const std::string &error)>;
using MicrophoneLevelCallback = std::function<void(double level)>;
using VideoFrameCallback = std::function<void(void *frame)>;

struct StreamStats {
    bool available = false;
    double latencyMs = -1.0;
    double jitterMs = -1.0;
    double inboundBitrateMbps = -1.0;
    double packetLossPercent = -1.0;
    double decodeTimeMs = -1.0;
    double renderFps = -1.0;
    uint64_t bytesReceived = 0;
    uint64_t packetsReceived = 0;
    int64_t packetsLost = 0;
    uint64_t framesReceived = 0;
    uint64_t framesDecoded = 0;
    uint64_t framesDropped = 0;
    uint64_t timestampMs = 0;
    std::string gpuType;
    std::string zone;
    std::string resolution;
    std::string codec;
    std::string videoDecoder;
    std::string videoSink;
    std::string videoPipelineMode;
    int fps = 0;
};

class IStreamSession {
public:
    virtual ~IStreamSession() = default;

    virtual void Start(const SessionInfo &session,
                       const std::string &offerSdp,
                       const StreamSettings &settings,
                       StreamStateCallback onState) = 0;
    virtual void Stop() = 0;
    virtual void AddRemoteIceCandidate(const IceCandidatePayload &candidate) = 0;
    virtual void OnAnswerReady(std::function<void(const SendAnswerRequest &)> cb) = 0;
    virtual void OnIceCandidateReady(std::function<void(const IceCandidatePayload &)> cb) = 0;
    virtual void SendInput(const uint8_t *data, size_t len) = 0;
    virtual void SendInputPartiallyReliable(const uint8_t *data, size_t len) = 0;
    virtual void CreateInputChannel() = 0;
    virtual bool InputReady() const = 0;
    virtual void SendKeyEvent(uint16_t keycode, uint16_t scancode, uint16_t modifiers, bool down) = 0;
    virtual void SendMouseMove(int16_t dx, int16_t dy) = 0;
    virtual void SendMouseButton(uint8_t button, bool down) = 0;
    virtual void SendMouseWheel(int16_t delta) = 0;
    virtual void SendGamepadState(const Input::GamepadState &state, uint16_t bitmap) = 0;
    virtual void SetMicrophoneEnabled(bool enabled) = 0;
    virtual void SetGameVolume(double volume) = 0;
    virtual void SetMicrophoneVolume(double volume) = 0;
    virtual void SetMaxBitrateMbps(int mbps) = 0;
    virtual void OnMicrophoneLevel(MicrophoneLevelCallback cb) = 0;
    virtual void OnVideoFrame(VideoFrameCallback cb) = 0;
    virtual void RefreshAudioDevices() = 0;
    virtual void RequestStats() = 0;
    virtual StreamStats GetLatestStats() const = 0;
    virtual void *NativeWindowHandle() const = 0;
    virtual void SetNativeWindow(void *wnd) = 0;
};

class StreamSession final : public IStreamSession {
public:
    StreamSession();
    ~StreamSession() override;

    void Start(const SessionInfo &session,
               const std::string &offerSdp,
               const StreamSettings &settings,
               StreamStateCallback onState) override;

    void Stop() override;

    void AddRemoteIceCandidate(const IceCandidatePayload &candidate) override;
    void OnAnswerReady(std::function<void(const SendAnswerRequest &)> cb) override;
    void OnIceCandidateReady(std::function<void(const IceCandidatePayload &)> cb) override;


    void SendInput(const uint8_t *data, size_t len) override;
    void SendInputPartiallyReliable(const uint8_t *data, size_t len) override;
    void CreateInputChannel() override;
    bool InputReady() const override { return m_inputReady; }
    void SendKeyEvent(uint16_t keycode, uint16_t scancode, uint16_t modifiers, bool down) override;
    void SendMouseMove(int16_t dx, int16_t dy) override;
    void SendMouseButton(uint8_t button, bool down) override;
    void SendMouseWheel(int16_t delta) override;
    void SendGamepadState(const Input::GamepadState &state, uint16_t bitmap) override;
    void SetMicrophoneEnabled(bool enabled) override;
    void SetGameVolume(double volume) override;
    void SetMicrophoneVolume(double volume) override;
    void SetMaxBitrateMbps(int mbps) override;
    void OnMicrophoneLevel(MicrophoneLevelCallback cb) override;
    void OnVideoFrame(VideoFrameCallback cb) override;
    void RefreshAudioDevices() override;
    void RequestStats() override;
    StreamStats GetLatestStats() const override;

    void *NativeWindowHandle() const override { return m_nativeWindow; }
    void SetNativeWindow(void *wnd) override { m_nativeWindow = wnd; }

private:
    void *m_pipeline = nullptr;
    void *m_webrtc = nullptr;
    void *m_videoSink = nullptr;
    void *m_audioSink = nullptr;
    void *m_audioVolume = nullptr;
    void *m_statsPad = nullptr;
    void *m_microphoneSource = nullptr;
    void *m_microphoneSinkPad = nullptr;
    void *m_microphoneVolume = nullptr;
    void *m_reliableInputChannel = nullptr;
    void *m_partialInputChannel = nullptr;
    void *m_nativeWindow = nullptr;
    void *m_answerTimeout = nullptr;
    void *m_mediaTimeout = nullptr;
    void *m_disconnectTimeout = nullptr;
    void *m_videoWatchdog = nullptr;
    void *m_inputHeartbeat = nullptr;
    void *m_startContext = nullptr;
    bool m_dataChannelsCreated = false;
    bool m_dataChannelsAllowed = true;
    bool m_inputReady = false;
    bool m_reliableInputOpen = false;
    bool m_partialInputOpen = false;
    bool m_mediaStarted = false;
    bool m_videoPadLinked = false;
    bool m_playPipelineAfterAnswer = false;
    bool m_connectedReported = false;
    bool m_failureReported = false;
    bool m_answerSent = false;
    bool m_remoteIcePwdSanitized = false;
    bool m_iceEverConnected = false;
    bool m_peerEverConnected = false;
    bool m_microphoneEnabled = false;
    std::atomic<bool> m_audioDeviceListenerInstalled{false};
    double m_gameVolume = 1.0;
    double m_microphoneVolumeLevel = 1.0;
    int m_lastIceConnectionState = -1;
    int m_lastPeerConnectionState = -1;
    int m_lastSignalingState = -1;
    int m_lastIceGatheringState = -1;
    std::atomic<uint64_t> m_lastMediaPacketMs{0};
    std::atomic<uint64_t> m_lastVideoFrameMs{0};
    std::atomic<uint64_t> m_lastVideoWatchdogMediaActiveLogMs{0};
    std::atomic<bool> m_videoWatchdogStarted{false};

    std::function<void(const SendAnswerRequest &)> m_onAnswer;
    std::function<void(const IceCandidatePayload &)> m_onIceCandidate;
    MicrophoneLevelCallback m_onMicrophoneLevel;
    VideoFrameCallback m_onVideoFrame;
    StreamStateCallback m_onState;
    Input::Encoder m_inputEncoder;
    std::shared_ptr<StreamStatsState> m_statsState;
    StreamSettings m_streamSettings;
    std::vector<void *> m_microphoneElements;

    static void OnPadAdded(void *pad, void *userData);
    static void OnIceCandidate(int sdpMLineIndex, const char *candidate, void *userData);
    static void OnDataChannel(void *dataChannel, void *userData);
    static void ConfigureInputChannel(void *session, void *dataChannel, const char *expectedLabel);
    static void HandleInputChannelMessage(void *session, const uint8_t *data, size_t len);
    static void StartInputHeartbeat(void *session);
    static void StopInputHeartbeat(void *session);
    static void SendFinalAnswer(void *ctx, const char *reason);
    static void ReportConnected(void *session, const char *reason);
    static bool StartPipelinePlaying(void *session, const char *reason);
    static void StartMediaTimeout(void *session);
    static void CancelMediaTimeout(void *session);
    static void StartDisconnectGraceTimer(void *session, const std::string &reason);
    static void CancelDisconnectGraceTimer(void *session);
    static void MarkMediaPacket(void *session);
    static bool MediaFlowRecentlyActive(void *session, uint64_t maxAgeMs);
    static void MarkVideoFrame(void *session);
    static bool VideoFlowRecentlyActive(void *session, uint64_t maxAgeMs);
    static void StartVideoWatchdog(void *session);
    static void CancelVideoWatchdog(void *session);
    static bool StartMicrophoneCapture(void *session, const StreamSettings &settings, const std::string &offerSdp);
    static void ReleaseMicrophoneSender(void *session);
    static void StartAudioDeviceMonitoring(void *session);
    static void StopAudioDeviceMonitoring(void *session);
    static void ConnectStateDiagnostics(void *webrtc, void *session);
    static void ReportEnded(void *session, const std::string &reason);
    static void ReportFailure(void *session, const std::string &error);
    static void FinishStartContext(void *ctx);
    static bool IsStartContextActive(void *ctx);
    static void FailStartContext(void *ctx, const std::string &error);
};

}
