#include "streaming/AwdlSuppressionLease.h"

#ifdef Q_OS_MACOS
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <cerrno>
#include <chrono>
#include <cstring>
#endif

AwdlSuppressionLease::AwdlSuppressionLease(QObject *parent)
    : QObject(parent)
{
}

AwdlSuppressionLease::~AwdlSuppressionLease()
{
    // Closing the connection releases the lease on the daemon side, so
    // destruction (including process exit) always restores the interface.
    release();
}

void AwdlSuppressionLease::setActive(bool active)
{
    if (m_active == active) return;
    m_active = active;
    if (active) acquire();
    else release();
}

#ifdef Q_OS_MACOS
namespace {
constexpr char kControlSocket[] = "/var/run/opennow-awdl/control.sock";
constexpr char kAcquire = 'A';
constexpr char kRelease = 'R';

bool sendFrame(int descriptor, char frame)
{
    return ::write(descriptor, &frame, 1) == 1;
}
} // namespace

void AwdlSuppressionLease::acquire()
{
    if (m_descriptor >= 0) return;
    const int descriptor = ::socket(AF_UNIX, SOCK_STREAM, 0);
    if (descriptor < 0) return;
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    const auto path = QByteArrayLiteral(kControlSocket);
    if (static_cast<size_t>(path.size()) >= sizeof(address.sun_path)) {
        ::close(descriptor);
        return;
    }
    std::memcpy(address.sun_path, path.constData(), static_cast<size_t>(path.size()) + 1);
    if (::connect(descriptor, reinterpret_cast<const sockaddr *>(&address), sizeof(address)) != 0) {
        // No helper daemon (not installed or not running): streaming is
        // unaffected, AWDL simply remains enabled.
        ::close(descriptor);
        return;
    }
    if (!sendFrame(descriptor, kAcquire)) {
        ::close(descriptor);
        return;
    }
    // The lease is held by the open connection; the reply is best-effort.
    char reply = 0;
    const auto readBytes = ::read(descriptor, &reply, 1);
    if (readBytes < 0 && errno != EAGAIN && errno != EWOULDBLOCK) {
        ::close(descriptor);
        return;
    }
    m_descriptor = descriptor;
}

void AwdlSuppressionLease::release()
{
    if (m_descriptor < 0) return;
    sendFrame(m_descriptor, kRelease);
    ::close(m_descriptor);
    m_descriptor = -1;
}
#else
void AwdlSuppressionLease::acquire() {}
void AwdlSuppressionLease::release() {}
#endif
