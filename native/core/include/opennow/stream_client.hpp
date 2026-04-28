#pragma once

#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "opennow/input_protocol.hpp"
#include "opennow/sdp.hpp"
#include "opennow/types.hpp"

namespace opennow {

struct StreamClientConfig {
  SessionInfo session;
  NvstSdpParams nvst_sdp;
  std::string remote_offer_sdp;
  std::string local_answer_sdp;
  int protocol_version = 2;
};

struct NativeWebRtcDataChannelConfig {
  std::string label;
  bool ordered = true;
  std::optional<int> max_retransmits;
  bool partially_reliable = false;
};

struct NativeIceCandidate {
  std::string candidate;
  std::optional<std::string> sdp_mid;
  std::optional<int> sdp_mline_index;
  std::optional<std::string> username_fragment;
};

struct StreamNegotiationArtifacts {
  std::string remote_offer_sdp;
  std::string local_answer_sdp;
  std::string nvst_sdp;
  IceCredentials ice_credentials;
};

struct NativeWebRtcStartRequest {
  SessionInfo session;
  NvstSdpParams nvst_sdp_params;
  StreamNegotiationArtifacts negotiation;
  std::vector<NativeWebRtcDataChannelConfig> input_channels;
  int protocol_version = 2;
};

struct StreamClientEvent {
  std::string type;
  std::string message;
  std::optional<StreamDiagnostics> diagnostics;
};

using StreamClientEventHandler = std::function<void(const StreamClientEvent&)>;

class NativeWebRtcBackend {
 public:
  virtual ~NativeWebRtcBackend() = default;
  virtual void start(const NativeWebRtcStartRequest& request) = 0;
  virtual void stop() = 0;
  virtual void add_ice_candidate(const NativeIceCandidate& candidate) = 0;
  virtual void send_input(const std::vector<std::uint8_t>& bytes, bool partially_reliable) = 0;
  virtual StreamDiagnostics diagnostics() const = 0;
};

class StreamClient {
 public:
  explicit StreamClient(std::unique_ptr<NativeWebRtcBackend> backend);

  void set_event_handler(StreamClientEventHandler handler);
  void start(const StreamClientConfig& config);
  void stop();
  void add_ice_candidate(const NativeIceCandidate& candidate);
  void send_heartbeat();
  void send_key_down(const KeyboardPayload& payload);
  void send_key_up(const KeyboardPayload& payload);
  void send_mouse_move(const MouseMovePayload& payload);
  void send_mouse_button_down(const MouseButtonPayload& payload);
  void send_mouse_button_up(const MouseButtonPayload& payload);
  void send_mouse_wheel(const MouseWheelPayload& payload);
  void send_gamepad_state(const GamepadInput& payload, std::uint16_t bitmap, bool partially_reliable);
  StreamDiagnostics diagnostics() const;
  const StreamNegotiationArtifacts& negotiation_artifacts() const;

 private:
  NativeWebRtcStartRequest build_start_request(const StreamClientConfig& config) const;
  void send_input_packet(const std::vector<std::uint8_t>& bytes, int input_type);

  std::unique_ptr<NativeWebRtcBackend> backend_;
  StreamClientEventHandler handler_;
  InputEncoder input_encoder_;
  StreamNegotiationArtifacts negotiation_artifacts_;
};

class ProofWebRtcBackend final : public NativeWebRtcBackend {
 public:
  void start(const NativeWebRtcStartRequest& request) override;
  void stop() override;
  void add_ice_candidate(const NativeIceCandidate& candidate) override;
  void send_input(const std::vector<std::uint8_t>& bytes, bool partially_reliable) override;
  StreamDiagnostics diagnostics() const override;

  bool started() const;
  std::size_t sent_packet_count() const;
  std::size_t partially_reliable_packet_count() const;
  std::size_t reliable_packet_count() const;
  std::size_t added_ice_candidate_count() const;
  const NativeWebRtcStartRequest& last_start_request() const;

 private:
  bool started_ = false;
  std::size_t sent_packet_count_ = 0;
  std::size_t partially_reliable_packet_count_ = 0;
  std::size_t reliable_packet_count_ = 0;
  std::size_t sent_byte_count_ = 0;
  std::size_t added_ice_candidate_count_ = 0;
  StreamDiagnostics diagnostics_;
  NativeWebRtcStartRequest last_start_request_;
};

}  // namespace opennow
