#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"
#include "core/CoreClient.h"
#include "streaming/NativeStreamRuntime.h"
#include "acceptance/MotionAcceptance.h"

#include <QColor>
#include <QGuiApplication>
#include <QKeyEvent>
#include <qpa/qwindowsysteminterface.h>
#include <QDeadlineTimer>
#include <QDateTime>
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

static qreal relativeLuminance(const QColor &color)
{
    return 0.2126 * color.redF() + 0.7152 * color.greenF() + 0.0722 * color.blueF();
}

static QRectF sceneRect(QQuickItem *item)
{
    return item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
}

static QQuickItem *findVisualChild(QQuickItem *item, const QString &name)
{
    if (!item)
        return nullptr;
    if (item->objectName() == name)
        return item;
    const auto children = item->childItems();
    for (auto *child : children) {
        if (auto *found = findVisualChild(child, name))
            return found;
    }
    return nullptr;
}

static void collectSettingsRailEntries(QQuickItem *item, QList<QQuickItem *> &found)
{
    if (!item)
        return;
    if (item->objectName().startsWith(u"settingsNavigation-"_s))
        found.append(item);
    const auto children = item->childItems();
    for (auto *child : children)
        collectSettingsRailEntries(child, found);
}

// The rail is a clipped Flickable whose entries scale with the interface scale,
// so it can be taller than its viewport. This asserts that an overflowing rail
// offers a scrollbar, that every entry is inside the viewport once scrolled to
// the end, and that a focused entry is pulled into view.
static bool settingsRailFits(QQuickWindow *window)
{
    auto *screen = window ? window->findChild<QQuickItem *>(u"desktopSettingsScreen"_s) : nullptr;
    if (!screen) {
        qCritical("Settings surface was not mounted for the rail check");
        return false;
    }
    if (!screen->isVisible()) {
        qCritical("Settings rail check ran while the settings surface was hidden");
        return false;
    }
    QList<QQuickItem *> buttons;
    collectSettingsRailEntries(screen, buttons);
    if (buttons.isEmpty()) {
        qCritical("Settings rail rendered no entries");
        return false;
    }
    QQuickItem *rail = nullptr;
    for (auto *parent = buttons.first()->parentItem(); parent; parent = parent->parentItem()) {
        if (parent->property("clip").toBool() && parent->property("contentHeight").isValid()) {
            rail = parent;
            break;
        }
    }
    if (!rail) {
        qCritical("Settings rail viewport was not found");
        return false;
    }
    if (rail->property("contentHeight").toReal() <= rail->height() + 1)
        return true;
    auto *bar = findVisualChild(rail, u"settingsRailScroll"_s);
    if (!bar || !bar->isVisible()) {
        qCritical("Settings rail overflows its viewport with no scroll affordance: content %f viewport %f",
                  rail->property("contentHeight").toReal(), rail->height());
        return false;
    }
    const auto viewport = sceneRect(rail);
    const auto inside = [&viewport](const QRectF &rect) {
        return rect.top() >= viewport.top() - 1 && rect.bottom() <= viewport.bottom() + 1;
    };
    rail->setProperty("contentY", 0.0);
    if (!inside(sceneRect(buttons.first()))) {
        qCritical("Settings rail clipped its first entry: %s", qPrintable(buttons.first()->objectName()));
        return false;
    }
    rail->setProperty("contentY", rail->property("contentHeight").toReal() - rail->height());
    if (!inside(sceneRect(buttons.last()))) {
        qCritical("Settings rail clipped its last entry at the end of the scroll: %s",
                  qPrintable(buttons.last()->objectName()));
        return false;
    }
    rail->setProperty("contentY", 0.0);
    buttons.last()->forceActiveFocus();
    if (!inside(sceneRect(buttons.last()))) {
        qCritical("Settings rail did not reveal the focused entry: %s contentY=%f",
                  qPrintable(buttons.last()->objectName()), rail->property("contentY").toReal());
        return false;
    }
    return true;
}

static bool accountChromeFits(QQuickWindow *window)
{
    if (!window)
        return false;
    if (auto *confirm = window->findChild<QQuickItem *>(u"accountSwitchConfirmation"_s); confirm && confirm->isVisible()) {
        auto *title = confirm->findChild<QQuickItem *>(u"accountSwitchTitle"_s);
        auto *body = confirm->findChild<QQuickItem *>(u"accountSwitchBody"_s);
        const auto titleColor = title ? title->property("color").value<QColor>() : QColor();
        const auto bodyColor = body ? body->property("color").value<QColor>() : QColor();
        if (!title || !body || relativeLuminance(titleColor) < 0.55 || relativeLuminance(bodyColor) < 0.28) {
            qCritical("Switch confirmation text is not readable on its dark card");
            return false;
        }
    }
    if (auto *picker = window->findChild<QQuickItem *>(u"desktopAccountPicker"_s); picker && picker->isVisible()) {
        auto *stage = picker->findChild<QQuickItem *>(u"accountPickerStage"_s);
        auto *grid = picker->findChild<QQuickItem *>(u"accountPickerGrid"_s);
        auto *heading = picker->findChild<QQuickItem *>(u"accountPickerHeading"_s);
        if (!stage || !grid || !heading)
            return false;
        const auto stageRect = sceneRect(stage);
        const auto gridRect = sceneRect(grid);
        const auto headingRect = sceneRect(heading);
        auto *manage = picker->findChild<QQuickItem *>(u"accountManageButton"_s);
        const auto manageRect = manage && manage->isVisible() ? sceneRect(manage) : QRectF();
        if (grid->height() < 100 || headingRect.top() < -1 || headingRect.bottom() > window->height() + 1
                || gridRect.top() < stageRect.top() - 1 || gridRect.bottom() > stageRect.bottom() + 1
                || (manage && manage->isVisible()
                    && (manageRect.top() < stageRect.top() - 1 || manageRect.bottom() > stageRect.bottom() + 1))) {
            qCritical("Account picker chrome leaves the window");
            return false;
        }
        auto *tile = findVisualChild(grid, u"accountTile0"_s);
        if (!tile || !tile->isVisible()) {
            qCritical("Account picker tile row is missing");
            return false;
        }
        const auto tileRect = sceneRect(tile);
        const auto visible = qMin(tileRect.bottom(), gridRect.bottom()) - qMax(tileRect.top(), gridRect.top());
        if (visible < 110) {
            qCritical("Account picker tile row is sliced");
            return false;
        }
        if (auto *error = picker->findChild<QQuickItem *>(u"accountActionError"_s);
                error && !error->property("text").toString().isEmpty()) {
            const auto errorRect = sceneRect(error);
            if (!error->isVisible() || errorRect.height() < 8
                    || errorRect.top() < stageRect.top() - 1 || errorRect.bottom() > stageRect.bottom() + 1) {
                qCritical("Account error is not visible inside the window");
                return false;
            }
        }
    }
    if (auto *card = window->findChild<QQuickItem *>(u"accountSwitcherCard"_s); card && card->isVisible()) {
        const auto rect = sceneRect(card);
        if (rect.top() < -1 || rect.bottom() > window->height() + 1 || rect.right() > window->width() + 1) {
            qCritical("Account switcher leaves the window");
            return false;
        }
        auto *switcher = window->findChild<QQuickItem *>(u"desktopAccountSwitcher"_s);
        auto *tiles = window->findChild<QQuickItem *>(u"accountSwitcherTiles"_s);
        if (switcher && tiles && tiles->property("contentHeight").toReal() > tiles->height() + 1) {
            switcher->setProperty("currentIndex", 0);
            tiles->setProperty("contentY", 0);
            switcher->setProperty("currentIndex", switcher->property("tileCount").toInt() - 1);
            if (tiles->property("contentY").toReal() <= 0) {
                qCritical("Switcher keyboard focus stays outside the tile list");
                return false;
            }
        }
    }
    return true;
}

void presentUpdateSurface(QQmlApplicationEngine &engine, QQuickWindow *window, const QString &surface)
{
    auto *store = engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!store)
        return;
    const auto replacement = u"This OpenNOW installation is registered with Windows Installer. Replace it once with setup.exe. In-app updates do not run Windows Installer."_s;
    QVariantMap state{{u"currentVersion"_s, QGuiApplication::applicationVersion()},
                      {u"canCheck"_s, false}};
    const auto release = u"https://github.com/OpenCloudGaming/OpenNOW/releases/tag/v1.2.3"_s;
    if (surface == u"confirm"_s || surface == u"ready"_s) {
        state.insert(u"status"_s, u"downloaded"_s);
        state.insert(u"availableVersion"_s, u"1.2.3"_s);
        state.insert(u"downloadedVersion"_s, u"1.2.3"_s);
        state.insert(u"releaseUrl"_s, release);
        state.insert(u"canInstall"_s, true);
        state.insert(u"message"_s, u"OpenNOW 1.2.3 downloaded and verified. Ready to install."_s);
    } else if (surface == u"progress"_s) {
        state.insert(u"status"_s, u"downloading"_s);
        state.insert(u"availableVersion"_s, u"1.2.3"_s);
        state.insert(u"releaseUrl"_s, release);
        state.insert(u"message"_s, u"Downloading OpenNOW 1.2.3…"_s);
        state.insert(u"downloadProgress"_s, QVariantMap{{u"receivedBytes"_s, 40'524'930},
                                                       {u"totalBytes"_s, 96'487'928}});
    } else if (surface == u"available"_s) {
        state.insert(u"status"_s, u"available"_s);
        state.insert(u"availableVersion"_s, u"1.2.3"_s);
        state.insert(u"releaseUrl"_s, release);
        state.insert(u"canCheck"_s, true);
        state.insert(u"canDownload"_s, true);
        state.insert(u"message"_s, u"OpenNOW 1.2.3 is available with signed update metadata."_s);
    } else if (surface == u"staged"_s) {
        state.insert(u"status"_s, u"available"_s);
        state.insert(u"availableVersion"_s, u"1.2.4"_s);
        state.insert(u"downloadedVersion"_s, u"1.2.3"_s);
        state.insert(u"releaseUrl"_s, release);
        state.insert(u"canCheck"_s, true);
        state.insert(u"canDownload"_s, true);
        state.insert(u"canInstall"_s, true);
        state.insert(u"message"_s, u"OpenNOW 1.2.4 is available with signed update metadata."_s);
    } else if (surface == u"error"_s) {
        state.insert(u"status"_s, u"error"_s);
        state.insert(u"canCheck"_s, true);
        state.insert(u"message"_s, u"Unable to reach GitHub Releases right now."_s);
    } else if (surface == u"uptodate"_s) {
        state.insert(u"status"_s, u"not-available"_s);
        state.insert(u"canCheck"_s, true);
        state.insert(u"lastCheckedAt"_s, u"1"_s);
        state.insert(u"message"_s, u"OpenNOW is up to date."_s);
    } else {
        state.insert(u"status"_s, u"not-available"_s);
        state.insert(u"message"_s, replacement);
    }
    store->setProperty("updaterError", QString());
    store->setProperty("updaterState", state);
    if (surface != u"error"_s && surface != u"other"_s) {
        store->setProperty("releaseHighlights", QVariantMap{{u"version"_s, u"1.2.3"_s}, {u"title"_s, u"OpenNOW 1.2.3"_s},
            {u"bodyMarkdown"_s, u"- Anti-AFK in desktop streams\n- Keep account chrome readable inside the window\n"
                                "- Left-docked Ctrl+G drawer with per-controller tuning\n"
                                "- Generic HID passthrough for HOTAS, flight controls and wheels\n"_s}});
    }
    if (surface == u"confirm"_s && window) {
        // Settings › Updates and the closed status-bar panel each own an install
        // action; open the confirmation that belongs to the visible one.
        for (auto *dialog : window->findChildren<QObject *>(u"updateInstallConfirmation"_s)) {
            auto *owner = qobject_cast<QQuickItem *>(dialog->parent());
            if (owner && owner->isVisible()) {
                QMetaObject::invokeMethod(dialog, "open");
                break;
            }
        }
    }
}

int AcceptanceSession::startSmokeWorkload()
{
    if (m_smokeTest && m_arguments.contains(u"--smoke-console-design"_s)
            && m_arguments.contains(u"--smoke-interactive"_s))
        return EXIT_SUCCESS;
    auto *launchWindow = m_engine.rootObjects().isEmpty() ? nullptr : m_engine.rootObjects().first();
    if (m_smokeTest && launchWindow
            && (!launchWindow->property("startupLaunchConsidered").toBool()
                || !launchWindow->property("consoleLaunchInputReady").toBool())) {
        auto *timer = new QTimer(this);
        timer->setInterval(25);
        connect(timer, &QTimer::timeout, this,
                [this, timer, launchWindow, deadline = QDeadlineTimer(8000)] {
            if (launchWindow->property("startupLaunchConsidered").toBool()
                    && launchWindow->property("consoleLaunchInputReady").toBool()) {
                timer->stop();
                timer->deleteLater();
                if (startSmokeWorkload() != EXIT_SUCCESS) m_application.exit(EXIT_FAILURE);
            } else if (deadline.hasExpired()) {
                const auto *input = m_engine.rootContext()->contextProperty(u"ControllerInput"_s).value<QObject *>();
                qCritical() << "Console launch did not release acceptance input within eight seconds"
                    << "startupConsidered=" << launchWindow->property("startupLaunchConsidered")
                    << "active=" << launchWindow->property("consoleLaunchActiveForSmokeTest")
                    << "framePresented=" << launchWindow->property("launchFramePresented")
                    << "inputReady=" << launchWindow->property("consoleLaunchInputReady")
                    << "settingsLoaded=" << launchWindow->property("settingsLoadedForSmokeTest")
                    << "blocked=" << m_controller.consoleLaunchInputBlocked()
                    << "keyboardDrain=" << m_controller.consoleLaunchInputDraining()
                    << "controllerBlocked=" << (input ? input->property("shellInputBlocked") : QVariant())
                    << "controllerDrain=" << (input ? input->property("shellInputDraining") : QVariant());
                m_application.exit(EXIT_FAILURE);
            }
        });
        timer->start();
        return EXIT_SUCCESS;
    }
    if (m_smokeTest && m_arguments.contains(u"--smoke-initial-console-warning"_s)) {
        auto *window = m_engine.rootObjects().isEmpty() ? nullptr
            : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) return EXIT_FAILURE;
        window->requestActivate();
        QTimer::singleShot(350, this, [this, window] {
            if (m_qmlWarningOccurred || m_controller.overlay() != u"application-quit-confirm"_s
                    || !window->activeFocusItem()
                    || window->activeFocusItem()->objectName() != u"quitConfirmKeepOpen"_s) {
                qCritical("Initial console warning did not retain safe keyboard focus");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            const auto route = m_controller.route();
            auto *loader = window->findChild<QObject *>(u"mainRouteLoader"_s);
            auto *page = loader ? qobject_cast<QQuickItem *>(loader->property("item").value<QObject *>()) : nullptr;
            if (!page) { m_application.exit(EXIT_FAILURE); return; }
            page->forceActiveFocus();
            for (const auto key : {Qt::Key_X, Qt::Key_PageDown}) {
                QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &press);
                QKeyEvent release(QEvent::KeyRelease, key, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &release);
            }
            if (m_controller.route() != route || m_controller.overlay() != u"application-quit-confirm"_s) {
                qCritical("A background focus request bypassed the console warning");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            QTimer::singleShot(50, this, [this, window, route] {
                if (!window->activeFocusItem()
                        || window->activeFocusItem()->objectName() != u"quitConfirmKeepOpen"_s) {
                    qCritical("Console warning did not reclaim safe focus");
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                QKeyEvent press(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &press);
                QKeyEvent release(QEvent::KeyRelease, Qt::Key_Return, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &release);
                const bool safe = m_controller.overlay().isEmpty() && m_controller.route() == route
                    && !window->property("applicationCloseConfirmed").toBool();
                m_application.exit(safe ? EXIT_SUCCESS : EXIT_FAILURE);
            });
        });
        return EXIT_SUCCESS;
    }
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
            const bool terminal = mode == u"finished"_s || mode == u"not-found"_s;
            const auto expectedState = terminal ? u"idle"_s : mode == u"unavailable"_s ? u"error"_s : mode;
            if (!store || store->property("streamState").toString() != expectedState
                    || m_controller.route() != (terminal ? u"game-detail"_s : u"inserting"_s)
                    || (terminal && (!store->property("activeSession").isNull()
                        || store->property("sessionRecoveryPending").toBool()
                        || !store->property("sessionClaimRequestId").toString().isEmpty()))
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
    if (m_smokeTest && m_arguments.contains(u"--smoke-console-session"_s))
        return startConsoleSessionWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-session-fullscreen"_s))
        return startSessionFullscreenWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-steam-launch"_s))
        return startSteamLaunchWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-stream-exit"_s))
        return startStreamExitWorkload();
    if (m_smokeTest && m_arguments.contains(u"--smoke-frame-generation-stats"_s))
        return startFrameGenerationStatsWorkload();
    if (m_smokeTest && (m_arguments.contains(u"--smoke-language-settings"_s)
                       || m_arguments.contains(u"--smoke-frame-rate-settings"_s)
                       || m_arguments.contains(u"--smoke-frame-generation"_s)
                       || m_arguments.contains(u"--smoke-ten-bit-warning"_s)
                       || m_arguments.contains(u"--smoke-onboarding"_s)
                       || m_arguments.contains(u"--smoke-upscaling"_s)
                       || m_arguments.contains(u"--smoke-stream-stats"_s)
                       || m_arguments.contains(u"--smoke-stream-stats-v2"_s)
                       || m_arguments.contains(u"--smoke-controller-metadata"_s)
                       || m_arguments.contains(u"--smoke-custom-background"_s))) {
        const bool controllerMetadata = m_arguments.contains(u"--smoke-controller-metadata"_s);
        const bool languageSettings = m_arguments.contains(u"--smoke-language-settings"_s);
        const bool frameRateSettings = m_arguments.contains(u"--smoke-frame-rate-settings"_s);
        const bool onboarding = m_arguments.contains(u"--smoke-onboarding"_s);
        const bool tenBitWarning = m_arguments.contains(u"--smoke-ten-bit-warning"_s);
        const bool customBackground = m_arguments.contains(u"--smoke-custom-background"_s);
        const bool streamStats = m_arguments.contains(u"--smoke-stream-stats"_s);
        QQmlComponent component(&m_engine, QUrl(languageSettings
            ? u"qrc:/acceptance/LanguageSettingsAcceptance.qml"_s
            : frameRateSettings
            ? u"qrc:/acceptance/FrameRateSettingsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-onboarding"_s)
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
        if (languageSettings && m_arguments.contains(u"--language-hdr-invalidation"_s))
            m_engine.rootContext()->setContextProperty(u"HdrOutput"_s, fixture->property("hdrOutput").value<QObject *>());
        if (customBackground) {
            QFile image(u":/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s);
            auto *localImage = new QTemporaryFile(&m_engine);
            if (!image.open(QIODevice::ReadOnly) || !localImage->open()) return EXIT_FAILURE;
            const auto data = image.readAll();
            if (localImage->write(data) != data.size() || !localImage->flush()) return EXIT_FAILURE;
            fixture->setProperty("imageUrl", QUrl::fromLocalFile(localImage->fileName()).toString());
            localImage->close();
        }
        QTimer::singleShot(150, this, [this, fixture, customBackground, onboarding, tenBitWarning, languageSettings, frameRateSettings] {
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            QVariant passed;
            const bool ok = window && QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
                Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && passed.toBool() && !m_qmlWarningOccurred;
            if (!ok) { m_application.exit(EXIT_FAILURE); return; }
            const auto finish = [this, window, fixture, customBackground, onboarding, tenBitWarning, languageSettings, frameRateSettings] {
                if (languageSettings) {
                    const QList<int> keys = m_arguments.contains(u"--language-keyboard-selection"_s)
                        ? QList<int>{Qt::Key_Tab, Qt::Key_Return} : QList<int>{Qt::Key_Escape};
                    for (const auto key : keys) {
                        QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier);
                        QKeyEvent release(QEvent::KeyRelease, key, Qt::NoModifier);
                        QGuiApplication::sendEvent(window, &press);
                        QGuiApplication::sendEvent(window, &release);
                    }
                }
                if (customBackground || onboarding || tenBitWarning || languageSettings || frameRateSettings) {
                    QVariant verified;
                    if (!QMetaObject::invokeMethod(fixture, "verify", Q_RETURN_ARG(QVariant, verified))
                        || !verified.toBool() || m_qmlWarningOccurred) {
                        m_application.exit(EXIT_FAILURE);
                        return;
                    }
                }
                QTimer::singleShot(languageSettings && m_arguments.contains(u"--language-preview"_s) ? 60000 : 150, this, [this, window] {
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
    } else if (m_smokeTest && (m_arguments.contains(u"--smoke-stream-idle-warning"_s)
            || m_arguments.contains(u"--smoke-stream-text-input-error"_s))) {
        const bool textInputError = m_arguments.contains(u"--smoke-stream-text-input-error"_s);
        auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
        auto *runtime = m_engine.rootContext()->contextProperty(u"NativeStreamRuntime"_s).value<QObject *>();
        auto *window = m_engine.rootObjects().isEmpty() ? nullptr
            : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!store || !runtime || !window || m_coreClient.state() == u"ready"_s) return EXIT_FAILURE;
        store->setProperty("streamerStartRequestId", u"idle-warning-fixture"_s);
        store->setProperty("streamInputPauseRequestId", u"idle-warning-fixture"_s);
        store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"idle-warning-fixture"_s},
            {u"phase"_s, u"ready"_s}});
        store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s},
            {u"sessionId"_s, u"idle-warning-fixture"_s}, {u"firstFrameLatencyMs"_s, 37}});
        store->setProperty("streamState", u"streaming"_s);
        if (m_arguments.contains(u"--smoke-idle-fullscreen"_s)) window->showFullScreen();
        window->requestActivate();
        const auto overlay = m_arguments.contains(u"--smoke-idle-stats"_s)
            ? (m_arguments.contains(u"--desktop"_s) ? u"desktop-stream-stats"_s : u"stream-stats"_s)
            : QString{};
        m_controller.showOverlay(overlay);
        QTimer::singleShot(250, this, [this, store, runtime, window, overlay, textInputError] {
            auto *surface = window->findChild<QQuickItem *>(u"streamSurfaceHost"_s);
            auto *notice = window->findChild<QQuickItem *>(textInputError
                ? u"streamInputCaptureNotice"_s : u"streamIdleWarning"_s);
            if (!surface || !notice || !surface->isVisible()) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            const auto inputEnabled = surface->property("inputEnabled").toBool();
            const auto focus = window->activeFocusItem();
            const auto route = m_controller.route();
            const auto previousNotice = notice->property("message").toString();
            const auto textError = [store] {
                const QVariant event = QVariantMap{{u"type"_s, u"text-input-error"_s},
                    {u"reason"_s, u"paste-queue-full"_s}};
                return QMetaObject::invokeMethod(store, "acceptNativeEvent", Q_ARG(QVariant, event));
            };
            const auto timer = [runtime](quint8 type, quint32 seconds, bool active) {
                return QMetaObject::invokeMethod(runtime, "sessionTimerReceived",
                    Q_ARG(quint8, type), Q_ARG(quint32, seconds), Q_ARG(bool, active));
            };
            if (!(textInputError ? textError() : timer(OPENNOW_STREAMER_TIMER_IDLE_WARNING, 60, true))) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            QTimer::singleShot(150, this, [this, store, window, surface, notice, timer,
                                          inputEnabled, focus, route, overlay, textInputError, textError, previousNotice] {
                const auto stable = [this, store, window, surface, inputEnabled, focus, route, overlay] {
                    return !m_qmlWarningOccurred && surface->isVisible()
                        && window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) == surface
                        && surface->property("inputEnabled").toBool() == inputEnabled
                        && window->activeFocusItem() == focus && m_controller.route() == route
                        && m_controller.overlay() == overlay
                        && store->property("streamState").toString() == u"streaming"_s
                        && store->property("streamerStartRequestId").toString() == u"idle-warning-fixture"_s;
                };
                bool passed = stable() && inputEnabled && focus == surface && notice->isVisible();
                QKeyEvent press(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &press);
                QKeyEvent release(QEvent::KeyRelease, Qt::Key_Escape, Qt::NoModifier);
                QGuiApplication::sendEvent(window, &release);
                passed = passed && stable() && notice->isVisible();
                const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                if (shot >= 0 && shot + 1 < m_arguments.size())
                    passed = passed && QFileInfo(m_arguments.at(shot + 1)).isAbsolute()
                        && window->grabWindow().save(m_arguments.at(shot + 1));
                if (textInputError) {
                    const auto message = u"Text input could not be delivered. Try pasting again."_s;
                    auto *noticeTimer = window->findChild<QObject *>(u"streamClipboardPasteNoticeTimer"_s);
                    passed = passed && noticeTimer && noticeTimer->property("interval").toInt() == 5000
                        && notice->property("message").toString() == message
                        && notice->property("title").toString() == u"Text input unavailable"_s
                        && QMetaObject::invokeMethod(surface, "clipboardPasteFailed")
                        && notice->property("message").toString()
                            == u"Clipboard paste failed. Use plain text up to 64 KiB and try again."_s
                        && textError() && notice->property("message").toString() == message && stable();
                    if (!passed) {
                        qCritical("Text retry notice did not preserve synchronous copy or stream ownership");
                        m_application.exit(EXIT_FAILURE);
                        return;
                    }
                    auto *expiry = new QTimer(this);
                    expiry->setInterval(50);
                    connect(expiry, &QTimer::timeout, this,
                            [this, store, notice, noticeTimer, stable, textError, previousNotice, expiry,
                             deadline = QDeadlineTimer(7'500)] {
                        if (noticeTimer->property("running").toBool() && !deadline.hasExpired()) return;
                        expiry->stop();
                        expiry->deleteLater();
                        const bool expired = !noticeTimer->property("running").toBool();
                        const bool streamStable = stable();
                        const auto afterTimeout = notice->property("message").toString();
                        bool passed = streamStable && expired && afterTimeout == previousNotice;
                        passed = passed && textError() && notice->isVisible();
                        store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"replacement-fixture"_s}});
                        passed = passed && !noticeTimer->property("running").toBool()
                            && notice->property("message").toString() == previousNotice;
                        if (!passed) qCritical() << "Text retry notice did not expire or reset on session replacement"
                            << "stable=" << streamStable << "expired=" << expired
                            << "message=" << afterTimeout << "previous=" << previousNotice;
                        m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
                    });
                    expiry->start();
                    return;
                }
                notice->setProperty("warningNowMs", double(QDateTime::currentMSecsSinceEpoch() + 2'000));
                passed = passed && notice->property("secondsLeft").toDouble() <= 58
                    && notice->property("secondsLeft").toDouble() > 0 && notice->isVisible() && stable();
                notice->setProperty("warningNowMs", double(QDateTime::currentMSecsSinceEpoch() + 61'000));
                passed = passed && notice->property("secondsLeft").toDouble() == 0
                    && notice->isVisible() && stable();
                passed = passed && timer(OPENNOW_STREAMER_TIMER_IDLE_WARNING, 0, false)
                    && !notice->isVisible() && stable();
                passed = passed && timer(OPENNOW_STREAMER_TIMER_IDLE_WARNING, 60, true)
                    && notice->isVisible() && notice->property("secondsLeft").toDouble() == 60
                    && timer(OPENNOW_STREAMER_TIMER_IDLE_EXPIRY, 0, true)
                    && !notice->isVisible() && stable();
                passed = passed && timer(OPENNOW_STREAMER_TIMER_IDLE_WARNING, 60, true)
                    && notice->isVisible();
                store->setProperty("activeSession", QVariant());
                passed = passed && !notice->isVisible();
                store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"idle-warning-fixture"_s}});
                passed = passed && timer(OPENNOW_STREAMER_TIMER_IDLE_WARNING, 60, true) && notice->isVisible();
                store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"replacement-fixture"_s}});
                passed = passed && !notice->isVisible();
                if (!passed) qCritical("Idle warning changed stream identity, focus, input ownership, or lifecycle");
                m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
            });
        });
    } else if (m_smokeTest && (m_arguments.contains(u"--smoke-input-capture-error"_s)
                              || m_arguments.contains(u"--smoke-pointer-lock"_s))) {
        auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
        if (!store) return EXIT_FAILURE;
        store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s}, {u"firstFrameLatencyMs"_s, 37}});
        store->setProperty("streamState", u"streaming"_s);
        QTimer::singleShot(150, this, [this] {
            const bool pointerLock = m_arguments.contains(u"--smoke-pointer-lock"_s);
            auto *window = m_engine.rootObjects().isEmpty() ? nullptr
                : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            auto *notice = window ? window->findChild<QQuickItem *>(pointerLock
                ? u"streamPointerLockHint"_s : u"streamInputCaptureNotice"_s) : nullptr;
            auto *surface = window ? window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) : nullptr;
            if (!notice || !surface || !surface->isVisible()) {
                qCritical("Input capture fixture could not find the visible stream surface and notice");
                m_application.exit(EXIT_FAILURE);
                return;
            }
            QObject *fixture = nullptr;
            if (pointerLock) {
                notice->setProperty("active", true);
                surface->setProperty("macPointerLockEnabled", false);
                QQmlComponent component(&m_engine, QUrl(u"qrc:/acceptance/PointerLockHintAcceptance.qml"_s));
                fixture = component.create();
                if (fixture) fixture->setParent(&m_engine);
                QVariant passed;
                if (!fixture || !QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
                        Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) || !passed.toBool()) {
                    qCritical() << component.errors();
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
            } else {
                notice->setProperty("message", u"Relative mouse input requires Wayland relative-pointer-v1 and pointer-constraints-v1; the compositor does not provide both protocols."_s);
            }
            QTimer::singleShot(250, this, [this, window, notice, surface, fixture] {
                bool passed = !m_qmlWarningOccurred && notice->isVisible() && surface->isVisible()
                    && window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) == surface;
                if (fixture) {
                    QVariant verified;
                    passed = passed && QMetaObject::invokeMethod(fixture, "verifyRendered", Q_RETURN_ARG(QVariant, verified),
                        Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && verified.toBool();
                }
                const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                if (shot >= 0 && shot + 1 < m_arguments.size())
                    passed = passed && QFileInfo(m_arguments.at(shot + 1)).isAbsolute()
                        && window->grabWindow().save(m_arguments.at(shot + 1));
                if (!passed) qCritical("Input capture fixture did not preserve the visible stream surface");
                m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
            });
        });
    } else if (m_smokeTest && (m_arguments.contains(u"--smoke-backend-availability"_s)
                     || m_arguments.contains(u"--smoke-queue-selector"_s)
                     || m_arguments.contains(u"--smoke-command-search"_s)
                     || m_arguments.contains(u"--smoke-game-details-layout"_s)
                     || m_arguments.contains(u"--smoke-game-dlss"_s)
                     || m_arguments.contains(u"--smoke-ownership"_s)
                     || m_arguments.contains(u"--smoke-game-labels"_s)
                     || m_arguments.contains(u"--smoke-catalog-sync"_s)
                     || m_arguments.contains(u"--smoke-library-error"_s)
                     || m_arguments.contains(u"--smoke-push-invalidation"_s)
                     || m_arguments.contains(u"--smoke-bug-reports"_s)
                     || m_arguments.contains(u"--smoke-microphone"_s)
                     || m_arguments.contains(u"--smoke-desktop-anti-afk"_s)
                     || m_arguments.contains(u"--smoke-stream-controllers"_s)
                     || m_arguments.contains(u"--smoke-flight-controls"_s)
                     || m_arguments.contains(u"--smoke-audio-output"_s)
                     || m_arguments.contains(u"--smoke-background-stream"_s)
                     || m_arguments.contains(u"--smoke-recording"_s)
                     || m_arguments.contains(u"--smoke-shortcuts"_s)
                     || m_arguments.contains(u"--smoke-console-settings"_s)
                     || m_arguments.contains(u"--smoke-collections"_s)
                     || m_arguments.contains(u"--smoke-steam-big-picture"_s)
                     || m_arguments.contains(u"--smoke-persistent-in-game-settings"_s)
                     || m_arguments.contains(u"--smoke-save-bandwidth"_s)
                     || m_arguments.contains(u"--smoke-store-launch"_s)
                     || m_arguments.contains(u"--smoke-network-test"_s)
                     || m_arguments.contains(u"--smoke-idle-mode"_s)
                     || m_arguments.contains(u"--smoke-console-launch"_s)
                     || m_arguments.contains(u"--smoke-console-spacing"_s)
                     || m_arguments.contains(u"--smoke-queue-drops"_s)
                     || m_arguments.contains(u"--smoke-color-format"_s)
                     || m_arguments.contains(u"--smoke-hdr-downgrade"_s)
                     || m_arguments.contains(u"--smoke-decoder-notice"_s)
                     || m_arguments.contains(u"--smoke-stream-toast-pending"_s)
                     || m_arguments.contains(u"--smoke-stream-recovery"_s)
                     || m_arguments.contains(u"--smoke-stream-end-notice-check"_s))) {
        QQmlComponent component(&m_engine, QUrl(m_arguments.contains(u"--smoke-library-error"_s)
            ? u"qrc:/acceptance/LibraryErrorAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-command-search"_s)
            ? u"qrc:/acceptance/CommandSearchAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-queue-selector"_s)
            ? u"qrc:/acceptance/QueueSelectorAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-game-details-layout"_s)
            ? u"qrc:/acceptance/GameDetailsLayoutAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-game-dlss"_s)
            ? u"qrc:/acceptance/GameDlssAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-color-format"_s)
            ? u"qrc:/acceptance/ColorFormatAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-hdr-downgrade"_s)
            ? u"qrc:/acceptance/HdrDowngradeAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-decoder-notice"_s)
            ? u"qrc:/acceptance/StreamDecoderNoticeAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-stream-toast-pending"_s)
            ? u"qrc:/acceptance/StreamToastPendingAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-ownership"_s)
            ? u"qrc:/acceptance/OwnershipAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-game-labels"_s)
            ? u"qrc:/acceptance/GameLabelsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-catalog-sync"_s)
            ? u"qrc:/acceptance/CatalogSyncAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-push-invalidation"_s)
            ? u"qrc:/acceptance/PushInvalidationAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-bug-reports"_s)
            ? u"qrc:/acceptance/BugReportAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-queue-drops"_s)
            ? u"qrc:/acceptance/QueueDropsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-microphone"_s)
            ? u"qrc:/acceptance/MicrophoneAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-desktop-anti-afk"_s)
            ? u"qrc:/acceptance/DesktopAntiAfkAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-stream-controllers"_s)
            ? u"qrc:/acceptance/DesktopStreamControllersAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-flight-controls"_s)
            ? u"qrc:/acceptance/FlightControlsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-audio-output"_s)
            ? u"qrc:/acceptance/AudioOutputAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-background-stream"_s)
            ? u"qrc:/acceptance/BackgroundStreamAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-recording"_s)
            ? u"qrc:/acceptance/RecordingAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-shortcuts"_s)
            ? u"qrc:/acceptance/ShortcutsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-console-settings"_s)
            ? u"qrc:/acceptance/ConsoleSettingsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-collections"_s)
            ? u"qrc:/acceptance/CollectionsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-steam-big-picture"_s)
            ? u"qrc:/acceptance/SteamBigPictureAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-persistent-in-game-settings"_s)
            ? u"qrc:/acceptance/PersistentInGameSettingsAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-save-bandwidth"_s)
            ? u"qrc:/acceptance/SaveBandwidthAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-store-launch"_s)
            ? u"qrc:/acceptance/StoreLaunchAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-network-test"_s)
            ? u"qrc:/acceptance/NetworkTestAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-idle-mode"_s)
            ? u"qrc:/acceptance/IdleModeAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-console-launch"_s)
            ? u"qrc:/acceptance/ConsoleLaunchAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-console-spacing"_s)
            ? u"qrc:/acceptance/ConsoleSpacingAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-stream-recovery"_s)
            ? u"qrc:/acceptance/StreamRecoveryAcceptance.qml"_s
            : m_arguments.contains(u"--smoke-stream-end-notice-check"_s)
            ? u"qrc:/acceptance/StreamEndNoticeAcceptance.qml"_s
            : u"qrc:/acceptance/BackendAvailabilityAcceptance.qml"_s));
        auto *fixture = component.create();
        if (!fixture) { qCritical() << component.errors(); return EXIT_FAILURE; }
        fixture->setParent(&m_engine);
        if (m_arguments.contains(u"--smoke-console-spacing"_s))
            fixture->setProperty("recordingSection", m_arguments.contains(u"--smoke-spacing-recording"_s));
        if (m_arguments.contains(u"--smoke-stream-controllers"_s)
                || m_arguments.contains(u"--smoke-flight-controls"_s)) {
            auto *input = fixture->property("input").value<QObject *>();
            if (!input) return EXIT_FAILURE;
            if (m_arguments.contains(u"--smoke-flight-controls"_s)) {
                auto *liveInput = m_engine.rootContext()->contextProperty(u"ControllerInput"_s).value<QObject *>();
                if (!liveInput || !liveInput->property("supportedFlightDevices").isValid()) return EXIT_FAILURE;
                input->setProperty("supportedFlightDevices", liveInput->property("supportedFlightDevices"));
            }
            m_engine.rootContext()->setContextProperty(u"ControllerInput"_s, input);
        }
        if (m_arguments.contains(u"--smoke-stream-recovery"_s)
            || m_arguments.contains(u"--smoke-microphone"_s)
            || m_arguments.contains(u"--smoke-recording"_s)
            || m_arguments.contains(u"--smoke-queue-drops"_s)) {
            auto *runtime = fixture->property("runtime").value<QObject *>();
            if (!runtime) return EXIT_FAILURE;
            m_engine.rootContext()->setContextProperty(u"NativeStreamRuntime"_s, runtime);
        }
        if (m_arguments.contains(u"--smoke-stream-recovery"_s)
            || m_arguments.contains(u"--smoke-queue-selector"_s)
            || m_arguments.contains(u"--smoke-command-search"_s)
            || m_arguments.contains(u"--smoke-game-details-layout"_s)
            || m_arguments.contains(u"--smoke-game-dlss"_s)
            || m_arguments.contains(u"--smoke-ownership"_s)
            || m_arguments.contains(u"--smoke-catalog-sync"_s)
            || m_arguments.contains(u"--smoke-library-error"_s)
            || m_arguments.contains(u"--smoke-push-invalidation"_s)
            || m_arguments.contains(u"--smoke-bug-reports"_s)
            || m_arguments.contains(u"--smoke-stream-end-notice-check"_s)
            || m_arguments.contains(u"--smoke-recording"_s)
            || m_arguments.contains(u"--smoke-shortcuts"_s)
            || m_arguments.contains(u"--smoke-queue-drops"_s)
            || m_arguments.contains(u"--smoke-collections"_s)
            || m_arguments.contains(u"--smoke-steam-big-picture"_s)
            || m_arguments.contains(u"--smoke-persistent-in-game-settings"_s)
            || m_arguments.contains(u"--smoke-save-bandwidth"_s)
            || m_arguments.contains(u"--smoke-store-launch"_s)
            || m_arguments.contains(u"--smoke-network-test"_s)) {
            auto *client = fixture->property("client").value<QObject *>();
            if (!client) return EXIT_FAILURE;
            m_engine.rootContext()->setContextProperty(u"CoreClient"_s, client);
        }
        QTimer::singleShot(150, this, [this, fixture] {
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            if (window && (m_arguments.contains(u"--smoke-color-format-fullscreen"_s)
                    || m_arguments.contains(u"--smoke-hdr-downgrade-fullscreen"_s)
                    || m_arguments.contains(u"--smoke-store-launch-fullscreen"_s)
                    || m_arguments.contains(u"--smoke-stream-recovery-fullscreen"_s)))
                window->showFullScreen();
            if (window && m_arguments.contains(u"--smoke-flight-controls"_s)
                    && m_arguments.contains(u"--flight-fullscreen"_s))
                window->showFullScreen();
            QVariant passed;
            const bool ok = window && QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
                Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && passed.toBool() && !m_qmlWarningOccurred;
            if (ok && m_arguments.contains(u"--smoke-flight-controls"_s)
                    && fixture->property("flightState").toString() == u"badges"_s) {
                auto *host = fixture->property("badgeHost").value<QQuickItem *>();
                const char *inputs[]{"badgeActiveInput", "badgeDeclinedInput", "badgeOffInput"};
                for (int index = 0; index < 3; ++index) {
                    auto *context = new QQmlContext(m_engine.rootContext(), fixture);
                    context->setContextProperty(u"ControllerInput"_s, fixture->property(inputs[index]).value<QObject *>());
                    QQmlComponent statsComponent(&m_engine);
                    statsComponent.loadFromModule(u"OpenNOW"_s, u"DesktopStreamStats"_s);
                    auto *stats = qobject_cast<QQuickItem *>(statsComponent.create(context));
                    if (!host || !stats) {
                        qCritical() << statsComponent.errors();
                        m_application.exit(EXIT_FAILURE);
                        return;
                    }
                    stats->setParent(host);
                    stats->setParentItem(host);
                    stats->setObjectName(u"flightBadgeStats%1"_s.arg(index));
                    stats->setSize(QSizeF(710, 90));
                    stats->setX(218);
                    stats->setY(index * 58 + 4);
                }
            }
            if (ok && m_arguments.contains(u"--smoke-color-format"_s)) {
                QTimer::singleShot(150, this, [this, window, fixture] {
                    QVariant notified;
                    if (!QMetaObject::invokeMethod(fixture, "notify", Q_RETURN_ARG(QVariant, notified),
                            Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) || !notified.toBool())
                        m_application.exit(EXIT_FAILURE);
                });
            }
            if (ok && (m_arguments.contains(u"--smoke-command-search"_s)
                       || m_arguments.contains(u"--smoke-stream-toast-pending"_s)
                       || m_arguments.contains(u"--smoke-stream-controllers"_s)
                       || m_arguments.contains(u"--smoke-flight-controls"_s)
                       || m_arguments.contains(u"--smoke-shortcuts"_s)
                       || m_arguments.contains(u"--smoke-console-settings"_s)
                       || m_arguments.contains(u"--bug-report-notice-check"_s)
                       || m_arguments.contains(u"--smoke-stream-end-notice-check"_s)
                       || m_arguments.contains(u"--smoke-game-dlss"_s)
                       || m_arguments.contains(u"--smoke-game-details-layout"_s))) {
                if (m_arguments.contains(u"--smoke-game-details-layout"_s)
                    && m_arguments.contains(u"--details-interactive"_s))
                    return;
                auto *timer = new QTimer(this);
                timer->setInterval(25);
                connect(timer, &QTimer::timeout, this, [this, fixture, window, timer, deadline = QDeadlineTimer(15000)] {
                    const auto key = fixture->property("key").toInt();
                    if (key != 0) {
                        fixture->setProperty("key", 0);
                        const auto modifiers = Qt::KeyboardModifiers(fixture->property("modifiers").toInt());
                        QKeyEvent press(QEvent::KeyPress, key, modifiers);
                        QKeyEvent release(QEvent::KeyRelease, key, modifiers);
                        QGuiApplication::sendEvent(window, &press);
                        QGuiApplication::sendEvent(window, &release);
                    }
                    if (const auto systemKey = fixture->property("systemKey").toInt(); systemKey != 0) {
                        fixture->setProperty("systemKey", 0);
                        const auto modifiers = Qt::KeyboardModifiers(fixture->property("modifiers").toInt());
                        using Delivery = QWindowSystemInterface::SynchronousDelivery;
                        QWindowSystemInterface::handleKeyEvent<Delivery>(window, QEvent::KeyPress, systemKey, modifiers);
                        QWindowSystemInterface::handleKeyEvent<Delivery>(window, QEvent::KeyRelease, systemKey, modifiers);
                    }
                    if (const auto clickValue = fixture->property("click"); clickValue.isValid()
                            && clickValue.toPointF().x() >= 0) {
                        const auto click = clickValue.toPointF();
                        fixture->setProperty("click", QPointF(-1, -1));
                        const auto global = window->mapToGlobal(click);
                        using Delivery = QWindowSystemInterface::SynchronousDelivery;
                        QWindowSystemInterface::handleMouseEvent<Delivery>(window, click, global,
                            Qt::NoButton, Qt::NoButton, QEvent::MouseMove);
                        QWindowSystemInterface::handleMouseEvent<Delivery>(window, click, global,
                            Qt::LeftButton, Qt::LeftButton, QEvent::MouseButtonPress);
                        QWindowSystemInterface::handleMouseEvent<Delivery>(window, click, global,
                            Qt::NoButton, Qt::LeftButton, QEvent::MouseButtonRelease);
                    }
                    QVariant state;
                    const bool advanced = QMetaObject::invokeMethod(fixture, "advance", Q_RETURN_ARG(QVariant, state));
                    if (advanced && state.toInt() == 0 && !m_qmlWarningOccurred && !deadline.hasExpired()) return;
                    timer->stop();
                    bool success = advanced && state.toInt() == 1 && !m_qmlWarningOccurred;
                    const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                    if (success && shot >= 0) {
                        auto image = window->grabWindow();
                        if (m_arguments.contains(u"--smoke-flight-controls"_s)
                                && fixture->property("flightState").toString() == u"badges"_s)
                            image = image.copy(0, 0, qRound(960 * image.devicePixelRatio()), qRound(218 * image.devicePixelRatio()));
                        success = shot + 1 < m_arguments.size() && QFileInfo(m_arguments.at(shot + 1)).isAbsolute()
                            && image.save(m_arguments.at(shot + 1));
                    }
                    if (!success) qCritical("Interactive shell acceptance failed");
                    m_application.exit(success ? EXIT_SUCCESS : EXIT_FAILURE);
                });
                timer->start();
                return;
            }
            if (ok && (m_arguments.contains(u"--smoke-collections"_s)
                       || m_arguments.contains(u"--smoke-queue-selector"_s)
                       || m_arguments.contains(u"--smoke-ownership"_s)
                       || m_arguments.contains(u"--smoke-game-labels"_s)
                       || m_arguments.contains(u"--smoke-push-invalidation"_s)
                       || m_arguments.contains(u"--smoke-bug-reports"_s)
                       || m_arguments.contains(u"--smoke-catalog-sync"_s)
                       || m_arguments.contains(u"--smoke-library-error"_s)
                       || m_arguments.contains(u"--smoke-color-format"_s)
                       || m_arguments.contains(u"--smoke-hdr-downgrade"_s)
                       || m_arguments.contains(u"--smoke-decoder-notice"_s)
                       || m_arguments.contains(u"--smoke-backend-availability"_s)
                       || m_arguments.contains(u"--smoke-queue-drops"_s)
                       || m_arguments.contains(u"--smoke-store-launch"_s))) {
                QTimer::singleShot(250, this, [this, window, fixture] {
                    if (m_arguments.contains(u"--smoke-ownership"_s)
                        || m_arguments.contains(u"--smoke-game-labels"_s)
                        || m_arguments.contains(u"--smoke-queue-selector"_s)
                        || m_arguments.contains(u"--smoke-library-error"_s)
                        || m_arguments.contains(u"--smoke-color-format"_s)
                        || m_arguments.contains(u"--smoke-hdr-downgrade"_s)
                        || m_arguments.contains(u"--smoke-decoder-notice"_s)
                        || m_arguments.contains(u"--smoke-store-launch"_s)
                        || m_arguments.contains(u"--smoke-push-invalidation"_s)
                        || m_arguments.contains(u"--smoke-bug-reports"_s)) {
                        QVariant verified;
                        if (!QMetaObject::invokeMethod(fixture, "verifyRendered", Q_RETURN_ARG(QVariant, verified),
                                Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) || !verified.toBool()) {
                            m_application.exit(EXIT_FAILURE);
                            return;
                        }
                    }
                    const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                    const bool saved = shot < 0 || (shot + 1 < m_arguments.size()
                        && window->grabWindow().save(m_arguments.at(shot + 1)));
                    m_application.exit(saved && !m_qmlWarningOccurred ? EXIT_SUCCESS : EXIT_FAILURE);
                });
            } else {
                const auto shot = m_arguments.indexOf(u"--screenshot"_s);
                const bool saved = shot < 0
                    || (shot + 1 < m_arguments.size()
                        && QFileInfo(m_arguments.at(shot + 1)).isAbsolute()
                        && window->grabWindow().save(m_arguments.at(shot + 1)));
                m_application.exit(ok && saved ? EXIT_SUCCESS : EXIT_FAILURE);
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
    } else if (m_arguments.contains(u"--smoke-account-shortcuts"_s)) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) return EXIT_FAILURE;
        QTimer::singleShot(200, this, [this, window] {
            const auto enabledSequences = [window] {
                QStringList enabled;
                for (int index = 1; index <= 9; ++index) {
                    auto *shortcut = window->findChild<QObject *>(u"accountSwitchShortcut%1"_s.arg(index));
                    if (!shortcut) return QStringList{u"missing"_s};
                    if (shortcut->property("enabled").toBool())
                        enabled.append(shortcut->property("sequence").toString());
                }
                return enabled;
            };
            const QStringList savedAccounts{u"Ctrl+1"_s, u"Ctrl+2"_s, u"Ctrl+3"_s, u"Ctrl+4"_s};
            auto *switcherShortcut = window->findChild<QObject *>(u"accountSwitcherShortcut"_s);
            auto *switcher = window->findChild<QObject *>(u"desktopAccountSwitcher"_s);
            bool passed = !m_qmlWarningOccurred && switcherShortcut && switcher
                && enabledSequences() == savedAccounts && switcherShortcut->property("enabled").toBool()
                && QMetaObject::invokeMethod(switcherShortcut, "activated", Qt::DirectConnection)
                && switcher->property("opened").toBool();
            QKeyEvent press(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &press);
            QKeyEvent release(QEvent::KeyRelease, Qt::Key_Escape, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &release);
            passed = passed && !switcher->property("opened").toBool();
            m_controller.navigate(u"settings-shortcuts"_s);
            passed = passed && enabledSequences().isEmpty() && !switcherShortcut->property("enabled").toBool();
            m_controller.navigate(u"home"_s);
            m_controller.showOverlay(u"account-switch-confirm"_s);
            passed = passed && enabledSequences().isEmpty() && !switcherShortcut->property("enabled").toBool();
            m_controller.showOverlay(QString());
            passed = passed && enabledSequences() == savedAccounts && !m_qmlWarningOccurred;
            if (!passed) qCritical("Account shortcuts escaped their safe routes or the switcher did not open and close");
            m_application.exit(passed ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    } else if (m_smokeFullscreenStatsShortcut) {
        auto *window = m_engine.rootObjects().isEmpty()
            ? nullptr : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        if (!window) return EXIT_FAILURE;
        auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
        if (!store) return EXIT_FAILURE;
        store->setProperty("streamerStartRequestId", u"stats-shortcut-fixture"_s);
        store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"stats-shortcut-fixture"_s}});
        store->setProperty("streamState", u"streaming"_s);
        store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s}, {u"firstFrameLatencyMs"_s, 37}});
        const bool configuredShortcut = m_arguments.contains(u"--smoke-configured-stats-shortcut"_s);
        if (configuredShortcut) {
            auto settings = store->property("settings").toMap();
            settings.insert(u"shortcutToggleStats"_s, u"F3"_s);
            store->setProperty("settings", settings);
        }
        window->showFullScreen();
        window->requestActivate();
        auto *statsShortcut = window->findChild<QObject *>(u"configuredStreamStatsShortcut"_s);
        auto *copyShortcut = window->findChild<QObject *>(u"streamStatsCopyShortcut"_s);
        auto *streamSurface = window->findChild<QObject *>(u"streamSurfaceHost"_s);
        if (!statsShortcut || !copyShortcut || !streamSurface) return EXIT_FAILURE;
        if (statsShortcut->property("sequence").toString()
                != (configuredShortcut ? u"F3"_s : u"Ctrl+N"_s)) return EXIT_FAILURE;
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
        const auto surfaceIndex = m_arguments.indexOf(u"--smoke-update-surface"_s);
        if (surfaceIndex >= 0 && surfaceIndex + 1 < m_arguments.size()) {
            const auto surface = m_arguments.at(surfaceIndex + 1);
            QTimer::singleShot(1'800, this, [this, screenshotPath, surface] {
                auto *window = m_engine.rootObjects().isEmpty()
                    ? nullptr
                    : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
                if (window && m_arguments.contains(u"--smoke-update-fullscreen"_s))
                    window->showFullScreen();
                presentUpdateSurface(m_engine, window, surface);
                QTimer::singleShot(400, this, [this, screenshotPath, surface] {
                    auto *window = m_engine.rootObjects().isEmpty()
                        ? nullptr
                        : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
                    presentUpdateSurface(m_engine, window, surface);
                    const bool openPanel = m_arguments.contains(u"--smoke-update-panel"_s);
                    if (openPanel && window) {
                        if (auto *panel = window->findChild<QObject *>(u"desktopUpdatePanel"_s))
                            QMetaObject::invokeMethod(panel, "open");
                    }
                    QTimer::singleShot(350, this, [this, screenshotPath, surface, openPanel] {
                        if (m_engine.rootObjects().isEmpty() || m_qmlWarningOccurred) {
                            m_application.exit(EXIT_FAILURE);
                            return;
                        }
                        auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
                        // The desktop chip only appears when there is an update action to take.
                        if (window && window->property("desktopSurfaceActive").toBool()) {
                            auto *chip = window->findChild<QQuickItem *>(u"desktopUpdateChip"_s);
                            auto *panel = window->findChild<QObject *>(u"desktopUpdatePanel"_s);
                            const bool chipExpected = surface != u"uptodate"_s && surface != u"other"_s;
                            // A staged older package keeps its install action beside Download.
                            auto *staged = window->findChild<QQuickItem *>(u"updatesPageInstallStaged"_s);
                            const bool stagedVisible = staged && staged->isVisible();
                            const bool stagedExpected = surface == u"staged"_s
                                && m_controller.route() == u"settings-updates"_s;
                            if (!chip || chip->isVisible() != chipExpected || stagedVisible != stagedExpected
                                    || (openPanel && (!panel || !panel->property("opened").toBool()))) {
                                qCritical("Update chip or panel did not match the presented updater state");
                                m_application.exit(EXIT_FAILURE);
                                return;
                            }
                        }
                        const auto saved = window && window->grabWindow().save(screenshotPath);
                        m_application.exit(saved ? EXIT_SUCCESS : EXIT_FAILURE);
                    });
                });
            });
        } else {
        QTimer::singleShot(1'000, this,
                           [this, screenshotPath] {
            if (m_engine.rootObjects().isEmpty() || m_qmlWarningOccurred) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
            if (m_arguments.contains(u"--smoke-account-chrome-check"_s) && !accountChromeFits(window)) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            if (m_arguments.contains(u"--smoke-settings-rail-check"_s) && !settingsRailFits(window)) {
                m_application.exit(EXIT_FAILURE);
                return;
            }
            const auto saved = window && window->grabWindow().save(screenshotPath);
            m_application.exit(saved ? EXIT_SUCCESS : EXIT_FAILURE);
        });
        }
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
            const bool updatesRoute = m_controller.route() == u"updates"_s
                || m_controller.route() == u"settings-updates"_s;
            const bool correctRoute = !updatesRoute || window->findChild<QObject *>(u"desktopUpdatesSettings"_s);
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
