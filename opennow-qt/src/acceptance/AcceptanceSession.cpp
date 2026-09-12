#include "acceptance/AcceptanceSession.h"
#include "acceptance/AcceptanceProfiler.h"
#include "app/AppController.h"
#include "app/platform/GraphicsDeviceSelection.h"

#include <QElapsedTimer>
#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>

#include <cstdio>
#include <cstdlib>
#include <memory>
#include <utility>

using namespace Qt::StringLiterals;

AcceptanceSession::AcceptanceSession(QGuiApplication &application,
                                     QQmlApplicationEngine &engine,
                                     AppController &controller, CoreClient &coreClient,
                                     const QStringList &arguments)
    : m_application(application)
    , m_engine(engine)
    , m_controller(controller)
    , m_coreClient(coreClient)
    , m_arguments(arguments)
    , m_smokeTest(arguments.contains(u"--smoke-test"_s))
    , m_smokeConsolePersistenceRollback(m_smokeTest
          && arguments.contains(u"--smoke-console-persistence-rollback"_s))
    , m_smokeStreamerEvent(m_smokeTest && arguments.contains(u"--smoke-streamer-event"_s))
    , m_smokeFullscreenStatsShortcut(m_smokeTest
          && arguments.contains(u"--smoke-fullscreen-stats-shortcut"_s))
    , m_performanceReportIndex(arguments.indexOf(u"--performance-report"_s))
    , m_performanceMode(m_performanceReportIndex >= 0
          && m_performanceReportIndex + 1 < arguments.size())
{
    QObject::connect(&m_engine, &QQmlApplicationEngine::warnings, this,
                     [this](const QList<QQmlError> &warnings) {
                         m_qmlWarningOccurred = true;
                         for (const auto &warning : warnings) {
                             const auto message = warning.toString().toUtf8();
                             std::fprintf(stderr, "%s\n", message.constData());
                         }
                     });
}

QString AcceptanceSession::coreProgram(const QStringList &arguments)
{
    const auto performanceIndex = arguments.indexOf(u"--performance-report"_s);
    if (performanceIndex >= 0 && performanceIndex + 1 < arguments.size()) return {};
    const bool smoke = arguments.contains(u"--smoke-test"_s);
    const bool explicitAllowed = !smoke || arguments.contains(u"--smoke-console-persistence-rollback"_s)
        || arguments.contains(u"--smoke-streamer-event"_s);
    const auto coreIndex = arguments.indexOf(u"--core"_s);
    if (explicitAllowed && coreIndex >= 0 && coreIndex + 1 < arguments.size())
        return arguments.at(coreIndex + 1);
    if (smoke) return {};
    const auto bundled = QDir(QCoreApplication::applicationDirPath()).filePath(
#ifdef Q_OS_WIN
        u"opennow-core.exe"_s
#else
        u"opennow-core"_s
#endif
    );
    return QFileInfo::exists(bundled) ? bundled : QString{};
}

void AcceptanceSession::configureContext()
{
    m_engine.rootContext()->setContextProperty(u"SmokeTestMode"_s, m_smokeTest);
    const auto gpuCountIndex = m_arguments.indexOf(u"--smoke-gpu-count"_s);
    if (m_smokeTest && gpuCountIndex >= 0 && gpuCountIndex + 1 < m_arguments.size()) {
        const auto count = qBound(0, m_arguments.at(gpuCountIndex + 1).toInt(), 4);
        QList<GraphicsDeviceSelection::Adapter> adapters;
        for (int index = 0; index < count; ++index) {
            adapters.append({u"fixture-gpu-%1"_s.arg(index),
                index == 0 ? u"Intel UHD Graphics (fixture)"_s : u"NVIDIA GeForce RTX (fixture %1)"_s.arg(index),
                quint64(index + 1), index == 0 ? 0ULL : 8ULL << 30, false});
        }
        adapters.append({u"fixture-software"_s, u"Software adapter (fixture)"_s, 99, 0, true});
        auto *devices = new GraphicsDeviceSelection(adapters, {}, &m_engine);
        m_engine.rootContext()->setContextProperty(u"GraphicsDevices"_s, devices);
    }
    m_engine.rootContext()->setContextProperty(
        u"SmokeTestGame"_s,
        m_smokeTest ? QVariantMap{
            {u"title"_s, u"Smoke Test Game"_s},
            {u"isAvailable"_s, true},
            {u"isInLibrary"_s, true},
            {u"publisherName"_s, u"OpenNOW Test Fixture"_s},
            {u"genres"_s, QStringList{u"Fixture"_s}},
            {u"selectedVariantIndex"_s, 0},
            {u"variants"_s, QVariantList{
                QVariantMap{{u"id"_s, u"1001"_s}, {u"store"_s, u"Steam"_s}, {u"inLibrary"_s, true}},
                QVariantMap{{u"id"_s, u"1002"_s}, {u"store"_s, u"Epic Games Store"_s}, {u"inLibrary"_s, false}},
                QVariantMap{{u"id"_s, u"1003"_s}, {u"store"_s, u"Xbox"_s}, {u"inLibrary"_s, true}},
            }},
            {u"availableStores"_s, QStringList{u"Steam"_s, u"Epic Games Store"_s, u"Xbox"_s}},
        } : QVariantMap{});
}

int AcceptanceSession::measureStartup(const QElapsedTimer &startupTimer, qint64 qmlReadyMs)
{
    if (m_arguments.contains(u"--measure-startup"_s)) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr
            : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) {
            return EXIT_FAILURE;
        }
        const auto reported = std::make_shared<bool>(false);
        QObject::connect(window, &QQuickWindow::frameSwapped, this,
                         [this, startupTimer, qmlReadyMs, reported] {
                             if (*reported) return;
                             *reported = true;
                             std::fprintf(stdout,
                                          "{\"qmlReadyMs\":%lld,\"firstFrameMs\":%lld}\n",
                                          static_cast<long long>(qmlReadyMs),
                                          static_cast<long long>(startupTimer.elapsed()));
                             std::fflush(stdout);
                             m_application.exit(EXIT_SUCCESS);
                         });
        QTimer::singleShot(5'000, this, [this, reported] {
            if (*reported) return;
            *reported = true;
            std::fprintf(stderr, "startup measurement timed out before the first frame\n");
            std::fflush(stderr);
            m_application.exit(EXIT_FAILURE);
        });
    }
    return EXIT_SUCCESS;
}

int AcceptanceSession::startWorkload()
{
    if (m_performanceMode) {
        if (m_controller.reducedMotion()) {
            qCritical("Performance acceptance requires production motion to be enabled");
            return EXIT_FAILURE;
        }
        const auto integerOption = [this](const QString &name, int fallback) {
            const auto index = m_arguments.indexOf(name);
            if (index < 0 || index + 1 >= m_arguments.size()) return fallback;
            bool valid = false;
            const auto value = m_arguments.at(index + 1).toInt(&valid);
            return valid ? value : fallback;
        };
        const auto realOption = [this](const QString &name, double fallback) {
            const auto index = m_arguments.indexOf(name);
            if (index < 0 || index + 1 >= m_arguments.size()) return fallback;
            bool valid = false;
            const auto value = m_arguments.at(index + 1).toDouble(&valid);
            return valid ? value : fallback;
        };
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr
            : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        auto options = AcceptanceProfiler::Options{
            .reportPath = m_arguments.at(m_performanceReportIndex + 1),
            .width = integerOption(u"--performance-width"_s, 1920),
            .height = integerOption(u"--performance-height"_s, 1080),
            .cycles = integerOption(u"--performance-cycles"_s, 3),
            .refreshRateHz = realOption(u"--performance-refresh-hz"_s, 0.0),
            .machineLabel = [this] {
                const auto index = m_arguments.indexOf(u"--performance-label"_s);
                return index >= 0 && index + 1 < m_arguments.size()
                    ? m_arguments.at(index + 1) : QString{};
            }(),
            .requireHardware = m_arguments.contains(u"--performance-require-hardware"_s),
        };
        auto *profiler = new AcceptanceProfiler(
            window, &m_controller, std::move(options),
            [this](bool passed) {
                m_application.exit(passed && !m_qmlWarningOccurred ? EXIT_SUCCESS : EXIT_FAILURE);
            }, this);
        if (!profiler->start()) {
            qCritical("Could not start the Qt performance acceptance workload");
            return EXIT_FAILURE;
        }

        return EXIT_SUCCESS;
    }
    return startSmokeWorkload();
}
