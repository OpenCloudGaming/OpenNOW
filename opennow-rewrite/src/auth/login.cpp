#include "opennow/auth/login.hpp"

#include <array>
#include <chrono>
#include <cstdint>
#include <iomanip>
#include <optional>
#include <random>
#include <sstream>
#include <string>
#include <vector>

#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
#pragma comment(lib, "Ws2_32.lib")
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#endif

namespace {
constexpr auto kClientId = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
constexpr auto kScopes = "openid consent email tk_client age";
constexpr std::array<uint16_t, 5> kRedirectPorts {2259, 6460, 7119, 8870, 9096};

constexpr std::array<uint32_t, 64> kTable {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
};

uint32_t rotr(const uint32_t x, const uint32_t n) { return (x >> n) | (x << (32 - n)); }

std::array<uint8_t, 32> sha256(const std::string& input) {
    std::array<uint32_t, 8> h {
        0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19,
    };

    std::vector<uint8_t> msg(input.begin(), input.end());
    const uint64_t bit_len = static_cast<uint64_t>(msg.size()) * 8;
    msg.push_back(0x80);
    while ((msg.size() % 64) != 56) {
        msg.push_back(0x00);
    }
    for (int i = 7; i >= 0; --i) {
        msg.push_back(static_cast<uint8_t>((bit_len >> (i * 8)) & 0xff));
    }

    for (std::size_t offset = 0; offset < msg.size(); offset += 64) {
        std::array<uint32_t, 64> w {};
        for (int i = 0; i < 16; ++i) {
            const auto base = offset + static_cast<std::size_t>(i) * 4;
            w[i] = (static_cast<uint32_t>(msg[base]) << 24) |
                   (static_cast<uint32_t>(msg[base + 1]) << 16) |
                   (static_cast<uint32_t>(msg[base + 2]) << 8) |
                   static_cast<uint32_t>(msg[base + 3]);
        }
        for (int i = 16; i < 64; ++i) {
            const uint32_t s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
            const uint32_t s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16] + s0 + w[i - 7] + s1;
        }

        uint32_t a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], hh = h[7];

        for (int i = 0; i < 64; ++i) {
            const uint32_t s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
            const uint32_t ch = (e & f) ^ ((~e) & g);
            const uint32_t temp1 = hh + s1 + ch + kTable[i] + w[i];
            const uint32_t s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
            const uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
            const uint32_t temp2 = s0 + maj;

            hh = g;
            g = f;
            f = e;
            e = d + temp1;
            d = c;
            c = b;
            b = a;
            a = temp1 + temp2;
        }

        h[0] += a; h[1] += b; h[2] += c; h[3] += d;
        h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
    }

    std::array<uint8_t, 32> out {};
    for (int i = 0; i < 8; ++i) {
        out[static_cast<std::size_t>(i) * 4] = static_cast<uint8_t>((h[i] >> 24) & 0xff);
        out[static_cast<std::size_t>(i) * 4 + 1] = static_cast<uint8_t>((h[i] >> 16) & 0xff);
        out[static_cast<std::size_t>(i) * 4 + 2] = static_cast<uint8_t>((h[i] >> 8) & 0xff);
        out[static_cast<std::size_t>(i) * 4 + 3] = static_cast<uint8_t>(h[i] & 0xff);
    }
    return out;
}

std::string base64url(const uint8_t* data, std::size_t len) {
    static constexpr char alphabet[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    std::string out;
    out.reserve((len * 4 + 2) / 3);

    std::size_t i = 0;
    while (i + 2 < len) {
        const uint32_t chunk = (static_cast<uint32_t>(data[i]) << 16)
                             | (static_cast<uint32_t>(data[i + 1]) << 8)
                             | static_cast<uint32_t>(data[i + 2]);
        out.push_back(alphabet[(chunk >> 18) & 0x3f]);
        out.push_back(alphabet[(chunk >> 12) & 0x3f]);
        out.push_back(alphabet[(chunk >> 6) & 0x3f]);
        out.push_back(alphabet[chunk & 0x3f]);
        i += 3;
    }

    if (i < len) {
        const uint32_t b0 = static_cast<uint32_t>(data[i]);
        out.push_back(alphabet[(b0 >> 2) & 0x3f]);
        if (i + 1 < len) {
            const uint32_t b1 = static_cast<uint32_t>(data[i + 1]);
            out.push_back(alphabet[((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0f)]);
            out.push_back(alphabet[(b1 & 0x0f) << 2]);
        } else {
            out.push_back(alphabet[(b0 & 0x03) << 4]);
        }
    }

    return out;
}

std::string url_encode(const std::string& raw) {
    std::ostringstream encoded;
    for (const unsigned char c : raw) {
        if ((c >= 'A' && c <= 'Z') ||
            (c >= 'a' && c <= 'z') ||
            (c >= '0' && c <= '9') ||
            c == '-' || c == '_' || c == '.' || c == '~') {
            encoded << c;
            continue;
        }
        encoded << '%' << std::uppercase << std::hex << std::setw(2) << std::setfill('0')
                << static_cast<int>(c) << std::nouppercase << std::dec;
    }
    return encoded.str();
}

std::string url_decode(std::string value) {
    std::string out;
    out.reserve(value.size());
    for (std::size_t i = 0; i < value.size(); ++i) {
        if (value[i] == '%' && i + 2 < value.size()) {
            const std::string hex = value.substr(i + 1, 2);
            const auto decoded = static_cast<char>(std::stoi(hex, nullptr, 16));
            out.push_back(decoded);
            i += 2;
        } else if (value[i] == '+') {
            out.push_back(' ');
        } else {
            out.push_back(value[i]);
        }
    }
    return out;
}

std::string random_alnum(const std::size_t len) {
    static constexpr char charset[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    static thread_local std::mt19937_64 rng(std::random_device{}());
    std::uniform_int_distribution<std::size_t> dist(0, sizeof(charset) - 2);

    std::string result;
    result.reserve(len);
    for (std::size_t i = 0; i < len; ++i) {
        result.push_back(charset[dist(rng)]);
    }
    return result;
}

#ifdef _WIN32
using socket_t = SOCKET;
constexpr socket_t kInvalidSocket = INVALID_SOCKET;
#else
using socket_t = int;
constexpr socket_t kInvalidSocket = -1;
#endif

void close_socket(socket_t s) {
#ifdef _WIN32
    closesocket(s);
#else
    close(s);
#endif
}

bool bind_localhost_port(socket_t sock, const uint16_t port) {
    sockaddr_in addr {};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(port);
    return bind(sock, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) == 0;
}
} // namespace

namespace opennow::auth {

LoginProvider LoginProvider::nvidia_default() {
    return {
        .idp_id = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
        .login_provider_code = "NVIDIA",
        .login_provider_display_name = "NVIDIA",
        .login_provider = "NVIDIA",
        .streaming_service_url = "https://prod.cloudmatchbeta.nvidiagrid.net/",
        .login_provider_priority = 0,
    };
}

bool LoginProvider::is_alliance_partner() const {
    return login_provider_code != "NVIDIA";
}

PkceChallenge LoginService::make_pkce_challenge() const {
    const std::string verifier = random_alnum(64);
    const auto digest = sha256(verifier);
    return {
        .verifier = verifier,
        .challenge = base64url(digest.data(), digest.size()),
    };
}

std::string LoginService::build_auth_url(
    const PkceChallenge& pkce,
    const uint16_t callback_port,
    const LoginProvider& provider
) const {
    const std::string redirect_uri = "http://localhost:" + std::to_string(callback_port);

    std::ostringstream url;
    url << "https://login.nvidia.com/authorize?"
        << "response_type=code"
        << "&device_id=" << url_encode(device_id())
        << "&scope=" << url_encode(kScopes)
        << "&client_id=" << url_encode(kClientId)
        << "&redirect_uri=" << url_encode(redirect_uri)
        << "&ui_locales=en_US"
        << "&nonce=" << url_encode(generate_nonce())
        << "&prompt=select_account"
        << "&code_challenge=" << url_encode(pkce.challenge)
        << "&code_challenge_method=S256"
        << "&idp_id=" << url_encode(provider.idp_id);

    return url.str();
}

std::optional<uint16_t> LoginService::pick_callback_port() const {
#ifdef _WIN32
    WSADATA wsa_data {};
    if (WSAStartup(MAKEWORD(2, 2), &wsa_data) != 0) {
        return std::nullopt;
    }
#endif

    for (const auto port : kRedirectPorts) {
        const socket_t s = socket(AF_INET, SOCK_STREAM, 0);
        if (s == kInvalidSocket) {
            continue;
        }

        const bool ok = bind_localhost_port(s, port);
        close_socket(s);
        if (ok) {
#ifdef _WIN32
            WSACleanup();
#endif
            return port;
        }
    }

#ifdef _WIN32
    WSACleanup();
#endif
    return std::nullopt;
}

std::optional<std::string> LoginService::extract_code_from_callback_target(
    const std::string& callback_target
) const {
    const auto query_pos = callback_target.find('?');
    if (query_pos == std::string::npos) {
        return std::nullopt;
    }

    const std::string query = callback_target.substr(query_pos + 1);
    std::size_t cursor = 0;
    while (cursor < query.size()) {
        const auto amp = query.find('&', cursor);
        const std::string param = query.substr(
            cursor,
            amp == std::string::npos ? std::string::npos : amp - cursor
        );

        constexpr auto prefix = "code=";
        if (param.rfind(prefix, 0) == 0 && param.size() > 5) {
            return url_decode(param.substr(5));
        }

        if (amp == std::string::npos) {
            break;
        }
        cursor = amp + 1;
    }

    return std::nullopt;
}

std::optional<std::string> LoginService::wait_for_callback_code(const uint16_t callback_port) const {
#ifdef _WIN32
    WSADATA wsa_data {};
    if (WSAStartup(MAKEWORD(2, 2), &wsa_data) != 0) {
        return std::nullopt;
    }
#endif

    const socket_t server = socket(AF_INET, SOCK_STREAM, 0);
    if (server == kInvalidSocket) {
#ifdef _WIN32
        WSACleanup();
#endif
        return std::nullopt;
    }

    int reuse = 1;
    setsockopt(server, SOL_SOCKET, SO_REUSEADDR, reinterpret_cast<const char*>(&reuse), sizeof(reuse));

    if (!bind_localhost_port(server, callback_port) || listen(server, 1) != 0) {
        close_socket(server);
#ifdef _WIN32
        WSACleanup();
#endif
        return std::nullopt;
    }

    const socket_t client = accept(server, nullptr, nullptr);
    if (client == kInvalidSocket) {
        close_socket(server);
#ifdef _WIN32
        WSACleanup();
#endif
        return std::nullopt;
    }

    std::array<char, 4096> buffer {};
#ifdef _WIN32
    const int read_bytes = recv(client, buffer.data(), static_cast<int>(buffer.size() - 1), 0);
#else
    const int read_bytes = static_cast<int>(recv(client, buffer.data(), buffer.size() - 1, 0));
#endif

    std::optional<std::string> code;
    if (read_bytes > 0) {
        buffer[static_cast<std::size_t>(read_bytes)] = '\0';
        const std::string req(buffer.data());
        const auto line_end = req.find("\r\n");
        const auto first_line = req.substr(0, line_end);
        const auto first_space = first_line.find(' ');
        const auto second_space = first_line.find(' ', first_space + 1);
        if (first_space != std::string::npos && second_space != std::string::npos) {
            const auto target = first_line.substr(first_space + 1, second_space - first_space - 1);
            code = extract_code_from_callback_target(target);
        }
    }

    const char response[] =
        "HTTP/1.1 200 OK\r\n"
        "Content-Type: text/html\r\n\r\n"
        "<html><body><h1>Login Successful</h1><p>You can return to OpenNOW Rewrite.</p></body></html>";
#ifdef _WIN32
    send(client, response, static_cast<int>(sizeof(response) - 1), 0);
#else
    send(client, response, sizeof(response) - 1, 0);
#endif

    close_socket(client);
    close_socket(server);
#ifdef _WIN32
    WSACleanup();
#endif
    return code;
}

std::string LoginService::generate_nonce() const {
    const auto now = std::chrono::high_resolution_clock::now().time_since_epoch().count();
    const auto digest = sha256(std::to_string(now) + ":nonce");
    return base64url(digest.data(), digest.size());
}

std::string LoginService::device_id() const {
    const auto digest = sha256("opennow-rewrite-device");
    return base64url(digest.data(), digest.size());
}

} // namespace opennow::auth
