#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"
#include "core/CoreClient.h"
#include "acceptance/MotionAcceptance.h"

#include <QGuiApplication>
#include <QKeyEvent>
#include <QDeadlineTimer>
#include <QQmlComponent>
#include <QFileInfo>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QTemporaryFile>
#include <QVariantMap>

#include <cstdio>
#include <cstdlib>
#include <memory>

using namespace Qt::StringLiterals;

int AcceptanceSession::startSmokeWorkload()
{
    const auto screenshotIndex = m_arguments.indexOf(u"--screenshot"_s);
    const auto resumeIndex = m_arguments.indexOf(u"--smoke-session-resume"_s);
    if (m_smokeTest && resumeIndex >= 0 && resumeIndex + 1 < m_arguments.size()) {
        const auto mode = m_arguments.at(resumeIndex + 1);
        QTimer::singleShot(350, this, [this, mode] {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            auto *window = m_engine.rootObjects().isEmpty() ? nullptr
                : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            auto *host = window ? window->findChild<QQuickItem *>(u"fallbackOverlayHost"_s) : nullptr;
            auto *dialog = window ? window->findChild<QQuickItem *>(u"sessionConflictDialog"_s) : nullptr;
            const auto expectedState = mode == u"unavailable"_s ? u"error"_s : mode;
            if (!store || store->property("streamState").toString() != expectedState
                    || m_controller.route() != u"inserting"_s
                    || (mode == u"conflict"_s && m_controller.overlay() != u"session-conflict"_s)) {
                qCritical("Session resume fixture did not reach its expected screen");
                m_application.exit(EXIT_FAILURE);
            }
            if (mode == u"conflict"_s && (!host || !host->isVisible() || host->opacity() <= 0
                    || !dialog || !dialog->isVisible() || dialog->width() <= 0 || dialog->height() <= 0
                    || (m_arguments.contains(u"--desktop"_s) && host->scale() != 1))) {
                qCritical("Session conflict dialog is hidden or incorrectly scaled");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (mode == u"conflict"_s && !m_arguments.contains(u"--screenshot"_s)) {
                QKeyEvent press(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &press);
                QKeyEvent release(QEvent::KeyRelease, Qt::Key_Escape, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &release);
                if (!m_controller.overlay().isEmpty() || m_controller.route() == u"inserting"_s
                        || !store->property("pendingLaunchParams").isNull()
                        || store->property("remoteSessions").toList().size() != 1) {
                    qCritical("Cancelling the session conflict did not preserve the running game");
                    m_application.exit(EXIT_FAILURE);
                }
            }
        });
    }
    if (m_smokeTest && m_arguments.contains(u"--smoke-gpu-count"_s))
        return startGpuSettingsWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-theme-settings"_s))
        return startThemeSettingsWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-session-launch"_s))
        return startSessionLaunchWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-session-fullscreen"_s))
        return startSessionFullscreenWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-stream-exit"_s))
        return startStreamExitWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-frame-generation-stats"_s))
        return startFrameGenerationStatsWorkload();
    if (m_smokeTest && (m_arguments.contains(u"--smoke-frame-generation"_s)
                       || m_arguments.contains(u"--smoke-ten-bit-warning"_s)
                       || m_arguments.contains(u"--smoke-onboarding"_s)
                       || m_arguments.contains(u"--smoke-upscaling"_s)
                       || m_arguments.contains(u"--smoke-stream-stats"_s)
                       || m_arguments.contains(u"--smoke-stream-stats-v2"_s)
                       || m_arguments.contains(u"--smoke-controller-metadata"_s)
                       || m_arguments.contains(u"--smoke-custom-background"_s))) {
        const bool controllerMetadata = m_arguments.contains(u"--smoke-controller-metadata"_s);
        const bool onboarding = m_arguments.contains(u"--smoke-onboarding"_s);
        const bool tenBitWarning = m_arguments.contains(u"--smoke-ten-bit-warning"_s);
        const bool customBackground = m_arguments.contains(u"--smoke-custom-background"_s);
        const bool streamStats = m_arguments.contains(u"--smoke-stream-stats"_s);
        QQmlComponent component(&m_engine, QUrl(m_arguments.contains(u"--smoke-onboarding"_s)
            ? m_arguments.contains(u"--onboarding-replay-check"_s)
                ? u"qrc:/acceptance/OnboardingReplayAcceptance.qml"_s
                : m_arguments.contains(u"--onboarding-ui-check"_s)
                ? u"qrc:/acceptance/OnboardingUiAcceptance.qml"_s
                : m_arguments.contains(u"--onboarding-awdl-check"_s)
                ? u"qrc:/acceptance/OnboardingAwdlAcceptance.qml"_s
                : m_arguments.contains(u"--onboarding-scroll-check"_s)
                ? u"qrc:/acceptance/OnboardingScrollAcceptance.qml"_s
                : u"qrc:/acceptance/OnboardingAcceptance.qml"_s
            : controllerMetadata
            ? u"qrc:/acceptance/ControllerMetadataAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-ten-bit-warning"_s)
            ? u"qrc:/acceptance/TenBitWarningAcceptance.qml"_s
            : customBackground
            ? u"qrc:/acceptance/CustomBackgroundAcceptance.qml"_s
            : streamStats
            ? u"qrc:/acceptance/StreamStatsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-stream-stats-v2"_s)
            ? u"qrc:/acceptance/StreamStatsV2Acceptance.qml"_s
            : m_arguments.contains(u"--smoke-upscaling"_s)
            ? u"qrc:/acceptance/UpscalingAcceptance.qml"_s
            : u"qrc:/acceptance/FrameGenerationAcceptance.qml"_s));
        auto *fixture = component.create();
        if (!fixture) { qCritical() << component.errors(); return EXIT_FAILURE; }
        fixture->setParent(&m_engine);
        if (controllerMetadata)
            m_engine.rootContext()->setContextProperty(u"ControllerInput"_s, fixture->property("input").value<QObject *>());
        if (customBackground) {
            QFile image(u":/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s);
            auto *localImage = new QTemporaryFile(&m_engine);
            if (!image.open(QIODevice::ReadOnly) || !localImage->open()) return EXIT_FAILURE;
            const auto data = image.readAll();
            if (localImage->write(data) != data.size() || !localImage->flush()) return EXIT_FAILURE;
            fixture->setProperty("imageUrl", QUrl::fromLocalFile(localImage->fileName()).toString());
            localImage->close();
        }
        QTimer::singleShot(150, this, [this, fixture, customBackground, onboarding, tenBitWarning] {
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            QVariant passed;
            const bool ok = window && QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
                Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && passed.toBool() && !m_qmlWarningOccurred;
            if (!ok) { m_application.exit(EXIT_FAILURE); return; }
            const auto finish = [this, window, fixture, customBackground, onboarding, tenBitWarning] {
                if (customBackground || onboarding || tenBitWarning) {
                    QVariant verified;
                    if (!QMetaObject::invokeMethod(fixture, "verify", Q_RETURN_ARG(QVariant, verified))
                        || !verified.toBool() || m_qmlWarningOccurred) {
                        m_application.exit(EXIT_FAILURE);
                        return;
                    }
                }
                QTimer::singleShot(150, this, [this, window] {
                    const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                    const bool saved = shot < 0 || (shot + 1 < m_arguments.size()
                        && window->grabWindow().save(m_arguments.at(shot + 1)));
                    m_application.exit(saved && !m_qmlWarningOccurred ? EXIT_SUCCESS : EXIT_FAILURE);
                });
            };
            if (customBackground) {
                auto *readiness = new QTimer(this);
                readiness->setInterval(25);
                connect(readiness, &QTimer::timeout, this,
                        [this, fixture, readiness, finish, deadline = QDeadlineTimer(5'000)] {
                    QVariant ready;
                    const bool ok = QMetaObject::invokeMethod(fixture, "ready", Q_RETURN_ARG(QVariant, ready))
                        && !m_qmlWarningOccurred;
                    if (!ok || (!ready.toBool() && deadline.hasExpired())) {
                        readiness->stop();
                        readiness->deleteLater();
                        qCritical("Custom background image failed to load before the acceptance deadline");
                        m_application.exit(EXIT_FAILURE);
                        return;
                    }
                    if (ready.toBool()) {
                        readiness->stop();
                        readiness->deleteLater();
                        finish();
                    }
                });
                readiness->start();
            } else {
                QTimer::singleShot(150, this, finish);
            }
        });
    } else if (m_smokeTest && m_arguments.contains(u"--smoke-input-capture-error"_s)) {
        auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
        if (!store) return EXIT_FAILURE;
        store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s}});
        store->setProperty("streamState", u"streaming"_s);
        QTimer::singleShot(150, this, [this] {
            auto *window = m_engine.rootObjects().isEmpty() ? nullptr
                : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            auto *notice = window ? window->findChild<QQuickItem *>(u"streamInputCaptureNotice"_s) : nullptr;
            auto *surface = window ? window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) : nullptr;
            if (!notice || !surface || !surface->isVisible()) {
                qCritical("Input capture fixture could not find the visible stream surface and notice");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            notice->setProperty("message", u"Relative mouse input requires Wayland relative-pointer-v1 and pointer-constraints-v1; the compositor does not provide both protocols."_s);
            QTimer::singleShot(250, this, [this, window, notice, surface] {
                bool passed = !m_qmlWarningOccurred && notice->isVisible() && surface->isVisible()
                    && window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) == surface;
                const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                if (shot >= 0 && shot + 1 < m_arguments.size())
                    passed = passed && QFileInfo(m_arguments.at(shot + 1)).isAbsolute()
                        && window->grabWindow().save(m_arguments.at(shot + 1));
                if (!passed) qCritical("Input capture fixture did not preserve the visible stream surface");
                m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
            });
        });
    } else if (m_smokeTest && (m_arguments.contains(u"--smoke-backend-availability"_s)
                     || m_arguments.contains(u"--smoke-microphone"_s)
                     || m_arguments.contains(u"--smoke-audio-output"_s)
                     || m_arguments.contains(u"--smoke-background-stream"_s)
                     || m_arguments.contains(u"--smoke-recording"_s)
                     || m_arguments.contains(u"--smoke-collections"_s)
                     || m_arguments.contains(u"--smoke-steam-big-picture"_s)
                     || m_arguments.contains(u"--smoke-persistent-in-game-settings"_s)
                     || m_arguments.contains(u"--smoke-idle-mode"_s)
                     || m_arguments.contains(u"--smoke-queue-drops"_s)
                     || m_arguments.contains(u"--smoke-stream-recovery"_s))) {
        QQmlComponent component(&m_engine, QUrl(m_arguments.contains(u"--smoke-queue-drops"_s)
            ? u"qrc:/acceptance/QueueDropsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-microphone"_s)
            ? u"qrc:/acceptance/MicrophoneAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-audio-output"_s)
            ? u"qrc:/acceptance/AudioOutputAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-background-stream"_s)
            ? u"qrc:/acceptance/BackgroundStreamAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-recording"_s)
            ? u"qrc:/acceptance/RecordingAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-collections"_s)
            ? u"qrc:/acceptance/CollectionsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-steam-big-picture"_s)
            ? u"qrc:/acceptance/SteamBigPictureAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-persistent-in-game-settings"_s)
            ? u"qrc:/acceptance/PersistentInGameSettingsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-idle-mode"_s)
            ? u"qrc:/acceptance/IdleModeAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-stream-recovery"_s)
            ? u"qrc:/acceptance/StreamRecoveryAcceptance.qml"_s
            : u"qrc:/acceptance/BackendAvailabilityAcceptance.qml"_s));
        auto *fixture = component.create();
        if (!fixture) { qCritical() << component.errors(); return EXIT_FAILURE; }
        fixture->setParent(&m_engine);
        if (m_arguments.contains(u"--smoke-microphone"_s)
            || m_arguments.contains(u"--smoke-recording"_s)
            || m_arguments.contains(u"--smoke-queue-drops"_s)) {
            auto *runtime = fixture->property("runtime").value<QObject *>();
            if (!runtime) return EXIT_FAILURE;
            m_engine.rootContext()->setContextProperty(u"NativeStreamRuntime"_s, runtime);
        }
        if (m_arguments.contains(u"--smoke-stream-recovery"_s)
            || m_arguments.contains(u"--smoke-recording"_s)
            || m_arguments.contains(u"--smoke-queue-drops"_s)
            || m_arguments.contains(u"--smoke-collections"_s)
            || m_arguments.contains(u"--smoke-steam-big-picture"_s)
            || m_arguments.contains(u"--smoke-persistent-in-game-settings"_s)) {
            auto *client = fixture->property("client").value<QObject *>();
            if (!client) return EXIT_FAILURE;
            m_engine.rootContext()->setContextProperty(u"CoreClient"_s, client);
        }
        QTimer::singleShot(150, this, [this, fixture] {
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            QVariant passed;
            const bool ok = window && QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
                Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && passed.toBool() && !m_qmlWarningOccurred;
            if (ok && (m_arguments.contains(u"--smoke-collections"_s)
                       || m_arguments.contains(u"--smoke-queue-drops"_s))) {
                QTimer::singleShot(250, this, [this, window] {
                    const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                    const bool saved = shot < 0 || (shot + 1 < m_arguments.size()
                        && window->grabWindow().save(m_arguments.at(shot + 1)));
                    m_application.exit(saved && !m_qmlWarningOccurred ? EXIT_SUCCESS : EXIT_FAILURE);
                });
            } else {
                m_application.exit(ok ? EXIT_SUCCESS : EXIT_FAILURE);
            }
        });
    } else if (m_smokeTest && (m_arguments.contains(u"--smoke-region-ping"_s) || m_arguments.contains(u"--smoke-store-paging"_s))) {
        const bool storePaging = m_arguments.contains(u"--smoke-store-paging"_s);
        QQmlComponent component(&m_engine, QUrl(storePaging ? u"qrc:/acceptance/StorePagingAcceptance.qml"_s
            : u"qrc:/acceptance/RegionPingAcceptance.qml"_s));
        auto *fixture = component.create();
        if (!fixture) { qCritical() << component.errors(); return EXIT_FAILURE; }
        fixture->setParent(&m_engine);
        auto *client = fixture->property("client").value<QObject *>();
        if (!client) return EXIT_FAILURE;
        m_engine.rootContext()->setContextProperty(u"CoreClient"_s, client);
        QTimer::singleShot(150, this, [this, fixture, storePaging] {
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            auto *screen = window->findChild<QObject *>(storePaging ? u"desktopStoreContent"_s : u"desktopSettingsScreen"_s);
            auto *row = window->findChild<QObject *>(storePaging ? u"storePageStatusText"_s : u"renewRegionLatency"_s);
            auto *button = window->findChild<QObject *>(storePaging ? u"storePageRetry"_s : u"renewRegionPingButton"_s);
            auto *picker = window->findChild<QObject *>(storePaging ? u"storeEmptyStatusText"_s : u"renewNetworkRegion"_s);
            QVariant passed;
            if (!screen || !row || !button || !picker
                || !QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
                    Q_ARG(QVariant, QVariant::fromValue(screen)), Q_ARG(QVariant, QVariant::fromValue(row)),
                    Q_ARG(QVariant, QVariant::fromValue(button)), Q_ARG(QVariant, QVariant::fromValue(picker)))
                || !passed.toBool() || m_qmlWarningOccurred) {
                qCritical("Store/region acceptance failed");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (auto *results = window->findChild<QObject *>(u"renewNetworkRegion"_s))
                results->setProperty("expanded", true);
            if (storePaging && m_arguments.contains(u"--smoke-store-navigation"_s)) {
                startStoreNavigationAcceptance(window, &m_qmlWarningOccurred);
                return;
            }
            if (storePaging && m_arguments.contains(u"--smoke-store-appearance"_s)) {
                QVariant prepared;
                if (!QMetaObject::invokeMethod(fixture, "prepareAppearance", Q_RETURN_ARG(QVariant, prepared),
                        Q_ARG(QVariant, QVariant::fromValue(screen)),
                        Q_ARG(QVariant, QVariant(m_arguments.contains(u"--smoke-store-genres"_s)))) || !prepared.toBool()) {
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
            }
            QTimer::singleShot(250, this, [this, window] {
                bool passed = !m_qmlWarningOccurred;
                const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                if (shot >= 0 && shot + 1 < m_arguments.size()) {
                    const auto path = m_arguments.at(shot + 1);
                    passed = passed && QFileInfo(path).isAbsolute() && window->grabWindow().save(path);
                }
                m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
            });
        });
    } else if (m_smokeTest && m_arguments.contains(u"--smoke-settings-motion"_s)) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        startSettingsMotionAcceptance(window, &m_controller, &m_qmlWarningOccurred,
                                      m_arguments.contains(u"--smoke-motion-fullscreen"_s));
    } else if (m_smokeTest && m_arguments.contains(u"--smoke-sidebar"_s)) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        startSidebarAcceptance(window, &m_controller, &m_qmlWarningOccurred,
                               m_arguments.contains(u"--smoke-motion-fullscreen"_s));
    } else if (m_smokeTest && m_arguments.contains(u"--smoke-motion"_s)) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        startMotionAcceptance(window, &m_controller, &m_qmlWarningOccurred,
                              m_arguments.contains(u"--smoke-motion-fullscreen"_s));
    } else if (m_smokeFullscreenStatsShortcut) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) return EXIT_FAILURE;
        window->showFullScreen();
        window->requestActivate();
        auto *statsShortcut = window->findChild<QObject *>(
            m_arguments.contains(u"--smoke-configured-stats-shortcut"_s)
                ? u"configuredStreamStatsShortcut"_s : u"streamStatsShortcut"_s);
        auto *copyShortcut = window->findChild<QObject *>(u"streamStatsCopyShortcut"_s);
        auto *streamSurface = window->findChild<QObject *>(u"streamSurfaceHost"_s);
        if (!statsShortcut || !copyShortcut || !streamSurface) return EXIT_FAILURE;
        const auto activateStatsShortcut = [statsShortcut] {
            return QMetaObject::invokeMethod(statsShortcut, "activated", Qt::DirectConnection);
        };
        const auto activateCopyShortcut = [copyShortcut] {
            return QMetaObject::invokeMethod(copyShortcut, "activated", Qt::DirectConnection);
        };
        const auto activateGuideShortcut = [streamSurface] {
            return QMetaObject::invokeMethod(
                streamSurface, "localShortcutRequested", Qt::DirectConnection,
                Q_ARG(QString, QStringLiteral("guide")));
        };
        const auto windowStayedFullscreen = [window] {
            return window->visibility() == QWindow::FullScreen
                && window->property("desktopSurfaceActive").toBool();
        };
        const auto streamInputStayedStable = [window, windowStayedFullscreen] {
            return windowStayedFullscreen()
                && !window->property("shellCaptureEnabledForSmokeTest").toBool();
        };
        QTimer::singleShot(150, this,
                           [this, statsShortcut, copyShortcut, activateStatsShortcut,
                            activateCopyShortcut, activateGuideShortcut,
                            windowStayedFullscreen, streamInputStayedStable] {
            if (!streamInputStayedStable() || m_qmlWarningOccurred
                || statsShortcut->property("context").toInt() != Qt::ApplicationShortcut
                || !statsShortcut->property("enabled").toBool()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (!activateStatsShortcut()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (m_controller.overlay() != u"desktop-stream-stats"_s
                    || !streamInputStayedStable()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            activateStatsShortcut();
            if (m_controller.overlay() != u"desktop-stream-stats-expanded"_s
                    || !streamInputStayedStable()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (!copyShortcut->property("enabled").toBool()
                    || !activateCopyShortcut()
                    || m_controller.overlay() != u"desktop-stream-stats-expanded"_s
                    || !m_controller.readClipboardText().startsWith(u"Stream stats:"_s)
                    || !streamInputStayedStable()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            activateStatsShortcut();
            if (!m_controller.overlay().isEmpty() || !streamInputStayedStable()
                    || !activateGuideShortcut()
                    || m_controller.overlay() != u"desktop-stream-menu"_s
                    || !windowStayedFullscreen()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            m_controller.showOverlay({});
            m_application.exit(streamInputStayedStable() ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    } else if (screenshotIndex >= 0 && screenshotIndex + 1 < m_arguments.size()) {
        const auto screenshotPath = m_arguments.at(screenshotIndex + 1);
        QTimer::singleShot(1'000, this,
                           [this, screenshotPath] {
            if (m_engine.rootObjects().isEmpty() || m_qmlWarningOccurred) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            const auto saved = window && window->grabWindow().save(screenshotPath);
            m_application.exit(saved ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    } else if (m_smokeStreamerEvent) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) return EXIT_FAILURE;
        const auto requested = std::make_shared<bool>(false);
        auto *timer = new QTimer(this);
        timer->setInterval(20);
        QObject::connect(timer, &QTimer::timeout, this,
                         [this, window, timer, requested] {
            if (m_qmlWarningOccurred) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (!*requested) {
                if (m_coreClient.state() != u"ready"_s) return;
                if (m_coreClient.request(u"test.streamer-event"_s).isEmpty()) {
                    qCritical("Could not request the streamer event fixture");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                *requested = true;
                return;
            }
            const auto snapshot = window->property("streamerSnapshotForSmokeTest").toMap();
            if (snapshot.value(u"status"_s).toString() == u"streaming"_s
                    && snapshot.value(u"firstFrameLatencyMs"_s).toInt() == 37
                    && snapshot.value(u"mediaBackend"_s).toString() == u"ffmpeg"_s
                    && snapshot.value(u"deviceRecoveryCount"_s).toInt() == 2
                    && snapshot.value(u"queueDropCount"_s).toInt() == 4) {
                timer->stop();
                m_application.exit(EXIT_SUCCESS);
            }
        });
        timer->start();
        QTimer::singleShot(3'000, this, [this] {
            qCritical("Flat streamer event smoke test timed out");
            m_application.exit(EXIT_FAILURE);
        });
    } else if (m_arguments.contains(u"--smoke-release-notes"_s)) {
        auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
        store->setProperty("updaterState", QVariantMap{
            {u"status"_s, u"available"_s}, {u"currentVersion"_s, QGuiApplication::applicationVersion()},
            {u"message"_s, u"A new release is available."_s}, {u"canDownload"_s, true}});
        store->setProperty("releaseHighlights", QVariantMap{{u"bodyMarkdown"_s,
            u"# Release notes\n\n**Bold fix** and *emphasis*.\n\n- First item\n- Second item\n\n"
             "[Project](https://github.com/OpenCloudGaming/OpenNOW)\n\n"
             "| Feature | Status |\n| --- | --- |\n| Updates | Ready |\n\n```text\ncode block\n```"_s}});
        QTimer::singleShot(500, this, [this] {
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            auto *notes = window->findChild<QObject *>(u"releaseNotesDocument"_s);
            const bool desktop = window->property("desktopSurfaceActive").toBool();
            const bool correctRoute = m_controller.route() != u"updates"_s
                || window->findChild<QObject *>(u"desktopUpdateScreen"_s);
            QString plain;
            const bool parsed = notes && QMetaObject::invokeMethod(notes, "getText",
                Q_RETURN_ARG(QString, plain), Q_ARG(int, 0),
                Q_ARG(int, notes->property("length").toInt()));
            const auto text = plain;
            const bool passed = desktop && correctRoute && parsed && !m_qmlWarningOccurred
                && text.contains(u"Bold fix"_s) && text.contains(u"Second item"_s)
                && text.contains(u"code block"_s) && !text.contains(u"**Bold fix**"_s)
                && !text.contains(u"# Release notes"_s)
                && notes->property("contentHeight").toReal() > 100
                && notes->property("height").toReal() >= notes->property("contentHeight").toReal()
                    + notes->property("topPadding").toReal() + notes->property("bottomPadding").toReal();
            if (!passed) qCritical("Release notes document or desktop update route failed");
            m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    } else if (m_smokeConsolePersistenceRollback) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) return EXIT_FAILURE;
        const auto phase = std::make_shared<int>(0);
        auto *timer = new QTimer(this);
        timer->setInterval(20);
        QObject::connect(timer, &QTimer::timeout, this,
                         [this, window, timer, phase] {
            if (m_qmlWarningOccurred) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (*phase == 0) {
                if (!window->property("settingsLoadedForSmokeTest").toBool()) return;
                const auto invoked = QMetaObject::invokeMethod(
                    window, "requestConsoleSurface", Q_ARG(QVariant, QVariant(true)));
                if (!invoked || window->property("desktopSurfaceActive").toBool()) {
                    qCritical("Console mode did not switch the rendered surface immediately");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                *phase = 1;
                return;
            }
            if (*phase == 1) {
                if (!window->property("consoleModePersistedForSmokeTest").toBool()
                        || window->property("modePersistenceBusyForSmokeTest").toBool()) return;
                const auto invoked = QMetaObject::invokeMethod(
                    window, "requestConsoleSurface", Q_ARG(QVariant, QVariant(false)));
                if (!invoked || !window->property("desktopSurfaceActive").toBool()) {
                    qCritical("Desktop mode did not switch the rendered surface immediately");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                *phase = 2;
                return;
            }
            if (*phase == 2) {
                if (window->property("consoleModePersistedForSmokeTest").toBool()
                        || window->property("modePersistenceBusyForSmokeTest").toBool()) return;
                const auto consoleInvoked = QMetaObject::invokeMethod(
                    window, "requestConsoleSurface", Q_ARG(QVariant, QVariant(true)));
                const auto desktopInvoked = QMetaObject::invokeMethod(
                    window, "requestConsoleSurface", Q_ARG(QVariant, QVariant(false)));
                if (!consoleInvoked || !desktopInvoked
                        || !window->property("desktopSurfaceActive").toBool()) {
                    qCritical("Rapid mode requests did not preserve the latest rendered surface");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                *phase = 3;
                return;
            }
            if (*phase == 3) {
                if (window->property("consoleModePersistedForSmokeTest").toBool()
                        || window->property("modePersistenceBusyForSmokeTest").toBool()) return;
                const auto invoked = QMetaObject::invokeMethod(
                    window, "requestConsoleSurface", Q_ARG(QVariant, QVariant(true)));
                if (!invoked || window->property("desktopSurfaceActive").toBool()) {
                    qCritical("Console mode did not switch after rapid request coalescing");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                *phase = 4;
                return;
            }
            if (*phase == 4) {
                if (!window->property("consoleModePersistedForSmokeTest").toBool()
                        || window->property("modePersistenceBusyForSmokeTest").toBool()) return;
                const auto invoked = QMetaObject::invokeMethod(
                    window, "requestConsoleSurface", Q_ARG(QVariant, QVariant(false)));
                if (!invoked || !window->property("desktopSurfaceActive").toBool()) {
                    qCritical("Desktop mode did not switch before persistence rollback");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                *phase = 5;
                return;
            }
            const auto error = window->property("modePersistenceErrorForSmokeTest").toString();
            if (!window->property("desktopSurfaceActive").toBool()
                    && window->property("consoleModePersistedForSmokeTest").toBool()
                    && !window->property("modePersistenceBusyForSmokeTest").toBool()
                    && error.contains(u"previous mode was restored"_s, Qt::CaseInsensitive)) {
                    timer->stop();
                    m_application.exit(EXIT_SUCCESS);
            }
        });
        timer->start();
        QTimer::singleShot(3'000, this, [this] {
            qCritical("Console mode persistence rollback smoke test timed out");
            m_application.exit(EXIT_FAILURE);
        });
    } else if (m_smokeTest) {
        QTimer::singleShot(500, this, [this] {
            auto *window = m_engine.rootObjects().isEmpty()
                ? nullptr
                : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            const auto *focused = window ? window->activeFocusItem() : nullptr;
            const auto failed = !window || m_qmlWarningOccurred || !focused
                || !focused->isVisible() || !focused->isEnabled();
            if (failed) {
                if (!window) {
                    qCritical("QML smoke test did not create a window");
                } else if (!focused) {
                    qCritical("QML smoke test lost active focus");
                } else {
                    qCritical("QML smoke test focus target is not usable: %s (visible=%d, enabled=%d)",
                              focused->metaObject()->className(), focused->isVisible(),
                              focused->isEnabled());
                }
            }
            m_application.exit(failed ? EXIT_FAILURE : EXIT_SUCCESS);
        });
    }
    return EXIT_SUCCESS;
}

int AcceptanceSession::startThemeSettingsWorkload()
{
    QQmlComponent component(&m_engine, QUrl(u"qrc:/acceptance/ThemeSettingsAcceptance.qml"_s));
    auto *fixture = component.create();
    if (!fixture) { qCritical() << component.errors(); return EXIT_FAILURE; }
    fixture->setParent(&m_engine);
    QTimer::singleShot(150, this, [this, fixture] {
        auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        QVariant passed;
        const bool ok = window && QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
            Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && passed.toBool() && !m_qmlWarningOccurred;
        if (!ok) { m_application.exit(EXIT_FAILURE); return; }
        QTimer::singleShot(150, this, [this, window] {
            const auto shot = m_arguments.indexOf(u"--screenshot"_s);
            const bool saved = shot < 0 || (shot + 1 < m_arguments.size()
                && window->grabWindow().save(m_arguments.at(shot + 1)));
            m_application.exit(saved && !m_qmlWarningOccurred ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    });
    return EXIT_SUCCESS;
}
