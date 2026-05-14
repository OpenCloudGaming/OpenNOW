#include "OPNStreamBackend.h"
#include "OPNLibWebRTCStreamSession.h"

namespace OPN {

StreamWebRTCBackend ResolveStreamWebRTCBackend() {
    return StreamWebRTCBackend::LibWebRTC;
}

std::string StreamWebRTCBackendName(StreamWebRTCBackend backend) {
    switch (backend) {
        case StreamWebRTCBackend::LibWebRTC: return "libwebrtc";
    }
    return "libwebrtc";
}

std::unique_ptr<IStreamSession> CreateStreamSession(StreamWebRTCBackend backend) {
    (void)backend;
    if (LibWebRTCStreamSession::IsAvailable()) {
        return std::make_unique<LibWebRTCStreamSession>();
    }
    return nullptr;
}

}
