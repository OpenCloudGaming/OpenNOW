#include "opennow/native/protocol.hpp"

#include <algorithm>
#include <regex>

namespace opennow::native {

namespace {

std::string UnescapeJsonString(std::string_view input) {
  std::string out;
  out.reserve(input.size());
  bool escaping = false;
  for (const char ch : input) {
    if (!escaping) {
      if (ch == '\\') {
        escaping = true;
      } else {
        out.push_back(ch);
      }
      continue;
    }

    switch (ch) {
      case '\"': out.push_back('\"'); break;
      case '\\': out.push_back('\\'); break;
      case 'n': out.push_back('\n'); break;
      case 'r': out.push_back('\r'); break;
      case 't': out.push_back('\t'); break;
      case 'b': out.push_back('\b'); break;
      case 'f': out.push_back('\f'); break;
      case '/': out.push_back('/'); break;
      default:
        out.push_back(ch);
        break;
    }
    escaping = false;
  }
  if (escaping) {
    out.push_back('\\');
  }
  return out;
}

}  // namespace

std::vector<std::uint8_t> FrameJson(std::string_view json) {
  const auto size = static_cast<std::uint32_t>(json.size());
  std::vector<std::uint8_t> framed(4 + json.size());
  framed[0] = static_cast<std::uint8_t>((size >> 24) & 0xff);
  framed[1] = static_cast<std::uint8_t>((size >> 16) & 0xff);
  framed[2] = static_cast<std::uint8_t>((size >> 8) & 0xff);
  framed[3] = static_cast<std::uint8_t>(size & 0xff);
  std::copy(json.begin(), json.end(), framed.begin() + 4);
  return framed;
}

std::string EscapeJson(std::string_view input) {
  std::string escaped;
  escaped.reserve(input.size());
  for (const char ch : input) {
    switch (ch) {
      case '\\': escaped += "\\\\"; break;
      case '"': escaped += "\\\""; break;
      case '\n': escaped += "\\n"; break;
      case '\r': escaped += "\\r"; break;
      case '\t': escaped += "\\t"; break;
      default: escaped.push_back(ch); break;
    }
  }
  return escaped;
}

std::string BuildEnvelope(std::string_view type, std::string_view payloadJson) {
  return std::string("{\"version\":1,\"type\":\"") + EscapeJson(type) + "\",\"payload\":" + std::string(payloadJson) + "}";
}

std::optional<std::string> FindJsonString(std::string_view json, std::string_view key) {
  const auto pattern = std::string("\\\"") + std::string(key) + "\\\"\\s*:\\s*\\\"((?:\\\\.|[^\\\"])*)\\\"";
  std::smatch match;
  const std::string value(json);
  if (std::regex_search(value, match, std::regex(pattern))) {
    return UnescapeJsonString(match[1].str());
  }
  return std::nullopt;
}

std::optional<int> FindJsonInt(std::string_view json, std::string_view key) {
  const auto pattern = std::string("\"") + std::string(key) + R"("\s*:\s*(-?\d+))";
  std::smatch match;
  const std::string value(json);
  if (std::regex_search(value, match, std::regex(pattern))) {
    return std::stoi(match[1].str());
  }
  return std::nullopt;
}

std::optional<bool> FindJsonBool(std::string_view json, std::string_view key) {
  const auto pattern = std::string("\"") + std::string(key) + R"("\s*:\s*(true|false))";
  std::smatch match;
  const std::string value(json);
  if (std::regex_search(value, match, std::regex(pattern))) {
    return match[1].str() == "true";
  }
  return std::nullopt;
}

}  // namespace opennow::native
