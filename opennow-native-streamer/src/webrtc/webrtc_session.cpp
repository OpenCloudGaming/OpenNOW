#include "opennow/native/webrtc_session.hpp"

#include "opennow/native/media_pipeline.hpp"
#include "opennow/native/protocol.hpp"
#include "opennow/native/sdp_helpers.hpp"

#if defined(OPENNOW_HAS_LIBDATACHANNEL)
#include <rtc/rtc.hpp>
#endif

#include <algorithm>
#include <cstddef>
#include <sstream>

namespace opennow::native {

namespace {

std::string ParseAudioCodecName(const std::string& sdp, int* payload_type, int* clock_rate, int* channels) {
  std::istringstream stream(sdp);
  std::string line;
  bool in_audio = false;
  while (std::getline(stream, line)) {
    if (!line.empty() && line.back() == '\r') {
      line.pop_back();
    }
    if (line.rfind("m=audio", 0) == 0) {
      in_audio = true;
      continue;
    }
    if (line.rfind("m=", 0) == 0 && in_audio) {
      break;
    }
    if (in_audio && line.rfind("a=rtpmap:", 0) == 0) {
      const auto colon = line.find(':');
      const auto space = line.find(' ', colon + 1);
      if (colon == std::string::npos || space == std::string::npos) {
        continue;
      }
      const auto slash = line.find('/', space + 1);
      const auto slash2 = line.find('/', slash + 1);
      *payload_type = std::stoi(line.substr(colon + 1, space - colon - 1));
      const auto codec = line.substr(space + 1, slash - space - 1);
      *clock_rate = slash == std::string::npos ? 48000 : std::stoi(line.substr(slash + 1, slash2 == std::string::npos ? std::string::npos : slash2 - slash - 1));
      *channels = slash2 == std::string::npos ? 2 : std::stoi(line.substr(slash2 + 1));
      return codec;
    }
  }
  return "opus";
}

#if defined(OPENNOW_HAS_LIBDATACHANNEL)
std::string ExtractLocalDescriptionSdp(const rtc::Description& description) {
  return std::string(description);
}

std::vector<std::uint8_t> BytesFromRtcBinary(const rtc::binary& data) {
  std::vector<std::uint8_t> out;
  out.reserve(data.size());
  for (const auto byte : data) {
    out.push_back(static_cast<std::uint8_t>(std::to_integer<unsigned int>(byte)));
  }
  return out;
}

rtc::binary RtcBinaryFromBytes(const std::vector<std::uint8_t>& data) {
  rtc::binary out;
  out.reserve(data.size());
  for (const auto byte : data) {
    out.push_back(static_cast<std::byte>(byte));
  }
  return out;
}
#endif

}  // namespace

void WebRtcSession::SetEmitter(EmitJson emitter) {
  emitter_ = std::move(emitter);
}

void WebRtcSession::SetLogger(LogFn logger) {
  logger_ = std::move(logger);
}

void WebRtcSession::SetMediaPipeline(MediaPipeline* media_pipeline) {
  media_pipeline_ = media_pipeline;
}

void WebRtcSession::SetInputReadyCallback(InputReadyFn callback) {
  input_ready_callback_ = std::move(callback);
}

bool WebRtcSession::ConfigureFromSession(const std::string& session_json, std::string& error) {
  if (const auto session_id = FindJsonString(session_json, "sessionId")) {
    session_id_ = *session_id;
  }
  if (const auto server_ip = FindJsonString(session_json, "serverIp")) {
    server_ip_ = *server_ip;
  }
  if (const auto media_ip = FindJsonString(session_json, "mediaConnectionIp")) {
    media_connection_ip_ = *media_ip;
  }
  if (const auto media_port = FindJsonInt(session_json, "mediaConnectionPort")) {
    media_connection_port_ = *media_port;
  }
  if (const auto codec = FindJsonString(session_json, "codec")) {
    preferred_codec_ = *codec;
  }
  if (const auto color_quality = FindJsonString(session_json, "colorQuality")) {
    color_quality_ = *color_quality;
  }
  if (const auto fps = FindJsonInt(session_json, "fps")) {
    fps_ = *fps;
  }
  if (const auto bitrate = FindJsonInt(session_json, "maxBitrateMbps")) {
    max_bitrate_kbps_ = *bitrate * 1000;
  }
  if (const auto resolution = FindJsonString(session_json, "resolution")) {
    const auto x = resolution->find('x');
    if (x != std::string::npos) {
      width_ = std::max(1, std::stoi(resolution->substr(0, x)));
      height_ = std::max(1, std::stoi(resolution->substr(x + 1)));
    }
  }
  if (media_pipeline_) {
    media_pipeline_->ConfigureVideoCodec(preferred_codec_);
  }
  error.clear();
  return true;
}

bool WebRtcSession::HandleOffer(const std::string& offer_sdp, std::string& error) {
#if !defined(OPENNOW_HAS_LIBDATACHANNEL)
  (void)offer_sdp;
  error = "libdatachannel is required for the native streamer backend.";
  EmitState("failed", "Native WebRTC unavailable", error);
  return false;
#else
  pending_offer_sdp_ = offer_sdp;
  media_failure_emitted_ = false;
  manual_media_candidate_injected_ = false;
  auto fixed_offer = FixServerIp(offer_sdp, !media_connection_ip_.empty() ? media_connection_ip_ : server_ip_);
  last_server_ice_ufrag_ = ExtractIceUfragFromOffer(fixed_offer);
  fixed_offer = RewriteH265LevelIdByProfile(fixed_offer, 153, 153);
  fixed_offer = RewriteH265TierFlag(fixed_offer, 0);
  fixed_offer = PreferCodec(fixed_offer, preferred_codec_);
  if (const auto threshold = ParsePartialReliableThresholdMs(fixed_offer)) {
    partial_reliable_threshold_ms_ = *threshold;
  }

  ConfigureTracksFromOffer(fixed_offer);
  if (!EnsurePeerConnection(error)) {
    EmitState("failed", "Peer connection setup failed", error);
    return false;
  }

  try {
    answer_sent_ = false;
    Log(std::string("Applying remote offer SDP (") + std::to_string(fixed_offer.size()) + " chars)");
    peer_connection_->setRemoteDescription(rtc::Description(fixed_offer, rtc::Description::Type::Offer));
    Log("setRemoteDescription completed successfully");
    EmitState("connecting", "Remote offer applied");
    Log("Invoking setLocalDescription(answer)");
    peer_connection_->setLocalDescription(rtc::Description::Type::Answer);
    Log("setLocalDescription(answer) returned without throwing");
    return true;
  } catch (const std::exception& ex) {
    error = ex.what();
    Log(std::string("Offer handling threw std::exception: ") + error);
    EmitState("failed", "Offer handling failed", error);
    return false;
  } catch (...) {
    error = "Unknown non-standard exception while applying remote offer";
    Log(error);
    EmitState("failed", "Offer handling failed", error);
    return false;
  }
#endif
}

void WebRtcSession::AddRemoteIce(const std::string& candidate_json) {
#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  if (!peer_connection_) {
    return;
  }
  const auto candidate = FindJsonString(candidate_json, "candidate");
  if (!candidate || candidate->empty()) {
    Log("Ignoring remote ICE payload without candidate string");
    return;
  }
  const auto mid = FindJsonString(candidate_json, "sdpMid");
  Log(std::string("Adding remote ICE candidate (mid=") + (mid ? *mid : std::string("<none>")) + "): " + *candidate);
  try {
    if (mid && !mid->empty()) {
      peer_connection_->addRemoteCandidate(rtc::Candidate(*candidate, *mid));
    } else {
      peer_connection_->addRemoteCandidate(rtc::Candidate(*candidate));
    }
  } catch (const std::exception& ex) {
    Log(std::string("Failed to add remote ICE: ") + ex.what());
  }
#else
  (void)candidate_json;
#endif
}

void WebRtcSession::Disconnect() {
#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  if (reliable_input_channel_) reliable_input_channel_->close();
  if (partial_input_channel_) partial_input_channel_->close();
  if (control_channel_) control_channel_->close();
  if (peer_connection_) {
    peer_connection_->close();
    peer_connection_.reset();
  }
  reliable_input_channel_.reset();
  partial_input_channel_.reset();
  control_channel_.reset();
  video_track_.reset();
  audio_track_.reset();
#endif
  answer_sent_ = false;
  input_ready_ = false;
  media_failure_emitted_ = false;
  manual_media_candidate_injected_ = false;
}

bool WebRtcSession::SendInputPacket(const InputPacket& packet) {
#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  auto channel = packet.route == InputRoute::PartiallyReliable ? partial_input_channel_ : reliable_input_channel_;
  if (!channel || !channel->isOpen()) {
    if (packet.route == InputRoute::PartiallyReliable) {
      channel = reliable_input_channel_;
    }
  }
  if (!channel || !channel->isOpen()) {
    return false;
  }
  return channel->send(RtcBinaryFromBytes(packet.bytes));
#else
  (void)packet;
  return false;
#endif
}

void WebRtcSession::Emit(const std::string& json) {
  if (emitter_) {
    emitter_(json);
  }
}

void WebRtcSession::Log(const std::string& message) const {
  if (logger_) {
    logger_(message);
  }
}

void WebRtcSession::EmitState(const std::string& state, const std::string& message, const std::string& detail) {
  std::ostringstream payload;
  payload << "{\"state\":\"" << EscapeJson(state) << "\",\"message\":\"" << EscapeJson(message) << "\"";
  if (!detail.empty()) {
    payload << ",\"detail\":\"" << EscapeJson(detail) << "\"";
  }
  payload << "}";
  Emit(BuildEnvelope("state", payload.str()));
}

bool WebRtcSession::EnsurePeerConnection(std::string& error) {
#if !defined(OPENNOW_HAS_LIBDATACHANNEL)
  error = "libdatachannel unavailable";
  return false;
#else
  if (peer_connection_) {
    return true;
  }
  try {
    rtc::Configuration config;
    config.disableAutoNegotiation = true;
    config.forceMediaTransport = true;
    peer_connection_ = std::make_shared<rtc::PeerConnection>(config);
    ConfigurePeerCallbacks();
    ConfigureInputChannels();
    ConfigureTrackHandlers();
    return true;
  } catch (const std::exception& ex) {
    error = ex.what();
    return false;
  }
#endif
}

void WebRtcSession::ConfigurePeerCallbacks() {
#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  peer_connection_->onStateChange([this](rtc::PeerConnection::State state) {
    switch (state) {
      case rtc::PeerConnection::State::Connected:
        if (!media_receive_supported_) {
          if (!media_failure_emitted_) {
            media_failure_emitted_ = true;
            EmitState(
                "failed",
                "Native streamer build lacks media receive support",
                "libdatachannel was built without media track handlers, so video/audio cannot be received in this build");
          }
        } else {
          EmitState("streaming", "Native WebRTC connected");
          TryInjectManualMediaCandidate();
        }
        break;
      case rtc::PeerConnection::State::Failed:
        EmitState("failed", "Native WebRTC failed");
        break;
      case rtc::PeerConnection::State::Disconnected:
        EmitState("failed", "Native WebRTC disconnected");
        break;
      case rtc::PeerConnection::State::Closed:
        EmitState("exited", "Native WebRTC closed");
        break;
      default:
        EmitState("connecting", "Native WebRTC connecting");
        break;
    }
  });

  peer_connection_->onLocalDescription([this](rtc::Description description) {
    const auto raw_sdp = ExtractLocalDescriptionSdp(description);
    Log(std::string("Native local description callback fired (type=") + description.typeString() + ", sdpLength=" + std::to_string(raw_sdp.size()) + ")");
    if (description.typeString() != "answer") {
      Log(std::string("Ignoring local description callback because type is ") + description.typeString());
      return;
    }
    if (answer_sent_) {
      Log("Ignoring duplicate answer callback because answer was already sent");
      return;
    }
    auto answer = MungeAnswerSdp(raw_sdp, max_bitrate_kbps_);
    const auto credentials = ExtractIceCredentials(answer);
    const auto nvst = BuildNvstSdp(
        width_,
        height_,
        width_,
        height_,
        fps_,
        max_bitrate_kbps_,
        preferred_codec_,
        color_quality_,
        partial_reliable_threshold_ms_,
        credentials);
    answer_sent_ = true;
    Emit(BuildEnvelope(
        "answer",
        std::string("{\"sdp\":\"") + EscapeJson(answer) + "\",\"nvstSdp\":\"" + EscapeJson(nvst) + "\"}"));
  });

  peer_connection_->onLocalCandidate([this](rtc::Candidate candidate) {
    Log(std::string("Emitting local ICE candidate (mid=") + candidate.mid() + ", answerSent=" + (answer_sent_ ? std::string("true") : std::string("false")) + "): " + candidate.candidate());
    std::ostringstream payload;
    payload << "{\"candidate\":\"" << EscapeJson(candidate.candidate()) << "\",\"sdpMid\":";
    if (candidate.mid().empty()) {
      payload << "null";
    } else {
      payload << "\"" << EscapeJson(candidate.mid()) << "\"";
    }
    payload << ",\"sdpMLineIndex\":null,\"usernameFragment\":null}";
    Emit(BuildEnvelope("local-ice", payload.str()));
  });

  peer_connection_->onDataChannel([this](std::shared_ptr<rtc::DataChannel> channel) {
    if (channel->label() == "control_channel") {
      control_channel_ = channel;
      channel->onMessage([this](rtc::message_variant message) {
        if (std::holds_alternative<std::string>(message)) {
          Log(std::string("Control channel: ") + std::get<std::string>(message));
        }
      });
    }
  });
#endif
}

void WebRtcSession::ConfigureTracksFromOffer(const std::string& offer_sdp) {
  int audio_payload = 111;
  int audio_clock = 48000;
  int audio_channels = 2;
  const auto audio_codec = ParseAudioCodecName(offer_sdp, &audio_payload, &audio_clock, &audio_channels);
  if (media_pipeline_) {
    media_pipeline_->ConfigureAudioCodec(audio_codec, audio_payload, audio_clock, audio_channels);
  }
}

void WebRtcSession::ConfigureInputChannels() {
#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  rtc::DataChannelInit reliable{};
  reliable.reliability.unordered = false;
  reliable_input_channel_ = peer_connection_->createDataChannel("input_channel_v1", reliable);
  reliable_input_channel_->onOpen([this]() {
    input_ready_ = true;
    const std::vector<std::uint8_t> handshake = {0x0e, 0x02};
    reliable_input_channel_->send(RtcBinaryFromBytes(handshake));
    EmitState("connecting", "Reliable input channel open");
  });
  reliable_input_channel_->onMessage([this](rtc::message_variant message) {
    if (const auto* bytes = std::get_if<rtc::binary>(&message)) {
      HandleReliableInputMessage(BytesFromRtcBinary(*bytes));
      return;
    }
    if (const auto* text = std::get_if<std::string>(&message)) {
      HandleReliableInputMessage(std::vector<std::uint8_t>(text->begin(), text->end()));
    }
  });

  rtc::DataChannelInit partial{};
  partial.reliability.unordered = true;
  partial.reliability.maxPacketLifeTime = std::chrono::milliseconds(partial_reliable_threshold_ms_);
  partial_input_channel_ = peer_connection_->createDataChannel("input_channel_partially_reliable", partial);
#endif
}

void WebRtcSession::ConfigureTrackHandlers() {
#if defined(OPENNOW_HAS_LIBDATACHANNEL) && defined(OPENNOW_HAS_LIBDATACHANNEL_MEDIA)
  media_receive_supported_ = true;
  peer_connection_->onTrack([this](std::shared_ptr<rtc::Track> track) {
    const auto description = track->description();
    if (description.mid() == "video" || description.type() == "video") {
      video_track_ = track;
      if (preferred_codec_ == "H265") {
        track->setMediaHandler(std::make_shared<rtc::H265RtpDepacketizer>());
      } else if (preferred_codec_ == "AV1") {
        if (!media_failure_emitted_) {
          media_failure_emitted_ = true;
          Log("AV1 video tracks are not supported by the current native libdatachannel media handler build; failing native AV1 session instead of routing through H264");
          EmitState(
              "failed",
              "Native AV1 streaming is unsupported in this build",
              "libdatachannel AV1 depacketizer/media receive support is unavailable; disable native streamer or use H264/H265");
        }
        return;
      } else {
        track->setMediaHandler(std::make_shared<rtc::H264RtpDepacketizer>());
      }
      track->chainMediaHandler(std::make_shared<rtc::RtcpReceivingSession>());
      track->onFrame([this](rtc::binary frame, rtc::FrameInfo info) {
        if (media_pipeline_) {
          const auto us = static_cast<std::uint64_t>(info.timestampSeconds ? info.timestampSeconds->count() * 1000000.0 : 0.0);
          media_pipeline_->PushVideoFrame(BytesFromRtcBinary(frame), us);
        }
      });
      return;
    }
    audio_track_ = track;
    track->setMediaHandler(std::make_shared<rtc::OpusRtpDepacketizer>());
    track->chainMediaHandler(std::make_shared<rtc::RtcpReceivingSession>());
    track->onFrame([this](rtc::binary frame, rtc::FrameInfo info) {
      if (media_pipeline_) {
        const auto us = static_cast<std::uint64_t>(info.timestampSeconds ? info.timestampSeconds->count() * 1000000.0 : 0.0);
        media_pipeline_->PushAudioFrame(BytesFromRtcBinary(frame), us);
      }
    });
  });
#elif defined(OPENNOW_HAS_LIBDATACHANNEL)
  media_receive_supported_ = false;
  Log("libdatachannel media support is unavailable; native track receive handlers are disabled for this build");
  EmitState(
      "failed",
      "Native streamer build lacks media receive support",
      "Install a media-enabled libdatachannel package to enable native video/audio playback");
  media_failure_emitted_ = true;
#endif
}

void WebRtcSession::TryInjectManualMediaCandidate() {
#if defined(OPENNOW_HAS_LIBDATACHANNEL)
  if (!peer_connection_ || media_connection_port_ <= 0 || manual_media_candidate_injected_) {
    return;
  }
  const auto public_ip = ExtractPublicIp(media_connection_ip_);
  if (!public_ip) {
    return;
  }
  const std::string candidate = "candidate:1 1 udp 2130706431 " + *public_ip + " " + std::to_string(media_connection_port_) + " typ host";
  Log(std::string("Injecting manual media ICE candidate: ") + candidate);
  try {
    peer_connection_->addRemoteCandidate(rtc::Candidate(candidate, "0"));
    manual_media_candidate_injected_ = true;
    Log("Manual media ICE candidate injected on mid 0");
  } catch (...) {
    try {
      peer_connection_->addRemoteCandidate(rtc::Candidate(candidate, "1"));
      manual_media_candidate_injected_ = true;
      Log("Manual media ICE candidate injected on mid 1");
    } catch (...) {
      Log("Manual ICE candidate injection failed");
    }
  }
#endif
}

void WebRtcSession::HandleReliableInputMessage(const std::vector<std::uint8_t>& bytes) {
  if (bytes.size() < 2) {
    return;
  }
  const std::uint16_t first_word = static_cast<std::uint16_t>(bytes[0] | (bytes[1] << 8));
  int version = 0;
  if (first_word == 526 && bytes.size() >= 4) {
    version = static_cast<int>(bytes[2] | (bytes[3] << 8));
  } else if (bytes[0] == 0x0e) {
    version = static_cast<int>(first_word);
  }
  if (version > 0) {
    input_protocol_version_ = version;
    if (input_ready_callback_) {
      input_ready_callback_(version);
    }
  }
}

}  // namespace opennow::native
