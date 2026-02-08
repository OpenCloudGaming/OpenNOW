#include <iostream>

#include "opennow/core/app.hpp"

int main() {
    opennow::core::App app;
    std::cout << app.banner() << '\n';
    std::cout << app.login_bootstrap_preview() << '\n';
    std::cout << "Next step: wire Qt6 shell + GStreamer pipeline." << '\n';
    return 0;
}
