#include <string>
#include <iostream>

#include "opennow/native/app.hpp"

namespace {

std::string ReadArgument(int argc, char** argv, const std::string& prefix, const std::string& fallback = "") {
  for (int index = 1; index < argc; ++index) {
    const std::string arg(argv[index]);
    if (arg.rfind(prefix, 0) == 0) {
      return arg.substr(prefix.size());
    }
  }
  return fallback;
}

}  // namespace

int main(int argc, char** argv) {
  const auto ipc_host = ReadArgument(argc, argv, "--ipc-host=", "127.0.0.1");
  const auto ipc_port_value = ReadArgument(argc, argv, "--ipc-port=", "0");
  const auto session_id = ReadArgument(argc, argv, "--session-id=", "unknown-session");
  const int ipc_port = std::stoi(ipc_port_value);

  opennow::native::Application app(ipc_host, ipc_port, session_id);
  std::string error;
  if (!app.Initialize(error)) {
    if (!error.empty()) {
      std::cerr << "[OpenNOW Native Streamer] " << error << std::endl;
    }
    return error.empty() ? 1 : 2;
  }
  return app.Run();
}
