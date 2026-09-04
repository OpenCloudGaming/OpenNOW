#include "AcceptanceProfiler.h"

#include "AppController.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSaveFile>
#include <QScreen>
#include <QSGRendererInterface>
#include <QSysInfo>
#include <QTimer>

#include <algorithm>
#include <cmath>
#include <limits>
#include <utility>

using namespace Qt::StringLiterals;

namespace {
constexpr int WarmupMs = 500;
constexpr int StepDurationMs = 500;
constexpr double IntervalBudgetMultiplier = 1.75;
constexpr double WorstIntervalBudgetMultiplier = 5.0;
constexpr double FirstFrameBudgetMultiplier = 6.0;
constexpr double MaximumMissedFrameRatio = 0.15;

QJsonValue finiteNumber(double value)
{
    return std::isfinite(value) ? QJsonValue(value) : QJsonValue();
}
}

AcceptanceProfiler::AcceptanceProfiler(QQuickWindow *window,
                                       AppController *controller,
                                       Options options,
                                       Completion completion,
                                       QObject *parent)
    : QObject(parent)
    , m_window(window)
    , m_controller(controller)
    , m_options(std::move(options))
    , m_completion(std::move(completion))
{
    const QVector<Action> cycle{
        {u"route-library"_s, u"route"_s, u"library"_s, false},
        {u"route-game-detail"_s, u"route"_s, u"game-detail"_s, false},
        {u"route-settings-dropdown"_s, u"route"_s, u"settings-video-dropdown"_s, false},
        {u"route-home"_s, u"route"_s, u"home"_s, false},
        {u"popup-quick-settings"_s, u"popup"_s, u"quick-settings"_s, true},
        {u"popup-guide"_s, u"popup"_s, u"guide-session"_s, true},
        {u"popup-guide-close"_s, u"popup"_s, QString{}, true},
        {u"popup-friends"_s, u"popup"_s, u"friends"_s, true},
        {u"popup-friends-close"_s, u"popup"_s, QString{}, true},
    };
    m_actions.reserve(cycle.size() * m_options.cycles);
    for (int cycleIndex = 0; cycleIndex < m_options.cycles; ++cycleIndex) {
        for (const auto &action : cycle) {
            auto named = action;
            named.name += u"-%1"_s.arg(cycleIndex + 1);
            m_actions.push_back(named);
        }
    }
}

bool AcceptanceProfiler::start()
{
    if (!m_window || !m_controller || m_options.reportPath.isEmpty()
            || m_options.width < 960 || m_options.width > 7680
            || m_options.height < 540 || m_options.height > 4320
            || m_options.cycles < 1 || m_options.cycles > 10) {
        return false;
    }
    const QFileInfo report(m_options.reportPath);
    if (!report.isAbsolute() || !QDir().mkpath(report.absolutePath())) {
        return false;
    }

    const auto dpr = std::max(1.0, m_window->effectiveDevicePixelRatio());
    m_window->setWidth(qRound(static_cast<double>(m_options.width) / dpr));
    m_window->setHeight(qRound(static_cast<double>(m_options.height) / dpr));
    m_window->show();
    m_window->requestActivate();
    m_timer.start();
    connect(m_window, &QQuickWindow::frameSwapped,
            this, [this] {
        const auto renderedNs = m_timer.nsecsElapsed();
        QMetaObject::invokeMethod(this, [this, renderedNs] {
            observeFrame(renderedNs);
        }, Qt::QueuedConnection);
    }, Qt::DirectConnection);
    QTimer::singleShot(WarmupMs, this, &AcceptanceProfiler::advance);
    QTimer::singleShot(WarmupMs + (m_actions.size() + 2) * StepDurationMs,
                       this, [this] {
        if (!m_finished) finish();
    });
    return true;
}

void AcceptanceProfiler::advance()
{
    if (m_finished) return;
    finalizeActiveStep();
    const auto nextIndex = m_results.size();
    if (nextIndex >= m_actions.size()) {
        finish();
        return;
    }

    StepResult result;
    result.action = m_actions.at(nextIndex);
    result.startedNs = m_timer.nsecsElapsed();
    result.accepted = result.action.overlay
        ? m_controller->showOverlay(result.action.value)
        : m_controller->navigate(result.action.value);
    m_results.push_back(std::move(result));
    m_activeStep = m_results.size() - 1;
    QTimer::singleShot(StepDurationMs, this, &AcceptanceProfiler::advance);
}

void AcceptanceProfiler::observeFrame(qint64 renderedNs)
{
    if (m_activeStep < 0 || m_activeStep >= m_results.size() || m_finished) return;
    auto &result = m_results[m_activeStep];
    if (result.firstFrameNs < 0) result.firstFrameNs = renderedNs;
    if (result.lastFrameNs >= result.startedNs) {
        result.frameIntervalsMs.push_back(
            static_cast<double>(renderedNs - result.lastFrameNs) / 1'000'000.0);
    }
    result.lastFrameNs = renderedNs;
}

void AcceptanceProfiler::finalizeActiveStep()
{
    if (m_activeStep < 0 || m_activeStep >= m_results.size()) return;
    m_results[m_activeStep].focusValid = focusIsValid();
    m_activeStep = -1;
}

void AcceptanceProfiler::finish()
{
    if (m_finished) return;
    finalizeActiveStep();
    m_finished = true;

    QStringList failures;
    const auto aggregates = aggregateMetrics(&failures);
    for (const auto &result : m_results) {
        if (!result.accepted) {
            failures.push_back(u"action was rejected: %1"_s.arg(result.action.name));
        }
        if (result.firstFrameNs < 0) {
            failures.push_back(u"no rendered frame observed: %1"_s.arg(result.action.name));
        }
        if (!result.focusValid) {
            failures.push_back(u"focus was invalid after transition: %1"_s.arg(result.action.name));
        }
    }
    if (m_results.size() != m_actions.size()) {
        failures.push_back(u"performance workload did not complete"_s);
    }
    const auto physicalSize = actualPhysicalSize();
    if (std::abs(physicalSize.width() - m_options.width) > 1
            || std::abs(physicalSize.height() - m_options.height) > 1) {
        failures.push_back(u"window manager did not provide the requested physical workload dimensions"_s);
    }
    const auto platform = QGuiApplication::platformName().toLower();
    const auto graphicsApi = graphicsApiName();
    if (m_options.requireHardware
            && (platform == u"offscreen"_s || platform == u"minimal"_s
                || graphicsApi == u"software"_s || graphicsApi == u"null"_s
                || graphicsApi == u"unknown"_s || !m_window->screen()
                || m_window->screen()->name().isEmpty())) {
        failures.push_back(u"representative acceptance requires a real screen and hardware renderer"_s);
    }
    if (m_options.requireHardware && m_options.refreshRateHz > 0.0) {
        failures.push_back(u"representative acceptance must use the display-reported refresh rate"_s);
    }
    const auto passed = failures.isEmpty();
    const auto reportWritten = writeReport(passed, failures, aggregates);
    if (m_completion) m_completion(passed && reportWritten);
}

QJsonArray AcceptanceProfiler::aggregateMetrics(QStringList *failures) const
{
    QJsonArray aggregates;
    const auto refreshRate = effectiveRefreshRate();
    const auto frameBudgetMs = 1000.0 / refreshRate;
    for (const auto &kind : {u"route"_s, u"popup"_s}) {
        QVector<double> intervals;
        QVector<double> firstFrames;
        for (const auto &result : m_results) {
            if (result.action.kind != kind) continue;
            intervals += result.frameIntervalsMs;
            if (result.firstFrameNs >= result.startedNs) {
                firstFrames.push_back(
                    static_cast<double>(result.firstFrameNs - result.startedNs) / 1'000'000.0);
            }
        }
        const auto p50 = percentile(intervals, 0.50);
        const auto p95 = percentile(intervals, 0.95);
        const auto worst = intervals.isEmpty()
            ? std::numeric_limits<double>::quiet_NaN()
            : *std::max_element(intervals.cbegin(), intervals.cend());
        const auto firstFrameP95 = percentile(firstFrames, 0.95);
        const auto missed = std::count_if(intervals.cbegin(), intervals.cend(),
                                          [frameBudgetMs](double interval) {
            return interval > frameBudgetMs * 1.5;
        });
        const auto missedRatio = intervals.isEmpty()
            ? 1.0 : static_cast<double>(missed) / intervals.size();

        QJsonObject metric{
            {u"kind"_s, kind},
            {u"frameCount"_s, intervals.size() + firstFrames.size()},
            {u"intervalSamples"_s, intervals.size()},
            {u"frameBudgetMs"_s, frameBudgetMs},
            {u"p50IntervalMs"_s, finiteNumber(p50)},
            {u"p95IntervalMs"_s, finiteNumber(p95)},
            {u"worstIntervalMs"_s, finiteNumber(worst)},
            {u"p95FirstFrameMs"_s, finiteNumber(firstFrameP95)},
            {u"missedFrameRatio"_s, missedRatio},
        };
        aggregates.push_back(metric);

        if (intervals.size() < 5) {
            failures->push_back(u"insufficient %1 animation frame samples"_s.arg(kind));
        } else {
            if (p95 > frameBudgetMs * IntervalBudgetMultiplier) {
                failures->push_back(u"%1 p95 frame interval exceeded budget"_s.arg(kind));
            }
            if (worst > frameBudgetMs * WorstIntervalBudgetMultiplier) {
                failures->push_back(u"%1 worst frame interval exceeded budget"_s.arg(kind));
            }
            if (firstFrameP95 > frameBudgetMs * FirstFrameBudgetMultiplier) {
                failures->push_back(u"%1 p95 first-frame latency exceeded budget"_s.arg(kind));
            }
            if (missedRatio > MaximumMissedFrameRatio) {
                failures->push_back(u"%1 missed-frame ratio exceeded budget"_s.arg(kind));
            }
        }
    }
    return aggregates;
}

bool AcceptanceProfiler::writeReport(bool passed,
                                     const QStringList &failures,
                                     const QJsonArray &aggregates) const
{
    QJsonArray steps;
    for (const auto &result : m_results) {
        const auto firstFrameMs = result.firstFrameNs >= result.startedNs
            ? static_cast<double>(result.firstFrameNs - result.startedNs) / 1'000'000.0
            : std::numeric_limits<double>::quiet_NaN();
        QJsonArray intervals;
        for (const auto interval : result.frameIntervalsMs) intervals.push_back(interval);
        steps.push_back(QJsonObject{
            {u"name"_s, result.action.name},
            {u"kind"_s, result.action.kind},
            {u"accepted"_s, result.accepted},
            {u"focusValid"_s, result.focusValid},
            {u"firstFrameMs"_s, finiteNumber(firstFrameMs)},
            {u"frameIntervalsMs"_s, intervals},
        });
    }
    QJsonArray failureValues;
    for (const auto &failure : failures) failureValues.push_back(failure);

    const auto *screen = m_window->screen();
    const auto physicalSize = actualPhysicalSize();
    const QJsonObject report{
        {u"schemaVersion"_s, 1},
        {u"kind"_s, u"opennow.qt.performance"_s},
        {u"generatedAt"_s, QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs)},
        {u"pass"_s, passed},
        {u"environment"_s, QJsonObject{
            {u"applicationVersion"_s, QCoreApplication::applicationVersion()},
            {u"machineLabel"_s, m_options.machineLabel.left(128)},
            {u"os"_s, QSysInfo::prettyProductName()},
            {u"kernel"_s, QSysInfo::kernelType() + u" "_s + QSysInfo::kernelVersion()},
            {u"cpuArchitecture"_s, QSysInfo::currentCpuArchitecture()},
            {u"qtPlatform"_s, QGuiApplication::platformName()},
            {u"graphicsApi"_s, graphicsApiName()},
            {u"screen"_s, screen ? screen->name() : QString{}},
            {u"devicePixelRatio"_s, m_window->effectiveDevicePixelRatio()},
            {u"refreshRateHz"_s, effectiveRefreshRate()},
            {u"reportedRefreshRateHz"_s, screen ? screen->refreshRate() : 0.0},
            {u"refreshRateOverrideHz"_s, finiteNumber(m_options.refreshRateHz)},
            {u"requestedPhysicalWidth"_s, m_options.width},
            {u"requestedPhysicalHeight"_s, m_options.height},
            {u"actualPhysicalWidth"_s, physicalSize.width()},
            {u"actualPhysicalHeight"_s, physicalSize.height()},
            {u"logicalWidth"_s, m_window->width()},
            {u"logicalHeight"_s, m_window->height()},
        }},
        {u"budget"_s, QJsonObject{
            {u"p95IntervalMultiplier"_s, IntervalBudgetMultiplier},
            {u"worstIntervalMultiplier"_s, WorstIntervalBudgetMultiplier},
            {u"firstFrameMultiplier"_s, FirstFrameBudgetMultiplier},
            {u"maximumMissedFrameRatio"_s, MaximumMissedFrameRatio},
        }},
        {u"workload"_s, QJsonObject{
            {u"cycles"_s, m_options.cycles},
            {u"stepDurationMs"_s, StepDurationMs},
            {u"transitionCount"_s, m_actions.size()},
        }},
        {u"aggregates"_s, aggregates},
        {u"steps"_s, steps},
        {u"failures"_s, failureValues},
    };

    QSaveFile file(m_options.reportPath);
    if (!file.open(QIODevice::WriteOnly)) return false;
    const auto bytes = QJsonDocument(report).toJson(QJsonDocument::Indented);
    if (file.write(bytes) != bytes.size()) {
        file.cancelWriting();
        return false;
    }
    return file.commit();
}

bool AcceptanceProfiler::focusIsValid() const
{
    const auto *focused = m_window ? m_window->activeFocusItem() : nullptr;
    return focused && focused->isVisible() && focused->isEnabled();
}

double AcceptanceProfiler::effectiveRefreshRate() const
{
    if (m_options.refreshRateHz >= 24.0 && m_options.refreshRateHz <= 500.0) {
        return m_options.refreshRateHz;
    }
    const auto *screen = m_window ? m_window->screen() : nullptr;
    const auto reported = screen ? screen->refreshRate() : 0.0;
    return reported >= 24.0 && reported <= 500.0 ? reported : 60.0;
}

QSize AcceptanceProfiler::actualPhysicalSize() const
{
    if (!m_window) return {};
    const auto dpr = std::max(1.0, m_window->effectiveDevicePixelRatio());
    return {
        qRound(static_cast<double>(m_window->width()) * dpr),
        qRound(static_cast<double>(m_window->height()) * dpr),
    };
}

QString AcceptanceProfiler::graphicsApiName() const
{
    const auto *interface = m_window ? m_window->rendererInterface() : nullptr;
    if (!interface) return u"unknown"_s;
    switch (interface->graphicsApi()) {
    case QSGRendererInterface::Software: return u"software"_s;
    case QSGRendererInterface::OpenVG: return u"openvg"_s;
    case QSGRendererInterface::OpenGL: return u"opengl"_s;
    case QSGRendererInterface::Direct3D11: return u"direct3d11"_s;
    case QSGRendererInterface::Direct3D12: return u"direct3d12"_s;
    case QSGRendererInterface::Vulkan: return u"vulkan"_s;
    case QSGRendererInterface::Metal: return u"metal"_s;
    case QSGRendererInterface::Null: return u"null"_s;
    default: return u"unknown"_s;
    }
}

double AcceptanceProfiler::percentile(QVector<double> values, double quantile)
{
    if (values.isEmpty()) return std::numeric_limits<double>::quiet_NaN();
    std::sort(values.begin(), values.end());
    const auto index = std::clamp(
        static_cast<qsizetype>(std::ceil(quantile * values.size())) - 1,
        qsizetype{0}, values.size() - 1);
    return values.at(index);
}
