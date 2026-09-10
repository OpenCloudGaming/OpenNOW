#include "app/platform/MacAwdlController.h"

#include <algorithm>

#ifdef Q_OS_MACOS
#include <cerrno>
#include <cstring>
#include <ifaddrs.h>
#include <net/if.h>
#endif

namespace {
constexpr qsizetype OutputLimit = 4096;

class SystemMacAwdlBackend final : public MacAwdlBackend
{
public:
    SystemMacAwdlBackend()
    {
        m_process.setStandardOutputFile(QProcess::nullDevice());
        connect(&m_process, &QProcess::readyReadStandardError, this, [this] {
            m_process.setReadChannel(QProcess::StandardError);
            while (m_process.bytesAvailable() > 0)
                emit standardErrorReady(m_process.read(OutputLimit));
        });
        connect(&m_process, &QProcess::errorOccurred, this, &MacAwdlBackend::processError);
        connect(&m_process, &QProcess::finished, this, &MacAwdlBackend::finished);
    }

    MacAwdlController::State readState(QString *error) override
    {
        error->clear();
#ifdef Q_OS_MACOS
        ifaddrs *addresses = nullptr;
        if (getifaddrs(&addresses) != 0) {
            *error = tr("Could not read the awdl0 interface: %1")
                         .arg(QString::fromLocal8Bit(std::strerror(errno)));
            return MacAwdlController::Unknown;
        }
        auto state = MacAwdlController::Unavailable;
        for (const ifaddrs *entry = addresses; entry; entry = entry->ifa_next) {
            if (entry->ifa_name && std::strcmp(entry->ifa_name, "awdl0") == 0) {
                state = (entry->ifa_flags & IFF_UP) ? MacAwdlController::Enabled
                                                  : MacAwdlController::Disabled;
                break;
            }
        }
        freeifaddrs(addresses);
        return state;
#else
        return MacAwdlController::Unsupported;
#endif
    }

    void start(const QString &program, const QStringList &arguments) override
    {
#ifdef Q_OS_MACOS
        m_process.start(program, arguments);
#else
        Q_UNUSED(program);
        Q_UNUSED(arguments);
        emit processError(QProcess::FailedToStart);
#endif
    }

    void kill() override { m_process.kill(); }

private:
    QProcess m_process;
};
}

MacAwdlController::MacAwdlController(QObject *parent)
    : MacAwdlController(std::make_unique<SystemMacAwdlBackend>(), 60'000, parent)
{
}

MacAwdlController::MacAwdlController(std::unique_ptr<MacAwdlBackend> backend,
                                   int timeoutMs, QObject *parent)
    : QObject(parent), m_backend(std::move(backend))
{
    m_timeout.setSingleShot(true);
    m_timeout.setInterval(std::max(1, timeoutMs));
    connect(&m_timeout, &QTimer::timeout, this, [this] {
        if (!m_busy)
            return;
        m_timedOut = true;
        setActionError(tr("The administrator request timed out. The current interface state will be checked when the helper exits."));
        m_backend->kill();
    });
    connect(m_backend.get(), &MacAwdlBackend::standardErrorReady, this,
            [this](const QByteArray &output) {
        if (!m_busy)
            return;
        if (output.size() >= OutputLimit)
            m_output = output.right(OutputLimit);
        else {
            m_output.append(output);
            if (m_output.size() > OutputLimit)
                m_output.remove(0, m_output.size() - OutputLimit);
        }
    });
    connect(m_backend.get(), &MacAwdlBackend::processError, this,
            [this](QProcess::ProcessError error) {
        if (!m_busy)
            return;
        if (error == QProcess::FailedToStart) {
            m_processError = tr("Could not start /usr/bin/osascript for the administrator request.");
            complete(-1, QProcess::NormalExit);
        } else if (error != QProcess::Crashed) {
            m_processError = tr("The administrator helper encountered a process error (%1).")
                                 .arg(static_cast<int>(error));
            m_backend->kill();
        }
    });
    connect(m_backend.get(), &MacAwdlBackend::finished, this, &MacAwdlController::complete);
    refresh();
}

MacAwdlController::~MacAwdlController()
{
    m_timeout.stop();
    disconnect(m_backend.get(), nullptr, this, nullptr);
    m_backend.reset();
}

MacAwdlController::State MacAwdlController::state() const { return m_state; }
bool MacAwdlController::busy() const { return m_busy; }
QString MacAwdlController::error() const { return m_error; }

void MacAwdlController::readStatus()
{
    QString error;
    const auto state = m_backend->readState(&error);
    if (state != m_state) {
        m_state = state;
        emit statusChanged();
    }
    m_statusError = state == Unknown && error.isEmpty()
        ? tr("The awdl0 interface state could not be read.") : error;
    updateError();
}

void MacAwdlController::setActionError(const QString &error)
{
    m_actionError = error;
    updateError();
}

void MacAwdlController::updateError()
{
    QString error = m_actionError;
    if (!m_statusError.isEmpty()) {
        if (!error.isEmpty())
            error += QLatin1Char(' ');
        error += m_statusError;
    }
    if (m_error == error)
        return;
    m_error = error;
    emit errorChanged();
}

void MacAwdlController::refresh()
{
    if (m_busy)
        return;
    readStatus();
}

void MacAwdlController::disable() { request(Disabled); }
void MacAwdlController::enable() { request(Enabled); }

void MacAwdlController::request(State target)
{
    if (m_busy)
        return;
    readStatus();
    if (m_state == Unsupported) {
        setActionError(tr("AWDL interface control is supported only on macOS."));
        return;
    }
    if (m_state == Unavailable) {
        setActionError(tr("The awdl0 interface is not available."));
        return;
    }
    if (m_state == Unknown) {
        setActionError({});
        return;
    }
    setActionError({});
    if (m_state == target)
        return;

    m_target = target;
    m_output.clear();
    m_processError.clear();
    m_timedOut = false;
    m_busy = true;
    emit busyChanged();
    m_timeout.start();
    m_backend->start(QStringLiteral("/usr/bin/osascript"),
                     {QStringLiteral("-e"), target == Disabled
                          ? QStringLiteral("do shell script \"/sbin/ifconfig awdl0 down\" with administrator privileges")
                          : QStringLiteral("do shell script \"/sbin/ifconfig awdl0 up\" with administrator privileges")});
}

void MacAwdlController::complete(int exitCode, QProcess::ExitStatus exitStatus)
{
    if (!m_busy)
        return;
    m_timeout.stop();
    readStatus();
    QString error;
    if (m_timedOut)
        error = tr("The administrator request timed out. The interface may still have changed; its current state is shown.");
    else if (!m_processError.isEmpty())
        error = m_processError;
    else if (exitStatus == QProcess::CrashExit)
        error = tr("The administrator helper crashed. Its changes could not be confirmed.");
    else if (exitCode != 0) {
        if (m_output.contains("(-128)"))
            error = tr("Administrator authorization was canceled (-128).");
        else
            error = tr("The administrator request failed (exit code %1).").arg(exitCode);
    } else if (m_state != m_target) {
        error = m_target == Disabled
            ? tr("The awdl0 interface was not confirmed down. macOS may have re-enabled it; no retry was attempted.")
            : tr("The awdl0 interface was not confirmed up; no retry was attempted.");
    }
    setActionError(error);
    m_busy = false;
    emit busyChanged();
}
