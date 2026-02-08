#include "opennow/core/app.hpp"

#include <sstream>

#include "opennow/auth/login.hpp"

namespace opennow::core {

std::string App::banner() const {
    return "OpenNOW Rewrite bootstrap is running.";
}

std::string App::login_bootstrap_preview() const {
    auth::LoginService login_service;
    const auto pkce = login_service.make_pkce_challenge();
    const auto port = login_service.pick_callback_port().value_or(2259);
    const auto url = login_service.build_auth_url(pkce, port, auth::LoginProvider::nvidia_default());

    std::ostringstream msg;
    msg << "Login base initialized (provider=NVIDIA, callback_port=" << port
        << ", auth_url_prefix=" << url.substr(0, 72) << "...)";
    return msg.str();
}

} // namespace opennow::core
