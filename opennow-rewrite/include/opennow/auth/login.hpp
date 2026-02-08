#pragma once

#include <cstdint>
#include <optional>
#include <string>

namespace opennow::auth {

struct LoginProvider {
    std::string idp_id;
    std::string login_provider_code;
    std::string login_provider_display_name;
    std::string login_provider;
    std::string streaming_service_url;
    int32_t login_provider_priority = 0;

    [[nodiscard]] static LoginProvider nvidia_default();
    [[nodiscard]] bool is_alliance_partner() const;
};

struct AuthTokens {
    std::string access_token;
    std::optional<std::string> refresh_token;
    std::optional<std::string> id_token;
    int64_t expires_at = 0;
};

struct PkceChallenge {
    std::string verifier;
    std::string challenge;
};

class LoginService {
public:
    [[nodiscard]] PkceChallenge make_pkce_challenge() const;
    [[nodiscard]] std::string build_auth_url(
        const PkceChallenge& pkce,
        uint16_t callback_port,
        const LoginProvider& provider
    ) const;

    [[nodiscard]] std::optional<uint16_t> pick_callback_port() const;
    [[nodiscard]] std::optional<std::string> extract_code_from_callback_target(
        const std::string& callback_target
    ) const;

    [[nodiscard]] std::optional<std::string> wait_for_callback_code(uint16_t callback_port) const;

private:
    [[nodiscard]] std::string generate_nonce() const;
    [[nodiscard]] std::string device_id() const;
};

} // namespace opennow::auth
