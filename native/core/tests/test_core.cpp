#include <cstdlib>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

#include "opennow/input_protocol.hpp"
#include "opennow/open_now_core.hpp"
#include "opennow/service_clients.hpp"
#include "opennow/service_builders.hpp"
#include "opennow/service_transport.hpp"
#include "opennow/settings.hpp"
#include "opennow/sdp.hpp"
#include "opennow/stream_client.hpp"

namespace {

void require(bool condition, const std::string& message) {
  if (!condition) {
    throw std::runtime_error(message);
  }
}

void require_bytes(
    const std::vector<std::uint8_t>& actual,
    const std::vector<std::uint8_t>& expected,
    const std::string& label) {
  if (actual == expected) {
    return;
  }
  std::cerr << label << " mismatch\nexpected:";
  for (const auto byte : expected) {
    std::cerr << " " << static_cast<int>(byte);
  }
  std::cerr << "\nactual:  ";
  for (const auto byte : actual) {
    std::cerr << " " << static_cast<int>(byte);
  }
  std::cerr << "\n";
  throw std::runtime_error(label + " bytes mismatch");
}

void test_settings_helpers() {
  using namespace opennow;

  require(color_quality_bit_depth(ColorQuality::EightBit420) == 0, "8-bit depth");
  require(color_quality_bit_depth(ColorQuality::TenBit420) == 10, "10-bit depth");
  require(color_quality_chroma_format(ColorQuality::EightBit444) == 2, "4:4:4 chroma");
  require(color_quality_chroma_format(ColorQuality::TenBit420) == 0, "4:2:0 chroma");
  require(color_quality_requires_hevc(ColorQuality::EightBit420) == false, "H264-compatible color");
  require(color_quality_requires_hevc(ColorQuality::TenBit444) == true, "HEVC color");

  const auto prefs = normalize_stream_preferences(VideoCodec::H264, ColorQuality::TenBit420);
  require(prefs.codec == VideoCodec::H264, "normalized codec");
  require(prefs.color_quality == ColorQuality::TenBit420, "normalized color quality");
  require(!prefs.migrated, "no migration for supported prefs");

  require(resolve_gfn_keyboard_layout("en-US", "darwin") == "m-us", "mac en-US layout");
  require(resolve_gfn_keyboard_layout("en-GB", "darwin") == "m-brit", "mac en-GB layout");
  require(resolve_gfn_keyboard_layout("de-DE", "darwin") == "de-DE", "mac generic layout");
  require(resolve_gfn_keyboard_layout("bogus", "darwin") == "en-US", "unknown layout fallback");
}

void test_input_protocol_helpers() {
  using namespace opennow;

  require(partially_reliable_hid_mask_for_input_type(7) == 128, "HID mask");
  require(partially_reliable_hid_mask_for_input_type(32) == 0, "HID mask out of range");
  require(is_partially_reliable_hid_transfer_eligible(kInputMouseRel), "mouse PR eligible");
  require(!is_partially_reliable_hid_transfer_eligible(kInputKeyDown), "key PR ineligible");
  require(normalize_to_int16(1.0) == 32767, "positive int16 normalization");
  require(normalize_to_int16(-1.0) == -32768, "negative int16 normalization");
  require(normalize_to_uint8(0.5) == 128, "uint8 normalization");

  const auto quantized = quantize_mouse_delta_with_residual(1.6);
  require(quantized.send == 2, "mouse quantized send");
  require(quantized.residual < -0.39 && quantized.residual > -0.41, "mouse quantized residual");
}

void test_input_encoder_v2_bytes() {
  using namespace opennow;

  InputEncoder encoder;
  encoder.set_protocol_version(2);

  require_bytes(encoder.encode_heartbeat(), {2, 0, 0, 0}, "heartbeat");

  require_bytes(
      encoder.encode_key_down({0x41, 0x1e, 0x03, 1}),
      {3, 0, 0, 0, 0, 0x41, 0, 0x03, 0, 0x1e, 0, 0, 0, 0, 0, 0, 0, 1},
      "key down");

  require_bytes(
      encoder.encode_mouse_move({2, -3, 9}),
      {7, 0, 0, 0, 0, 2, 0xff, 0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9},
      "mouse move");

  require_bytes(
      encoder.encode_mouse_button_down({kMouseLeft, 10}),
      {8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10},
      "mouse button");

  require_bytes(
      encoder.encode_mouse_wheel({-120, 11}),
      {10, 0, 0, 0, 0, 0, 0xff, 0x88, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11},
      "mouse wheel");
}

void test_input_encoder_gamepad() {
  using namespace opennow;

  InputEncoder encoder;
  encoder.set_protocol_version(2);
  GamepadInput gamepad;
  gamepad.controller_id = 1;
  gamepad.buttons = 0x1000;
  gamepad.left_trigger = 10;
  gamepad.right_trigger = 20;
  gamepad.left_stick_x = 100;
  gamepad.left_stick_y = -100;
  gamepad.right_stick_x = 200;
  gamepad.right_stick_y = -200;
  gamepad.timestamp_us = 0x0102030405060708ULL;

  require_bytes(
      encoder.encode_gamepad_state(gamepad, 0x0003, false),
      {
          12, 0, 0, 0,
          26, 0,
          1, 0,
          3, 0,
          20, 0,
          0, 0x10,
          10, 20,
          100, 0,
          0x9c, 0xff,
          200, 0,
          0x38, 0xff,
          0, 0,
          85, 0,
          0, 0,
          8, 7, 6, 5, 4, 3, 2, 1,
      },
      "gamepad");

  require(encoder.get_next_gamepad_sequence(2) == 1, "gamepad sequence first");
  require(encoder.get_next_gamepad_sequence(2) == 2, "gamepad sequence second");
}

void test_core_facade() {
  opennow::OpenNowCore core({});
  require(!core.has_auth_service(), "auth dependency absent");
  require(!core.has_catalog_service(), "catalog dependency absent");
  require(!core.has_subscription_service(), "subscription dependency absent");
  require(!core.has_session_service(), "session dependency absent");
  require(!core.has_signaling_client(), "signaling dependency absent");
  require(opennow::native_core_version() == "0.1.0", "native core version");
}

void test_service_builders() {
  using namespace opennow;

  const LoginProvider provider{
      "provider-id",
      "NVIDIA",
      "NVIDIA",
      "https://prod.cloudmatchbeta.nvidiagrid.net/",
      0,
  };
  const auto auth_url = build_auth_url({provider, "challenge value", 2259, "nonce", "device"});
  require(auth_url.find("https://login.nvidia.com/authorize?") == 0, "auth URL endpoint");
  require(auth_url.find("client_id=ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ") != std::string::npos, "auth URL client id");
  require(auth_url.find("redirect_uri=http%3A%2F%2Flocalhost%3A2259") != std::string::npos, "auth URL redirect");
  require(auth_url.find("code_challenge=challenge%20value") != std::string::npos, "auth URL challenge");
  require(auth_url.find("idp_id=provider-id") != std::string::npos, "auth URL provider");

  const auto headers = build_cloudmatch_headers({"token", "client", "device", true});
  require(headers.at("Authorization") == "GFNJWT token", "cloudmatch authorization");
  require(headers.at("nv-device-os") == "MACOS", "cloudmatch macOS device OS");
  require(headers.at("Origin") == "https://play.geforcenow.com", "cloudmatch origin");

  const auto rtsp_url = build_signaling_url("rtsps://example.nvidia.com:443/nvst/", "10.0.0.5");
  require(rtsp_url.signaling_url == "wss://example.nvidia.com/nvst/", "RTSP signaling URL");
  require(rtsp_url.signaling_host == "example.nvidia.com", "RTSP signaling host");

  const auto relative_url = build_signaling_url("/nvst/", "10.0.0.5");
  require(relative_url.signaling_url == "wss://10.0.0.5:443/nvst/", "relative signaling URL");
  require(!relative_url.signaling_host, "relative signaling host");

  const auto sign_in_url = build_signaling_sign_in_url("10.0.0.5", "peer-1", std::nullopt);
  require(sign_in_url == "wss://10.0.0.5:443/nvst/sign_in?peer_id=peer-1&version=2", "sign-in URL");

  require(
      build_peer_info_message(1, 2, "peer-1") ==
          "{\"ackid\":1,\"peer_info\":{\"browser\":\"Chrome\",\"browserVersion\":\"131\",\"connected\":true,\"id\":2,\"name\":\"peer-1\",\"peerRole\":0,\"resolution\":\"1920x1080\",\"version\":2}}",
      "peer info JSON");

  require(
      build_answer_message(2, 2, "v=0", std::optional<std::string>("nvst")) ==
          "{\"peer_msg\":{\"from\":2,\"to\":1,\"msg\":\"{\\\"type\\\":\\\"answer\\\",\\\"sdp\\\":\\\"v=0\\\",\\\"nvstSdp\\\":\\\"nvst\\\"}\"},\"ackid\":2}",
      "answer JSON");

  require(
      build_ice_candidate_message(3, 2, "candidate", std::optional<std::string>("0"), 0) ==
          "{\"peer_msg\":{\"from\":2,\"to\":1,\"msg\":\"{\\\"candidate\\\":\\\"candidate\\\",\\\"sdpMid\\\":\\\"0\\\",\\\"sdpMLineIndex\\\":0}\"},\"ackid\":3}",
      "ICE JSON");

  require(
      build_keyframe_request_message(4, 2, "decoder", 5, 1) ==
          "{\"peer_msg\":{\"from\":2,\"to\":1,\"msg\":\"{\\\"type\\\":\\\"request_keyframe\\\",\\\"reason\\\":\\\"decoder\\\",\\\"backlogFrames\\\":5,\\\"attempt\\\":1}\"},\"ackid\":4}",
      "keyframe JSON");
}

void test_sdp_helpers() {
  using namespace opennow;

  require(extract_public_ip("80-250-97-40.cloudmatchbeta.nvidiagrid.net") == "80.250.97.40", "dashed public IP");
  require(extract_public_ip("161.248.11.132") == "161.248.11.132", "dotted public IP");
  require(!extract_public_ip("example.com"), "non-IP host");

  const std::string offer =
      "v=0\n"
      "a=ice-ufrag:ufrag\n"
      "a=ice-pwd:pwd\n"
      "a=fingerprint:sha-256 AA:BB\n"
      "m=video 0 RTP/AVP 96\n"
      "c=IN IP4 0.0.0.0\n"
      "a=rtpmap:96 H265/90000\n"
      "a=fmtp:96 profile-id=1;tier-flag=1;level-id=186\n"
      "a=candidate:1 1 udp 1 0.0.0.0 123 typ host\n";

  const auto fixed = fix_server_ip(offer, "80-250-97-40.cloudmatchbeta.nvidiagrid.net");
  require(fixed.find("c=IN IP4 80.250.97.40") != std::string::npos, "fixed connection IP");
  require(fixed.find("udp 1 80.250.97.40 123") != std::string::npos, "fixed candidate IP");

  require(extract_ice_ufrag_from_offer(offer) == "ufrag", "ice ufrag");
  const auto credentials = extract_ice_credentials(offer);
  require(credentials.ufrag == "ufrag", "credentials ufrag");
  require(credentials.pwd == "pwd", "credentials pwd");
  require(credentials.fingerprint == "AA:BB", "credentials fingerprint");

  const auto rewritten = rewrite_h265_tier_flag(offer, 0);
  require(rewritten.replacements == 1, "H265 tier replacements");
  require(rewritten.sdp.find("tier-flag=0") != std::string::npos, "H265 tier value");

  const auto munged = munge_answer_sdp("m=video 9 RTP/AVP 96\na=fmtp:111 minptime=10\nm=audio 9 RTP/AVP 111", 25000);
  require(munged.find("b=AS:25000") != std::string::npos, "video bitrate line");
  require(munged.find("b=AS:128") != std::string::npos, "audio bitrate line");
  require(munged.find("stereo=1") != std::string::npos, "opus stereo");

  NvstSdpParams params;
  params.width = 2560;
  params.height = 1440;
  params.fps = 120;
  params.max_bitrate_kbps = 50000;
  params.codec = VideoCodec::AV1;
  params.color_quality = ColorQuality::TenBit420;
  params.credentials = {"u", "p", "f"};
  const auto nvst = build_nvst_sdp(params);
  require(nvst.find("a=general.icePassword:p") != std::string::npos, "NVST ICE password");
  require(nvst.find("a=video.clientViewportWd:2560") != std::string::npos, "NVST width");
  require(nvst.find("a=video.maxFPS:120") != std::string::npos, "NVST FPS");
  require(nvst.find("a=video.scalingFeature1:1") != std::string::npos, "NVST AV1 scaling");
}

void test_fixture_backed_auth_catalog_subscription_session_services() {
  using namespace opennow;

  auto transport = std::make_shared<FixtureServiceTransport>();
  transport->add_response(
      HttpMethod::Get,
      "https://fixture.local/v2/loginProviders",
      {200, {}, "{\"providers\":[{\"idpId\":\"nvidia\",\"code\":\"NV\",\"displayName\":\"NVIDIA\",\"streamingServiceUrl\":\"https://stream.local\",\"priority\":1}]}"});
  transport->add_response(
      HttpMethod::Post,
      "https://fixture.local/v1/auth/session",
      {200, {}, "{\"session\":{\"provider\":{\"idpId\":\"nvidia\",\"code\":\"NV\",\"displayName\":\"NVIDIA\",\"streamingServiceUrl\":\"https://stream.local/\"},\"tokens\":{\"accessToken\":\"access\",\"refreshToken\":\"refresh\",\"expiresAtMs\":1234,\"clientToken\":\"client\"},\"user\":{\"userId\":\"user-1\",\"displayName\":\"Ada\",\"email\":\"ada@example.test\",\"membershipTier\":\"ULTIMATE\"}}}"});
  transport->add_response(
      HttpMethod::Post,
      "https://fixture.local/v1/catalog/library",
      {200, {}, "{\"games\":[{\"id\":\"100\",\"uuid\":\"uuid-100\",\"launchAppId\":\"200\",\"title\":\"Portal\",\"selectedVariantIndex\":0,\"variants\":[{\"id\":\"200\",\"store\":\"STEAM\",\"supportedControls\":[\"keyboardMouse\",\"gamepad\"],\"libraryStatus\":\"IN_LIBRARY\"}]}]}"});
  transport->add_response(
      HttpMethod::Get,
      "https://mes.geforcenow.com/v4/subscriptions?serviceName=gfn_pc&languageCode=en_US&vpcId=NP-AMS-08&userId=user-1",
      {200, {}, "{\"membershipTier\":\"ULTIMATE\",\"type\":\"PAID\",\"subType\":\"UNLIMITED\",\"allottedTimeInMinutes\":600,\"purchasedTimeInMinutes\":60,\"rolledOverTimeInMinutes\":0,\"remainingTimeInMinutes\":540,\"totalTimeInMinutes\":660,\"notifications\":{\"notifyUserWhenTimeRemainingInMinutes\":30},\"currentSubscriptionState\":{\"state\":\"ACTIVE\",\"isGamePlayAllowed\":true},\"features\":{\"resolutions\":[{\"widthInPixels\":1920,\"heightInPixels\":1080,\"framesPerSecond\":60,\"isEntitled\":true},{\"widthInPixels\":3840,\"heightInPixels\":2160,\"framesPerSecond\":120,\"isEntitled\":true}]},\"addons\":[{\"type\":\"STORAGE\",\"subType\":\"PERMANENT_STORAGE\",\"status\":\"OK\",\"attributes\":[{\"key\":\"TOTAL_STORAGE_SIZE_IN_GB\",\"textValue\":\"200\"},{\"key\":\"USED_STORAGE_SIZE_IN_GB\",\"textValue\":\"25\"},{\"key\":\"STORAGE_METRO_REGION_NAME\",\"textValue\":\"Amsterdam\"},{\"key\":\"STORAGE_METRO_REGION\",\"textValue\":\"NP-AMS-08\"}]}]}"});
  transport->add_response(
      HttpMethod::Post,
      "https://stream.local/v1/session",
      {200, {}, "{\"session\":{\"sessionId\":\"session-1\",\"appId\":\"200\",\"serverIp\":\"10.0.0.5\",\"signalingServer\":\"10.0.0.5\",\"signalingUrl\":\"wss://10.0.0.5:443/nvst/\",\"iceServers\":[{\"urls\":[\"stun:10.0.0.5:3478\"],\"username\":\"u\",\"credential\":\"p\"}]}}"});
  transport->add_response(
      HttpMethod::Get,
      "https://stream.local/v1/session/session-1",
      {200, {}, "{\"session\":{\"sessionId\":\"session-1\",\"appId\":\"200\",\"serverIp\":\"10.0.0.5\",\"signalingServer\":\"10.0.0.5\",\"signalingUrl\":\"wss://10.0.0.5:443/nvst/\",\"iceServers\":[]}}"});
  transport->add_response(
      HttpMethod::Delete,
      "https://stream.local/v1/session/session-1",
      {204, {}, ""});

  TransportAuthService auth(transport, "https://fixture.local");
  const auto providers = auth.get_providers();
  require(providers.size() == 1, "provider count");
  require(providers[0].streaming_service_url == "https://stream.local/", "provider URL normalized");

  const auto session = auth.login({"nvidia"});
  require(session.tokens.access_token == "access", "auth access token");
  require(session.user.membership_tier == "ULTIMATE", "auth membership tier");
  require(auth.get_session(true).refresh.outcome == "refreshed", "auth refresh scaffold");

  TransportCatalogService catalog(transport, "https://fixture.local");
  CatalogBrowseRequest catalog_request;
  catalog_request.token = "access";
  const auto library = catalog.fetch_library_games(catalog_request);
  require(library.size() == 1, "library game count");
  require(library[0].variants[0].supported_controls.size() == 2, "library controls parsed");
  require(library[0].variants[0].library_status == "IN_LIBRARY", "library status parsed");

  TransportSubscriptionService subscription(transport);
  const auto subscription_info = subscription.fetch_subscription({"access", "user-1"});
  require(subscription_info.membership_tier == "ULTIMATE", "subscription membership tier");
  require(subscription_info.is_unlimited, "subscription unlimited");
  require(subscription_info.used_hours == 2.0, "subscription used hours");
  require(subscription_info.entitled_resolutions[0].width == 3840, "subscription resolutions sorted");
  require(subscription_info.storage_addon->size_gb == 200, "subscription storage addon");

  TransportSessionService sessions(transport);
  const auto created = sessions.create_session({"200", "access", "client", "https://stream.local", "NP-AMS-08", "1920x1080"});
  require(created.id == "session-1", "created session id");
  require(created.ice_servers.size() == 1, "created session ICE");
  require(created.ice_servers[0].urls[0] == "stun:10.0.0.5:3478", "created session ICE URL");
  require(sessions.poll_session("session-1").server_ip == "10.0.0.5", "poll session server IP");
  sessions.stop_session("session-1");

  require(transport->requests().size() == 7, "fixture request count");
}

void test_fixture_backed_signaling_client() {
  using namespace opennow;

  auto transport = std::make_shared<FixtureSignalingTransport>();
  TransportSignalingClient signaling(
      transport,
      "10.0.0.5",
      "session-1",
      "peer-1",
      std::optional<std::string>("wss://10.0.0.5:443/nvst/"));

  std::vector<SignalingEvent> events;
  signaling.set_listener([&](const SignalingEvent& event) {
    events.push_back(event);
  });

  signaling.connect();
  require(transport->connected(), "signaling connected");
  require(transport->connect_request()->url == "wss://10.0.0.5:443/nvst/sign_in?peer_id=peer-1&version=2", "signaling URL");
  require(transport->connect_request()->protocol == "x-nv-sessionid.session-1", "signaling protocol");
  require(transport->sent_messages()[0].find("\"peer_info\"") != std::string::npos, "peer info sent");
  require(events[0].type == "connected", "connected event");

  transport->receive_text("{\"ackid\":8,\"hb\":1}");
  require(transport->sent_messages().back() == "{\"hb\":1}", "heartbeat response");

  transport->receive_text("{\"ackid\":9,\"peer_msg\":{\"from\":1,\"to\":2,\"msg\":\"{\\\"type\\\":\\\"offer\\\",\\\"sdp\\\":\\\"v=0\\\"}\"}}");
  require(transport->sent_messages().back() == "{\"ack\":9}", "ack response");
  require(events.back().type == "offer", "offer event");
  require(events.back().message == "v=0", "offer sdp");

  signaling.send_answer("answer-sdp");
  require(transport->sent_messages().back().find("answer-sdp") != std::string::npos, "answer sent");

  signaling.send_ice_candidate("candidate:1");
  require(transport->sent_messages().back().find("candidate:1") != std::string::npos, "ICE sent");

  signaling.request_keyframe();
  require(transport->sent_messages().back().find("request_keyframe") != std::string::npos, "keyframe sent");

  signaling.disconnect();
  require(!transport->connected(), "signaling disconnected");
  require(events.back().type == "disconnected", "disconnected event");
}

void test_stream_client_proof_backend() {
  using namespace opennow;

  auto backend = std::make_unique<ProofWebRtcBackend>();
  auto* backend_ptr = backend.get();
  StreamClient client(std::move(backend));

  StreamClientConfig config;
  config.nvst_sdp.width = 1920;
  config.nvst_sdp.height = 1080;
  config.nvst_sdp.codec = VideoCodec::H264;
  config.nvst_sdp.color_quality = ColorQuality::EightBit420;
  config.nvst_sdp.credentials = {"ufrag", "pwd", "AA:BB"};
  config.protocol_version = 2;
  bool saw_started = false;
  client.set_event_handler([&](const StreamClientEvent& event) {
    if (event.type == "started") {
      saw_started = true;
    }
  });
  client.start(config);
  require(saw_started, "stream start event");
  require(backend_ptr->started(), "proof backend started");
  require(client.diagnostics().connection_state == "proof-ready", "proof diagnostics state");
  require(client.diagnostics().negotiation_ready, "proof negotiation ready");
  require(client.diagnostics().partially_reliable_input_open, "proof partial reliable open");

  client.send_mouse_move({1, 1, 1});
  require(backend_ptr->sent_packet_count() == 1, "proof input packet count");
  require(backend_ptr->partially_reliable_packet_count() == 1, "proof partial packet count");
  client.send_key_down({0x41, 0x1e, 0x03, 2});
  require(backend_ptr->reliable_packet_count() == 1, "proof reliable packet count");
  require(client.diagnostics().input_packets_sent == 2, "proof diagnostics input count");

  client.add_ice_candidate({"candidate:1 1 udp 1 80.250.97.40 123 typ host", "0", 0, "ufrag"});
  require(backend_ptr->added_ice_candidate_count() == 1, "proof ICE candidate count");
  client.stop();
  require(!backend_ptr->started(), "proof backend stopped");
}

void test_stream_client_negotiation_artifacts() {
  using namespace opennow;

  auto backend = std::make_unique<ProofWebRtcBackend>();
  auto* backend_ptr = backend.get();
  StreamClient client(std::move(backend));

  StreamClientConfig config;
  config.session.server_ip = "80-250-97-40.cloudmatchbeta.nvidiagrid.net";
  config.remote_offer_sdp =
      "v=0\n"
      "m=video 0 RTP/AVP 96\n"
      "c=IN IP4 0.0.0.0\n"
      "a=candidate:1 1 udp 1 0.0.0.0 123 typ host\n";
  config.local_answer_sdp =
      "v=0\n"
      "a=ice-ufrag:local-ufrag\n"
      "a=ice-pwd:local-pwd\n"
      "a=fingerprint:sha-256 11:22\n"
      "m=video 9 RTP/AVP 96\n"
      "a=rtpmap:96 H264/90000\n"
      "m=audio 9 RTP/AVP 111\n"
      "a=fmtp:111 minptime=10\n";
  config.nvst_sdp.width = 2560;
  config.nvst_sdp.height = 1440;
  config.nvst_sdp.fps = 120;
  config.nvst_sdp.max_bitrate_kbps = 50000;
  config.nvst_sdp.codec = VideoCodec::H264;
  config.nvst_sdp.color_quality = ColorQuality::TenBit420;

  client.start(config);
  const auto& artifacts = client.negotiation_artifacts();
  require(
      artifacts.remote_offer_sdp.find("c=IN IP4 80.250.97.40") != std::string::npos,
      "negotiation fixed offer connection IP");
  require(
      artifacts.local_answer_sdp.find("b=AS:50000") != std::string::npos,
      "negotiation munged video bitrate");
  require(
      artifacts.local_answer_sdp.find("stereo=1") != std::string::npos,
      "negotiation munged opus stereo");
  require(artifacts.ice_credentials.ufrag == "local-ufrag", "negotiation ICE ufrag");
  require(
      artifacts.nvst_sdp.find("a=general.icePassword:local-pwd") != std::string::npos,
      "negotiation NVST ICE password");
  require(
      backend_ptr->last_start_request().input_channels.size() == 2,
      "negotiation input channels");
  require(
      backend_ptr->last_start_request().input_channels[1].partially_reliable,
      "negotiation partial reliable channel");
  client.stop();
}

void test_stream_client_lifecycle_guards() {
  using namespace opennow;

  StreamClientConfig config;
  config.nvst_sdp.credentials = {"ufrag", "pwd", "AA:BB"};
  StreamClient client(std::make_unique<ProofWebRtcBackend>());

  bool send_before_start_failed = false;
  try {
    client.send_heartbeat();
  } catch (const std::logic_error&) {
    send_before_start_failed = true;
  }
  require(send_before_start_failed, "send before start guard");

  client.start(config);
  client.stop();

  bool candidate_after_stop_failed = false;
  try {
    client.add_ice_candidate({"candidate:1 1 udp 1 80.250.97.40 123 typ host", "0", 0, "ufrag"});
  } catch (const std::logic_error&) {
    candidate_after_stop_failed = true;
  }
  require(candidate_after_stop_failed, "ICE candidate after stop guard");

  StreamClient missing_credentials(std::make_unique<ProofWebRtcBackend>());
  bool missing_credentials_failed = false;
  try {
    missing_credentials.start({});
  } catch (const std::invalid_argument&) {
    missing_credentials_failed = true;
  }
  require(missing_credentials_failed, "missing ICE credentials guard");
}

}  // namespace

int main() {
  try {
    test_settings_helpers();
    test_input_protocol_helpers();
    test_input_encoder_v2_bytes();
    test_input_encoder_gamepad();
    test_core_facade();
    test_service_builders();
    test_sdp_helpers();
    test_fixture_backed_auth_catalog_subscription_session_services();
    test_fixture_backed_signaling_client();
    test_stream_client_proof_backend();
    test_stream_client_negotiation_artifacts();
    test_stream_client_lifecycle_guards();
  } catch (const std::exception& error) {
    std::cerr << error.what() << "\n";
    return EXIT_FAILURE;
  }

  return EXIT_SUCCESS;
}
