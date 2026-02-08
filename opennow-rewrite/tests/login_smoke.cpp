#include <iostream>

#include "opennow/auth/login.hpp"

int main() {
    opennow::auth::LoginService service;
    const auto pkce = service.make_pkce_challenge();
    if (pkce.verifier.size() < 43 || pkce.challenge.size() < 43) {
        std::cerr << "pkce generation failed\n";
        return 1;
    }

    if (pkce.challenge.find('=') != std::string::npos) {
        std::cerr << "pkce challenge must be base64url without padding\n";
        return 1;
    }

    const auto provider = opennow::auth::LoginProvider::nvidia_default();
    const auto url = service.build_auth_url(pkce, 2259, provider);
    if (url.find("https://login.nvidia.com/authorize") != 0) {
        std::cerr << "unexpected auth url: " << url << "\n";
        return 1;
    }
    if (url.find("code_challenge_method=S256") == std::string::npos) {
        std::cerr << "missing S256 marker\n";
        return 1;
    }

    const auto code = service.extract_code_from_callback_target("/?code=abc123%2Ffoo%2Bbar&state=foo");
    if (!code || *code != "abc123/foo+bar") {
        std::cerr << "code extraction failed\n";
        return 1;
    }

    const auto port = service.pick_callback_port();
    if (!port) {
        std::cerr << "callback port selection failed\n";
        return 1;
    }

    std::cout << "login_smoke ok\n";
    return 0;
}
