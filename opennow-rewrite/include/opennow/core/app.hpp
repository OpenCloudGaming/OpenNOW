#pragma once

#include <string>

namespace opennow::core {

class App {
public:
    [[nodiscard]] std::string banner() const;
    [[nodiscard]] std::string login_bootstrap_preview() const;
};

} // namespace opennow::core
