#pragma once

#include "OPNStreamSession.h"
#include <memory>
#include <string>

namespace OPN {

enum class StreamWebRTCBackend {
    LibWebRTC,
};

StreamWebRTCBackend ResolveStreamWebRTCBackend();
std::string StreamWebRTCBackendName(StreamWebRTCBackend backend);
std::unique_ptr<IStreamSession> CreateStreamSession(StreamWebRTCBackend backend);

}
