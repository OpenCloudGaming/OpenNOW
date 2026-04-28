#include "opennow/service_protocol.hpp"

#include <algorithm>
#include <cctype>
#include <map>
#include <sstream>
#include <stdexcept>
#include <utility>
#include <variant>

#include "opennow/service_builders.hpp"

namespace opennow {

namespace {

struct JsonValue {
  using Object = std::map<std::string, JsonValue>;
  using Array = std::vector<JsonValue>;
  std::variant<std::nullptr_t, bool, double, std::string, Object, Array> value;

  bool is_object() const { return std::holds_alternative<Object>(value); }
  bool is_array() const { return std::holds_alternative<Array>(value); }

  const Object& object() const {
    if (!is_object()) {
      throw std::runtime_error("Expected JSON object");
    }
    return std::get<Object>(value);
  }

  const Array& array() const {
    if (!is_array()) {
      throw std::runtime_error("Expected JSON array");
    }
    return std::get<Array>(value);
  }
};

class JsonParser {
 public:
  explicit JsonParser(const std::string& text) : text_(text) {}

  JsonValue parse() {
    auto value = parse_value();
    skip_ws();
    if (pos_ != text_.size()) {
      throw std::runtime_error("Unexpected trailing JSON data");
    }
    return value;
  }

 private:
  JsonValue parse_value() {
    skip_ws();
    if (pos_ >= text_.size()) {
      throw std::runtime_error("Unexpected end of JSON");
    }
    const char ch = text_[pos_];
    if (ch == '{') {
      return JsonValue{parse_object()};
    }
    if (ch == '[') {
      return JsonValue{parse_array()};
    }
    if (ch == '"') {
      return JsonValue{parse_string()};
    }
    if (ch == 't') {
      consume_literal("true");
      return JsonValue{true};
    }
    if (ch == 'f') {
      consume_literal("false");
      return JsonValue{false};
    }
    if (ch == 'n') {
      consume_literal("null");
      return JsonValue{nullptr};
    }
    return JsonValue{parse_number()};
  }

  JsonValue::Object parse_object() {
    JsonValue::Object object;
    expect('{');
    skip_ws();
    if (consume('}')) {
      return object;
    }
    while (true) {
      const auto key = parse_string();
      skip_ws();
      expect(':');
      object.emplace(key, parse_value());
      skip_ws();
      if (consume('}')) {
        return object;
      }
      expect(',');
    }
  }

  JsonValue::Array parse_array() {
    JsonValue::Array array;
    expect('[');
    skip_ws();
    if (consume(']')) {
      return array;
    }
    while (true) {
      array.push_back(parse_value());
      skip_ws();
      if (consume(']')) {
        return array;
      }
      expect(',');
    }
  }

  std::string parse_string() {
    expect('"');
    std::string out;
    while (pos_ < text_.size()) {
      const char ch = text_[pos_++];
      if (ch == '"') {
        return out;
      }
      if (ch != '\\') {
        out += ch;
        continue;
      }
      if (pos_ >= text_.size()) {
        throw std::runtime_error("Invalid JSON escape");
      }
      const char escaped = text_[pos_++];
      switch (escaped) {
        case '"':
        case '\\':
        case '/':
          out += escaped;
          break;
        case 'b':
          out += '\b';
          break;
        case 'f':
          out += '\f';
          break;
        case 'n':
          out += '\n';
          break;
        case 'r':
          out += '\r';
          break;
        case 't':
          out += '\t';
          break;
        default:
          throw std::runtime_error("Unsupported JSON escape");
      }
    }
    throw std::runtime_error("Unterminated JSON string");
  }

  double parse_number() {
    const auto start = pos_;
    if (text_[pos_] == '-') {
      ++pos_;
    }
    while (pos_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[pos_]))) {
      ++pos_;
    }
    if (pos_ < text_.size() && text_[pos_] == '.') {
      ++pos_;
      while (pos_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[pos_]))) {
        ++pos_;
      }
    }
    if (pos_ < text_.size() && (text_[pos_] == 'e' || text_[pos_] == 'E')) {
      ++pos_;
      if (pos_ < text_.size() && (text_[pos_] == '+' || text_[pos_] == '-')) {
        ++pos_;
      }
      while (pos_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[pos_]))) {
        ++pos_;
      }
    }
    const auto token = text_.substr(start, pos_ - start);
    if (token.empty() || token == "-") {
      throw std::runtime_error("Invalid JSON number");
    }
    return std::stod(token);
  }

  void skip_ws() {
    while (pos_ < text_.size() && std::isspace(static_cast<unsigned char>(text_[pos_]))) {
      ++pos_;
    }
  }

  bool consume(char expected) {
    skip_ws();
    if (pos_ < text_.size() && text_[pos_] == expected) {
      ++pos_;
      return true;
    }
    return false;
  }

  void expect(char expected) {
    if (!consume(expected)) {
      throw std::runtime_error("Unexpected JSON token");
    }
  }

  void consume_literal(const char* literal) {
    while (*literal != '\0') {
      if (pos_ >= text_.size() || text_[pos_] != *literal) {
        throw std::runtime_error("Invalid JSON literal");
      }
      ++pos_;
      ++literal;
    }
  }

  const std::string& text_;
  std::size_t pos_ = 0;
};

JsonValue parse_json(const std::string& body) {
  return JsonParser(body).parse();
}

const JsonValue* find(const JsonValue::Object& object, const std::string& key) {
  const auto it = object.find(key);
  return it == object.end() ? nullptr : &it->second;
}

std::optional<std::string> string_at(const JsonValue::Object& object, const std::string& key) {
  const auto* value = find(object, key);
  if (!value || !std::holds_alternative<std::string>(value->value)) {
    return std::nullopt;
  }
  return std::get<std::string>(value->value);
}

std::optional<double> number_at(const JsonValue::Object& object, const std::string& key) {
  const auto* value = find(object, key);
  if (!value || !std::holds_alternative<double>(value->value)) {
    return std::nullopt;
  }
  return std::get<double>(value->value);
}

std::optional<int> int_at(const JsonValue::Object& object, const std::string& key) {
  const auto value = number_at(object, key);
  if (!value) {
    return std::nullopt;
  }
  return static_cast<int>(*value);
}

std::optional<bool> bool_at(const JsonValue::Object& object, const std::string& key) {
  const auto* value = find(object, key);
  if (!value || !std::holds_alternative<bool>(value->value)) {
    return std::nullopt;
  }
  return std::get<bool>(value->value);
}

const JsonValue::Object* object_at(const JsonValue::Object& object, const std::string& key) {
  const auto* value = find(object, key);
  return value && value->is_object() ? &value->object() : nullptr;
}

const JsonValue::Array* array_at(const JsonValue::Object& object, const std::string& key) {
  const auto* value = find(object, key);
  return value && value->is_array() ? &value->array() : nullptr;
}

std::string json_escape(const std::string& value) {
  std::string escaped;
  escaped.reserve(value.size() + 8);
  for (const char ch : value) {
    switch (ch) {
      case '\\':
        escaped += "\\\\";
        break;
      case '"':
        escaped += "\\\"";
        break;
      case '\n':
        escaped += "\\n";
        break;
      case '\r':
        escaped += "\\r";
        break;
      case '\t':
        escaped += "\\t";
        break;
      default:
        escaped += ch;
        break;
    }
  }
  return escaped;
}

std::string quote(const std::string& value) {
  return "\"" + json_escape(value) + "\"";
}

std::string normalize_base_url(std::string base) {
  if (base.empty()) {
    return "/";
  }
  if (base.back() != '/') {
    base.push_back('/');
  }
  return base;
}

std::string subscription_url(const SubscriptionFetchRequest& request) {
  const auto region = request.vpc_id.value_or("NP-AMS-08");
  std::ostringstream url;
  url << "https://mes.geforcenow.com/v4/subscriptions"
      << "?serviceName=gfn_pc&languageCode=en_US"
      << "&vpcId=" << region
      << "&userId=" << request.user_id;
  return url.str();
}

std::map<std::string, std::string> auth_headers(const std::string& token) {
  return {
      {"Authorization", "GFNJWT " + token},
      {"Accept", "application/json"},
      {"Content-Type", "application/json"},
      {"nv-client-id", kNvidiaClientId},
      {"nv-client-type", "NATIVE"},
      {"nv-client-version", kGfnClientVersion},
      {"nv-device-os", "MACOS"},
      {"nv-device-type", "DESKTOP"},
  };
}

LoginProvider parse_provider(const JsonValue::Object& object) {
  LoginProvider provider;
  provider.idp_id = string_at(object, "idpId").value_or(string_at(object, "idp_id").value_or(""));
  provider.code = string_at(object, "code").value_or(provider.idp_id);
  provider.display_name = string_at(object, "displayName").value_or(
      string_at(object, "name").value_or(provider.code));
  provider.streaming_service_url = normalize_base_url(
      string_at(object, "streamingServiceUrl").value_or(
          string_at(object, "streaming_service_url")
              .value_or("https://prod.cloudmatchbeta.nvidiagrid.net/")));
  provider.priority = int_at(object, "priority").value_or(0);
  return provider;
}

AuthTokens parse_tokens(const JsonValue::Object& object) {
  AuthTokens tokens;
  tokens.access_token = string_at(object, "accessToken").value_or(
      string_at(object, "access_token").value_or(""));
  tokens.refresh_token = string_at(object, "refreshToken").value_or(
      string_at(object, "refresh_token").value_or(""));
  if (tokens.refresh_token && tokens.refresh_token->empty()) {
    tokens.refresh_token.reset();
  }
  tokens.id_token = string_at(object, "idToken").value_or(string_at(object, "id_token").value_or(""));
  if (tokens.id_token && tokens.id_token->empty()) {
    tokens.id_token.reset();
  }
  tokens.client_token = string_at(object, "clientToken").value_or(
      string_at(object, "client_token").value_or(""));
  if (tokens.client_token && tokens.client_token->empty()) {
    tokens.client_token.reset();
  }
  tokens.expires_at_ms = static_cast<std::int64_t>(
      number_at(object, "expiresAtMs").value_or(number_at(object, "expires_at_ms").value_or(0)));
  if (const auto value = number_at(object, "clientTokenExpiresAtMs")) {
    tokens.client_token_expires_at_ms = static_cast<std::int64_t>(*value);
  }
  if (const auto value = number_at(object, "clientTokenLifetimeMs")) {
    tokens.client_token_lifetime_ms = static_cast<std::int64_t>(*value);
  }
  return tokens;
}

AuthUser parse_user(const JsonValue::Object& object) {
  AuthUser user;
  user.user_id = string_at(object, "userId").value_or(string_at(object, "user_id").value_or(""));
  user.display_name = string_at(object, "displayName").value_or(
      string_at(object, "display_name").value_or(user.user_id));
  user.email = string_at(object, "email");
  user.avatar_url = string_at(object, "avatarUrl").value_or(string_at(object, "avatar_url").value_or(""));
  if (user.avatar_url && user.avatar_url->empty()) {
    user.avatar_url.reset();
  }
  user.membership_tier = string_at(object, "membershipTier").value_or(
      string_at(object, "membership_tier").value_or("FREE"));
  return user;
}

std::vector<std::string> parse_string_array(const JsonValue::Object& object, const std::string& key) {
  std::vector<std::string> values;
  const auto* array = array_at(object, key);
  if (!array) {
    return values;
  }
  for (const auto& item : *array) {
    if (std::holds_alternative<std::string>(item.value)) {
      values.push_back(std::get<std::string>(item.value));
    }
  }
  return values;
}

GameVariant parse_variant(const JsonValue::Object& object) {
  GameVariant variant;
  variant.id = string_at(object, "id").value_or("");
  variant.store = string_at(object, "store").value_or("Unknown");
  variant.supported_controls = parse_string_array(object, "supportedControls");
  if (variant.supported_controls.empty()) {
    variant.supported_controls = parse_string_array(object, "supported_controls");
  }
  variant.library_status = string_at(object, "libraryStatus").value_or(
      string_at(object, "library_status").value_or(""));
  if (variant.library_status && variant.library_status->empty()) {
    variant.library_status.reset();
  }
  return variant;
}

GameInfo parse_game(const JsonValue::Object& object) {
  GameInfo game;
  game.id = string_at(object, "id").value_or("");
  game.uuid = string_at(object, "uuid");
  game.launch_app_id = string_at(object, "launchAppId").value_or(
      string_at(object, "launch_app_id").value_or(""));
  if (game.launch_app_id && game.launch_app_id->empty()) {
    game.launch_app_id.reset();
  }
  game.title = string_at(object, "title").value_or(game.id);
  game.selected_variant_index = int_at(object, "selectedVariantIndex").value_or(
      int_at(object, "selected_variant_index").value_or(0));

  if (const auto* variants = array_at(object, "variants")) {
    for (const auto& item : *variants) {
      if (item.is_object()) {
        game.variants.push_back(parse_variant(item.object()));
      }
    }
  }
  return game;
}

std::vector<IceServer> parse_ice_servers(const JsonValue::Object& object) {
  std::vector<IceServer> servers;
  const auto* array = array_at(object, "iceServers");
  if (!array) {
    return servers;
  }
  for (const auto& item : *array) {
    if (!item.is_object()) {
      continue;
    }
    const auto& server_object = item.object();
    IceServer server;
    if (const auto* urls = array_at(server_object, "urls")) {
      for (const auto& url : *urls) {
        if (std::holds_alternative<std::string>(url.value)) {
          server.urls.push_back(std::get<std::string>(url.value));
        }
      }
    } else if (auto url = string_at(server_object, "url")) {
      server.urls.push_back(*url);
    }
    server.username = string_at(server_object, "username");
    server.credential = string_at(server_object, "credential");
    servers.push_back(server);
  }
  return servers;
}

}  // namespace

HttpRequest build_providers_request(const std::string& service_base_url) {
  return {HttpMethod::Get, normalize_base_url(service_base_url) + "v2/loginProviders", {}, ""};
}

HttpRequest build_auth_session_request(
    const std::string& service_base_url,
    const AuthLoginRequest& request) {
  std::ostringstream body;
  body << "{";
  if (request.provider_idp_id) {
    body << "\"providerIdpId\":" << quote(*request.provider_idp_id);
  }
  body << "}";
  return {
      HttpMethod::Post,
      normalize_base_url(service_base_url) + "v1/auth/session",
      {{"Content-Type", "application/json"}},
      body.str()};
}

HttpRequest build_catalog_request(
    const std::string& service_base_url,
    CatalogRequestKind kind,
    const CatalogBrowseRequest& request) {
  std::string path = "v1/catalog/browse";
  if (kind == CatalogRequestKind::Library) {
    path = "v1/catalog/library";
  } else if (kind == CatalogRequestKind::Public) {
    path = "v1/catalog/public";
  }

  std::ostringstream body;
  body << "{";
  bool needs_comma = false;
  const auto append_string = [&](const char* key, const std::optional<std::string>& value) {
    if (!value) {
      return;
    }
    body << (needs_comma ? "," : "") << quote(key) << ":" << quote(*value);
    needs_comma = true;
  };
  append_string("searchQuery", request.search_query);
  append_string("sortId", request.sort_id);
  if (request.fetch_count) {
    body << (needs_comma ? "," : "") << "\"fetchCount\":" << *request.fetch_count;
    needs_comma = true;
  }
  if (!request.filter_ids.empty()) {
    body << (needs_comma ? "," : "") << "\"filterIds\":[";
    for (std::size_t i = 0; i < request.filter_ids.size(); ++i) {
      body << (i == 0 ? "" : ",") << quote(request.filter_ids[i]);
    }
    body << "]";
  }
  body << "}";

  auto headers = request.token ? auth_headers(*request.token) : std::map<std::string, std::string>{};
  return {HttpMethod::Post, normalize_base_url(service_base_url) + path, headers, body.str()};
}

HttpRequest build_subscription_request(const SubscriptionFetchRequest& request) {
  return {HttpMethod::Get, subscription_url(request), auth_headers(request.token), ""};
}

HttpRequest build_session_create_request(const SessionCreateRequest& request) {
  std::ostringstream body;
  body << "{\"appId\":" << quote(request.app_id)
       << ",\"region\":" << quote(request.region)
       << ",\"resolution\":" << quote(request.resolution)
       << ",\"fps\":" << request.fps
       << ",\"maxBitrateKbps\":" << request.max_bitrate_kbps << "}";
  auto headers = auth_headers(request.access_token);
  headers["x-client-token"] = request.client_token;
  return {HttpMethod::Post, normalize_base_url(request.streaming_base_url) + "v1/session", headers, body.str()};
}

HttpRequest build_session_poll_request(
    const std::string& streaming_base_url,
    const std::string& session_id) {
  return {HttpMethod::Get, normalize_base_url(streaming_base_url) + "v1/session/" + session_id, {}, ""};
}

HttpRequest build_session_stop_request(
    const std::string& streaming_base_url,
    const std::string& session_id) {
  return {HttpMethod::Delete, normalize_base_url(streaming_base_url) + "v1/session/" + session_id, {}, ""};
}

std::vector<LoginProvider> parse_login_providers_response(const std::string& body) {
  const auto root = parse_json(body);
  const auto& object = root.object();
  const auto* providers = array_at(object, "providers");
  if (!providers) {
    providers = array_at(object, "loginProviders");
  }
  std::vector<LoginProvider> result;
  if (!providers) {
    return result;
  }
  for (const auto& item : *providers) {
    if (item.is_object()) {
      result.push_back(parse_provider(item.object()));
    }
  }
  return result;
}

AuthSession parse_auth_session_response(const std::string& body) {
  const auto root = parse_json(body);
  const auto& object = root.object();
  const auto* session = object_at(object, "session");
  const auto& source = session ? *session : object;
  const auto* provider = object_at(source, "provider");
  const auto* tokens = object_at(source, "tokens");
  const auto* user = object_at(source, "user");
  if (!provider || !tokens || !user) {
    throw std::runtime_error("Auth session response is missing provider, tokens, or user");
  }
  return {parse_provider(*provider), parse_tokens(*tokens), parse_user(*user)};
}

std::vector<GameInfo> parse_catalog_games_response(const std::string& body) {
  const auto root = parse_json(body);
  const auto& object = root.object();
  const JsonValue::Array* games = array_at(object, "games");
  if (!games) {
    games = array_at(object, "items");
  }
  std::vector<GameInfo> result;
  if (!games) {
    return result;
  }
  for (const auto& item : *games) {
    if (item.is_object()) {
      result.push_back(parse_game(item.object()));
    }
  }
  return result;
}

SubscriptionInfo parse_subscription_response(
    const std::string& body,
    const std::optional<std::string>& server_region_id) {
  const auto root = parse_json(body);
  const auto& object = root.object();
  const double allotted_minutes = number_at(object, "allottedTimeInMinutes").value_or(0);
  const double purchased_minutes = number_at(object, "purchasedTimeInMinutes").value_or(0);
  const double rolled_over_minutes = number_at(object, "rolledOverTimeInMinutes").value_or(0);
  const double fallback_total_minutes = allotted_minutes + purchased_minutes + rolled_over_minutes;
  const double total_minutes = number_at(object, "totalTimeInMinutes").value_or(fallback_total_minutes);
  const double remaining_minutes = number_at(object, "remainingTimeInMinutes").value_or(0);
  const double used_minutes = std::max(total_minutes - remaining_minutes, 0.0);

  SubscriptionInfo info;
  info.membership_tier = string_at(object, "membershipTier").value_or("FREE");
  info.subscription_type = string_at(object, "type");
  info.subscription_sub_type = string_at(object, "subType");
  info.allotted_hours = allotted_minutes / 60.0;
  info.purchased_hours = purchased_minutes / 60.0;
  info.rolled_over_hours = rolled_over_minutes / 60.0;
  info.used_hours = used_minutes / 60.0;
  info.remaining_hours = remaining_minutes / 60.0;
  info.total_hours = total_minutes / 60.0;
  info.first_entitlement_start_date_time = string_at(object, "firstEntitlementStartDateTime");
  info.server_region_id = server_region_id;
  info.current_span_start_date_time = string_at(object, "currentSpanStartDateTime");
  info.current_span_end_date_time = string_at(object, "currentSpanEndDateTime");
  info.is_unlimited = info.subscription_sub_type == "UNLIMITED";

  if (const auto* notifications = object_at(object, "notifications")) {
    info.notify_user_when_time_remaining_in_minutes =
        int_at(*notifications, "notifyUserWhenTimeRemainingInMinutes");
    info.notify_user_on_session_when_remaining_time_in_minutes =
        int_at(*notifications, "notifyUserOnSessionWhenRemainingTimeInMinutes");
  }
  if (const auto* state = object_at(object, "currentSubscriptionState")) {
    info.state = string_at(*state, "state");
    info.is_game_play_allowed = bool_at(*state, "isGamePlayAllowed");
  }
  if (const auto* features = object_at(object, "features")) {
    if (const auto* resolutions = array_at(*features, "resolutions")) {
      for (const auto& item : *resolutions) {
        if (!item.is_object()) {
          continue;
        }
        const auto& resolution = item.object();
        info.entitled_resolutions.push_back({
            int_at(resolution, "widthInPixels").value_or(0),
            int_at(resolution, "heightInPixels").value_or(0),
            int_at(resolution, "framesPerSecond").value_or(0),
        });
      }
    }
  }
  std::sort(info.entitled_resolutions.begin(), info.entitled_resolutions.end(),
            [](const EntitledResolution& left, const EntitledResolution& right) {
              if (left.width != right.width) {
                return left.width > right.width;
              }
              if (left.height != right.height) {
                return left.height > right.height;
              }
              return left.fps > right.fps;
            });

  if (const auto* addons = array_at(object, "addons")) {
    for (const auto& item : *addons) {
      if (!item.is_object()) {
        continue;
      }
      const auto& addon = item.object();
      if (string_at(addon, "type") != "STORAGE" ||
          string_at(addon, "subType") != "PERMANENT_STORAGE" ||
          string_at(addon, "status") != "OK") {
        continue;
      }
      StorageAddon storage;
      if (const auto* attributes = array_at(addon, "attributes")) {
        for (const auto& attr_item : *attributes) {
          if (!attr_item.is_object()) {
            continue;
          }
          const auto& attr = attr_item.object();
          const auto key = string_at(attr, "key").value_or("");
          const auto text_value = string_at(attr, "textValue");
          if (key == "TOTAL_STORAGE_SIZE_IN_GB" && text_value) {
            storage.size_gb = std::stoi(*text_value);
          } else if (key == "USED_STORAGE_SIZE_IN_GB" && text_value) {
            storage.used_gb = std::stoi(*text_value);
          } else if (key == "STORAGE_METRO_REGION_NAME") {
            storage.region_name = text_value;
          } else if (key == "STORAGE_METRO_REGION") {
            storage.region_code = text_value;
          }
        }
      }
      info.storage_addon = storage;
      break;
    }
  }

  return info;
}

SessionInfo parse_session_response(const std::string& body) {
  const auto root = parse_json(body);
  const auto& object = root.object();
  const auto* session = object_at(object, "session");
  const auto& source = session ? *session : object;
  SessionInfo info;
  info.id = string_at(source, "sessionId").value_or(string_at(source, "id").value_or(""));
  info.app_id = string_at(source, "appId").value_or(string_at(source, "app_id").value_or(""));
  info.server_ip = string_at(source, "serverIp").value_or(string_at(source, "server_ip").value_or(""));
  if (info.server_ip && info.server_ip->empty()) {
    info.server_ip.reset();
  }
  info.signaling_server = string_at(source, "signalingServer").value_or(
      string_at(source, "signaling_server").value_or(""));
  if (info.signaling_server && info.signaling_server->empty()) {
    info.signaling_server.reset();
  }
  info.signaling_url = string_at(source, "signalingUrl").value_or(
      string_at(source, "signaling_url").value_or(""));
  if (info.signaling_url && info.signaling_url->empty()) {
    info.signaling_url.reset();
  }
  info.ice_servers = parse_ice_servers(source);
  return info;
}

ParsedSignalingMessage parse_signaling_message(
    const std::string& text,
    int peer_id) {
  ParsedSignalingMessage parsed;
  JsonValue root;
  try {
    root = parse_json(text);
  } catch (...) {
    parsed.events.push_back({"log", "Ignoring non-JSON signaling packet"});
    return parsed;
  }

  const auto& object = root.object();
  if (const auto ackid = int_at(object, "ackid")) {
    bool should_ack = true;
    if (const auto* peer_info = object_at(object, "peer_info")) {
      should_ack = int_at(*peer_info, "id").value_or(-1) != peer_id;
    }
    if (should_ack) {
      parsed.outbound_messages.push_back("{\"ack\":" + std::to_string(*ackid) + "}");
    }
  }

  if (number_at(object, "hb")) {
    parsed.outbound_messages.push_back("{\"hb\":1}");
    return parsed;
  }

  const auto* peer_msg = object_at(object, "peer_msg");
  const auto peer_payload_text = peer_msg ? string_at(*peer_msg, "msg") : std::nullopt;
  if (!peer_payload_text) {
    return parsed;
  }

  JsonValue peer_payload;
  try {
    peer_payload = parse_json(*peer_payload_text);
  } catch (...) {
    parsed.events.push_back({"log", "Received non-JSON peer payload"});
    return parsed;
  }
  const auto& peer_object = peer_payload.object();
  if (string_at(peer_object, "type") == "offer") {
    if (auto sdp = string_at(peer_object, "sdp")) {
      parsed.events.push_back({"offer", *sdp});
    }
    return parsed;
  }
  if (auto candidate = string_at(peer_object, "candidate")) {
    parsed.events.push_back({"remote-ice", *candidate});
  }
  return parsed;
}

}  // namespace opennow
