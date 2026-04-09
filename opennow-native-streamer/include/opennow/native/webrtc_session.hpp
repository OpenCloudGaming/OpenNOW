#pragma once

#include <chrono>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "opennow/native/av1_rtp_depacketizer.hpp"

#if defined(OPENNOW_HAS_LIBDATACHANNEL)
namespace rtc {
class PeerConnection;
class DataChannel;
class Track;
}
#endif

#include "opennow/native/input_protocol.hpp"

namespace opennow::native {

class MediaPipeline;

class WebRtcSession {
 public:
  using EmitJson = std::function<void(const std::string&)>;
  using LogFn = std::function<void(const std::string&)>;
  using InputReadyFn = std::function<void(int protocol_version)>;

  void SetEmitter(EmitJson emitter);
  void SetLogger(LogFn logger);
  void SetMediaPipeline(MediaPipeline* media_pipeline);
  void SetInputReadyCallback(InputReadyFn callback);

  bool ConfigureFromSession(const std::string& session_json, std::string& error);
  bool HandleOffer(const std::string& offer_sdp, std::string& error);
  void AddRemoteIce(const std::string& candidate_json);
  void Disconnect();
  bool SendInputPacket(const InputPacket& packet);

 private:
  void Emit(const std::string& json);
  void Log(const std::string& message) const;
  void EmitState(const std::string& state, const std::string& message, const std::string& detail = "");
  bool EnsurePeerConnection(std::string& error);
  void ConfigurePeerCallbacks();
  void ConfigureTracksFromOffer(const std::string& offer_sdp);
  void ConfigureInputChannels();
  void ConfigureTrackHandlers();
  void TryInjectManualMediaCandidate();
  void HandleReliableInputMessage(const std::vector<std::uint8_t>& bytes);

  EmitJson emitter_;
  LogFn logger_;
  MediaPipeline* media_pipeline_ = nullptr;
  InputReadyFn input_ready_callback_;

  std::string session_id_;
  std::string server_ip_;
  std::string media_connection_ip_;
  int media_connection_port_ = 0;
  std::string preferred_codec_ = "H264";
  std::string color_quality_ = "8bit_420";
  int max_bitrate_kbps_ = 75000;
  int fps_ = 60;
  int partial_reliable_threshold_ms_ = 500;
  int width_ = 1920;
  int height_ = 1080;

  bool answer_sent_ = false;
  bool input_ready_ = false;
  bool media_receive_supported_ = true;
  bool media_failure_emitted_ = false;
  bool manual_media_candidate_injected_ = false;
  int input_protocol_version_ = 2;
  std::string pending_offer_sdp_;
  std::string last_server_ice_ufrag_;
  Av1RtpDepacketizer av1_depacketizer_;

#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  std::shared_ptr<class rtc::PeerConnection> peer_connection_;
  std::shared_ptr<class rtc::DataChannel> reliable_input_channel_;
  std::shared_ptr<class rtc::DataChannel> partial_input_channel_;
  std::shared_ptr<class rtc::DataChannel> control_channel_;
  std::shared_ptr<class rtc::Track> video_track_;
  std::shared_ptr<class rtc::Track> audio_track_;
#endif
};

}  // namespace opennow::native
