#pragma once

#include "OPNStreamSession.h"
#include "OPNInputProtocol.h"
#include <memory>
#include <mutex>
#include <string>
#include <atomic>

namespace OPN {

class LibWebRTCStreamSession final : public IStreamSession {
public:
    LibWebRTCStreamSession();
    ~LibWebRTCStreamSession() override;

    static bool IsAvailable();
    static std::string AvailabilityDescription();

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
    bool InputReady() const override;
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
    void *NativeWindowHandle() const override;
    void SetNativeWindow(void *wnd) override;

    void HandleLocalIceCandidate(const IceCandidatePayload &candidate);
    void HandleConnectionState(bool connected, const std::string &error);
    void HandleDataChannelState(const std::string &label, bool open);
    void HandleDataChannelMessage(const std::string &label, const uint8_t *data, size_t len);
    void HandleAudioDeviceChange();
    void HandleVideoFrame(void *frame);
    double GameVolume() const;
    int TargetFps() const;
    void SetVideoRendererState(const std::string &sink, const std::string &pipelineMode);

private:
    void HandleStatsReport(void *report);
    void HandleMicrophoneLevelReport(void *report);
    void StartStatsPolling();
    void StopStatsPolling();
    void StartInputHeartbeat();
    void StopInputHeartbeat();
    void StartMicrophoneLevelPolling();
    void StopMicrophoneLevelPolling();
    void StartAudioDeviceMonitoring();
    void StopAudioDeviceMonitoring();

    void *m_impl = nullptr;
    void *m_nativeWindow = nullptr;
    void *m_inputHeartbeat = nullptr;
    void *m_statsTimer = nullptr;
    void *m_microphoneLevelTimer = nullptr;
    std::atomic<bool> m_audioDeviceMonitoringActive{false};
    bool m_inputReady = false;
    bool m_reliableOpen = false;
    bool m_partialOpen = false;
    bool m_statsRequestInFlight = false;
    bool m_microphoneLevelRequestInFlight = false;
    bool m_microphoneEnabled = false;
    uint32_t m_defaultInputDevice = 0;
    uint32_t m_defaultOutputDevice = 0;
    double m_gameVolume = 1.0;
    double m_microphoneVolumeLevel = 1.0;
    StreamStats m_latestStats;
    mutable std::mutex m_statsMutex;
    uint64_t m_previousStatsTimestampMs = 0;
    uint64_t m_previousBytesReceived = 0;
    uint64_t m_previousPacketsReceived = 0;
    uint64_t m_previousFramesDecoded = 0;
    int64_t m_previousPacketsLost = 0;
    StreamSettings m_settings;
    Input::Encoder m_inputEncoder;
    std::function<void(const SendAnswerRequest &)> m_onAnswer;
    std::function<void(const IceCandidatePayload &)> m_onIceCandidate;
    StreamStateCallback m_onState;
    MicrophoneLevelCallback m_onMicrophoneLevel;
    VideoFrameCallback m_onVideoFrame;
};

}
