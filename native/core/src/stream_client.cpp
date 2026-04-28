#include "opennow/stream_client.hpp"

#include <algorithm>
#include <stdexcept>
#include <utility>

#include "opennow/settings.hpp"

namespace opennow {

namespace {

bool has_ice_credentials(const IceCredentials& credentials) {
  return !credentials.ufrag.empty() && !credentials.pwd.empty() &&
      !credentials.fingerprint.empty();
}

std::string resolution_label(const NvstSdpParams& params) {
  return std::to_string(params.width) + "x" + std::to_string(params.height);
}

std::vector<NativeWebRtcDataChannelConfig> build_input_channels() {
  return {
      {"input_1", true, std::nullopt, false},
      {"input_1_pr", false, 0, true},
  };
}

}  // namespace

StreamClient::StreamClient(std::unique_ptr<NativeWebRtcBackend> backend)
    : backend_(std::move(backend)) {
  if (!backend_) {
    throw std::invalid_argument("StreamClient requires a NativeWebRtcBackend");
  }
}

void StreamClient::set_event_handler(StreamClientEventHandler handler) {
  handler_ = std::move(handler);
}

NativeWebRtcStartRequest StreamClient::build_start_request(const StreamClientConfig& config) const {
  auto params = config.nvst_sdp;
  StreamNegotiationArtifacts artifacts;
  artifacts.remote_offer_sdp = config.remote_offer_sdp;
  if (!artifacts.remote_offer_sdp.empty() && config.session.server_ip) {
    artifacts.remote_offer_sdp = fix_server_ip(artifacts.remote_offer_sdp, *config.session.server_ip);
  }

  artifacts.local_answer_sdp = config.local_answer_sdp.empty()
      ? config.local_answer_sdp
      : munge_answer_sdp(config.local_answer_sdp, params.max_bitrate_kbps);

  const auto extracted_credentials = extract_ice_credentials(artifacts.local_answer_sdp);
  artifacts.ice_credentials = has_ice_credentials(extracted_credentials)
      ? extracted_credentials
      : params.credentials;
  if (!has_ice_credentials(artifacts.ice_credentials)) {
    throw std::invalid_argument(
        "StreamClient requires ICE credentials from local answer SDP or NVST parameters");
  }

  params.credentials = artifacts.ice_credentials;
  artifacts.nvst_sdp = build_nvst_sdp(params);

  NativeWebRtcStartRequest request;
  request.session = config.session;
  request.nvst_sdp_params = params;
  request.negotiation = std::move(artifacts);
  request.input_channels = build_input_channels();
  request.protocol_version = config.protocol_version;
  return request;
}

void StreamClient::start(const StreamClientConfig& config) {
  input_encoder_.set_protocol_version(config.protocol_version);
  input_encoder_.reset_gamepad_sequences();
  const auto request = build_start_request(config);
  negotiation_artifacts_ = request.negotiation;
  backend_->start(request);
  if (handler_) {
    handler_({"started", "stream backend started", backend_->diagnostics()});
  }
}

void StreamClient::stop() {
  backend_->stop();
  if (handler_) {
    handler_({"stopped", "stream backend stopped", backend_->diagnostics()});
  }
}

void StreamClient::add_ice_candidate(const NativeIceCandidate& candidate) {
  backend_->add_ice_candidate(candidate);
}

void StreamClient::send_heartbeat() {
  send_input_packet(input_encoder_.encode_heartbeat(), kInputHeartbeat);
}

void StreamClient::send_key_down(const KeyboardPayload& payload) {
  send_input_packet(input_encoder_.encode_key_down(payload), kInputKeyDown);
}

void StreamClient::send_key_up(const KeyboardPayload& payload) {
  send_input_packet(input_encoder_.encode_key_up(payload), kInputKeyUp);
}

void StreamClient::send_mouse_move(const MouseMovePayload& payload) {
  send_input_packet(input_encoder_.encode_mouse_move(payload), kInputMouseRel);
}

void StreamClient::send_mouse_button_down(const MouseButtonPayload& payload) {
  send_input_packet(input_encoder_.encode_mouse_button_down(payload), kInputMouseButtonDown);
}

void StreamClient::send_mouse_button_up(const MouseButtonPayload& payload) {
  send_input_packet(input_encoder_.encode_mouse_button_up(payload), kInputMouseButtonUp);
}

void StreamClient::send_mouse_wheel(const MouseWheelPayload& payload) {
  send_input_packet(input_encoder_.encode_mouse_wheel(payload), kInputMouseWheel);
}

void StreamClient::send_gamepad_state(
    const GamepadInput& payload,
    std::uint16_t bitmap,
    bool partially_reliable) {
  backend_->send_input(
      input_encoder_.encode_gamepad_state(payload, bitmap, partially_reliable),
      partially_reliable);
}

void StreamClient::send_input_packet(const std::vector<std::uint8_t>& bytes, int input_type) {
  backend_->send_input(bytes, is_partially_reliable_hid_transfer_eligible(input_type));
}

StreamDiagnostics StreamClient::diagnostics() const {
  return backend_->diagnostics();
}

const StreamNegotiationArtifacts& StreamClient::negotiation_artifacts() const {
  return negotiation_artifacts_;
}

void ProofWebRtcBackend::start(const NativeWebRtcStartRequest& request) {
  started_ = true;
  sent_packet_count_ = 0;
  partially_reliable_packet_count_ = 0;
  reliable_packet_count_ = 0;
  sent_byte_count_ = 0;
  added_ice_candidate_count_ = 0;
  last_start_request_ = request;
  diagnostics_ = {};
  diagnostics_.connection_state = "proof-ready";
  diagnostics_.input_ready = true;
  diagnostics_.resolution = resolution_label(request.nvst_sdp_params);
  diagnostics_.codec = to_string(request.nvst_sdp_params.codec);
  diagnostics_.is_hdr = color_quality_is_10_bit(request.nvst_sdp_params.color_quality);
  diagnostics_.partially_reliable_input_open =
      std::any_of(
          request.input_channels.begin(),
          request.input_channels.end(),
          [](const auto& channel) { return channel.partially_reliable; });
  diagnostics_.mouse_move_transport =
      diagnostics_.partially_reliable_input_open ? "partially_reliable" : "reliable";
  diagnostics_.mouse_flush_interval_ms = request.nvst_sdp_params.partial_reliable_threshold_ms;
  diagnostics_.negotiation_ready = true;
  diagnostics_.backend_detail =
      "proof backend prepared SDP/NVST artifacts without opening network media";
  diagnostics_.lag_reason = "stable";
}

void ProofWebRtcBackend::stop() {
  started_ = false;
  diagnostics_.connection_state = "closed";
  diagnostics_.input_ready = false;
  diagnostics_.partially_reliable_input_open = false;
}

void ProofWebRtcBackend::add_ice_candidate(const NativeIceCandidate& candidate) {
  if (!started_) {
    throw std::logic_error("cannot add ICE candidate before stream backend starts");
  }
  if (candidate.candidate.empty()) {
    throw std::invalid_argument("ICE candidate is empty");
  }
  ++added_ice_candidate_count_;
  diagnostics_.ice_candidates_added = added_ice_candidate_count_;
}

void ProofWebRtcBackend::send_input(
    const std::vector<std::uint8_t>& bytes,
    bool partially_reliable) {
  if (!started_) {
    throw std::logic_error("cannot send input before stream backend starts");
  }
  if (bytes.empty()) {
    throw std::invalid_argument("input packet is empty");
  }
  ++sent_packet_count_;
  sent_byte_count_ += bytes.size();
  if (partially_reliable) {
    ++partially_reliable_packet_count_;
    diagnostics_.partially_reliable_input_queue_peak_buffered_bytes = std::max(
        diagnostics_.partially_reliable_input_queue_peak_buffered_bytes,
        bytes.size());
  } else {
    ++reliable_packet_count_;
    diagnostics_.input_queue_peak_buffered_bytes =
        std::max(diagnostics_.input_queue_peak_buffered_bytes, bytes.size());
  }
  diagnostics_.input_packets_sent = sent_packet_count_;
  diagnostics_.reliable_input_packets_sent = reliable_packet_count_;
  diagnostics_.partially_reliable_input_packets_sent = partially_reliable_packet_count_;
  diagnostics_.input_bytes_sent = sent_byte_count_;
}

StreamDiagnostics ProofWebRtcBackend::diagnostics() const {
  return diagnostics_;
}

bool ProofWebRtcBackend::started() const {
  return started_;
}

std::size_t ProofWebRtcBackend::sent_packet_count() const {
  return sent_packet_count_;
}

std::size_t ProofWebRtcBackend::partially_reliable_packet_count() const {
  return partially_reliable_packet_count_;
}

std::size_t ProofWebRtcBackend::reliable_packet_count() const {
  return reliable_packet_count_;
}

std::size_t ProofWebRtcBackend::added_ice_candidate_count() const {
  return added_ice_candidate_count_;
}

const NativeWebRtcStartRequest& ProofWebRtcBackend::last_start_request() const {
  return last_start_request_;
}

}  // namespace opennow
