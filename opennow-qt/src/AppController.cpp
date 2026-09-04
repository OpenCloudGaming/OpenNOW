#include "AppController.h"

#include <algorithm>
#include <QCoreApplication>
#include <QClipboard>
#include <QDesktopServices>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QFile>
#include <QGuiApplication>
#include <QPixmap>
#include <QRegularExpression>
#include <QSaveFile>
#include <QScreen>
#include <QSettings>
#include <QStandardPaths>
#include <QUrl>
#include <QUrlQuery>

#include <utility>

using namespace Qt::StringLiterals;

AppController::AppController(QObject *parent)
    : QObject(parent)
    , m_route(u"home"_s)
{
}

QString AppController::route() const
{
    return m_route;
}

QString AppController::backRoute() const
{
    return m_routeHistory.isEmpty() ? QString{} : m_routeHistory.constLast();
}

QString AppController::overlay() const
{
    return m_overlay;
}

bool AppController::reducedMotion() const
{
    return m_reducedMotion;
}

int AppController::controllerCount() const
{
    return m_controllerCount;
}

QString AppController::inputMode() const
{
    return m_inputMode;
}

bool AppController::navigate(const QString &route)
{
    if (!routes().contains(route) || route == m_route) {
        return false;
    }

    if (!m_overlay.isEmpty()) {
        applyOverlay({});
    }

    m_routeHistory.push_back(m_route);
    m_route = route;
    emit routeChanged();
    return true;
}

bool AppController::navigateFromLastPrimary(const QString &route)
{
    if (!routes().contains(route) || route == m_route) {
        return false;
    }

    if (!m_overlay.isEmpty()) {
        applyOverlay({});
    }

    auto routeStack = m_routeHistory;
    routeStack.push_back(m_route);
    qsizetype primaryIndex = -1;
    for (qsizetype index = routeStack.size() - 1; index >= 0; --index) {
        if (primaryRoutes().contains(routeStack.at(index))) {
            primaryIndex = index;
            break;
        }
    }

    m_routeHistory = primaryIndex >= 0
        ? routeStack.mid(0, primaryIndex + 1)
        : QVector<QString>{u"home"_s};
    m_route = route;
    emit routeChanged();
    return true;
}

bool AppController::showOverlay(const QString &overlay)
{
    if ((!overlay.isEmpty() && !overlays().contains(overlay)) || overlay == m_overlay) {
        return false;
    }

    return applyOverlay(overlay);
}

bool AppController::goBack()
{
    if (!m_overlay.isEmpty()) {
        return applyOverlay({});
    }

    if (m_routeHistory.isEmpty()) {
        return false;
    }

    m_route = m_routeHistory.takeLast();
    emit routeChanged();
    return true;
}

void AppController::setOverlayTransitionGuard(std::function<bool(bool)> guard)
{
    m_overlayTransitionGuard = std::move(guard);
}

bool AppController::applyOverlay(const QString &overlay)
{
    if (m_overlayTransitionGuard && !m_overlayTransitionGuard(!overlay.isEmpty())) {
        return false;
    }
    m_overlay = overlay;
    emit overlayChanged();
    return true;
}

bool AppController::cyclePrimaryRoute(int direction)
{
    const auto &items = primaryRoutes();
    auto index = items.indexOf(m_route);
    if (index < 0) {
        index = 0;
    } else {
        const auto delta = direction < 0 ? -1 : 1;
        index = (index + delta + items.size()) % items.size();
    }
    return navigate(items.at(index));
}

bool AppController::cycleGuidePage(int direction)
{
    static const QStringList pages{
        u"guide-session"_s,
        u"guide-controls"_s,
        u"guide-media"_s,
        u"guide-shortcuts"_s,
    };
    auto index = pages.indexOf(m_overlay);
    if (index < 0) return false;
    const auto delta = direction < 0 ? -1 : 1;
    index = (index + delta + pages.size()) % pages.size();
    return showOverlay(pages.at(index));
}

QStringList AppController::supportedRoutes() const
{
    return routes();
}

bool AppController::openExternalUrl(const QString &value) const
{
    const QUrl url(value);
    if (!url.isValid() || (url.scheme() != u"https"_s && url.scheme() != u"http"_s)) {
        return false;
    }
    return QDesktopServices::openUrl(url);
}

bool AppController::openLocalPath(const QString &value, bool reveal) const
{
    const QFileInfo info(value);
    if (!info.isAbsolute() || !info.exists()) {
        return false;
    }
    const auto target = reveal && info.isFile() ? info.absolutePath() : info.absoluteFilePath();
    return QDesktopServices::openUrl(QUrl::fromLocalFile(target));
}

QString AppController::readClipboardText() const
{
    const auto *clipboard = QGuiApplication::clipboard();
    if (!clipboard) return {};
    auto text = clipboard->text(QClipboard::Clipboard).left(65'536);
    text.remove(QChar::Null);
    return text;
}

bool AppController::writeClipboardText(const QString &value) const
{
    auto *clipboard = QGuiApplication::clipboard();
    if (!clipboard) return false;
    auto text = value.left(65'536);
    text.remove(QChar::Null);
    if (text.isEmpty()) return false;
    clipboard->setText(text, QClipboard::Clipboard);
    return clipboard->text(QClipboard::Clipboard) == text;
}

QString AppController::normalizeNativeStreamerExecutable(const QString &urlOrPath) const
{
    const QUrl candidate(urlOrPath);
    const auto path = candidate.isLocalFile() ? candidate.toLocalFile() : urlOrPath;
    const QFileInfo info(path);
    if (!info.isAbsolute() || !info.isFile()) return {};
#ifdef Q_OS_WIN
    if (info.suffix().compare(u"exe"_s, Qt::CaseInsensitive) != 0) return {};
#else
    if (!info.isExecutable()) return {};
#endif
    return info.canonicalFilePath();
}

bool AppController::copyScreenshotTo(const QString &sourcePath,
                                     const QString &destinationUrlOrPath) const
{
    const QFileInfo source(sourcePath);
    if (!source.isAbsolute() || !source.isFile() || source.size() < 0
            || source.size() > 512LL * 1024 * 1024) {
        return false;
    }
    const auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    if (pictures.isEmpty()) return false;
    QDir screenshots(QDir(pictures).filePath(u"OpenNOW/Screenshots"_s));
    if (!screenshots.exists()
            || QDir::cleanPath(source.absolutePath()) != QDir::cleanPath(screenshots.absolutePath())) {
        return false;
    }
    const auto sourceSuffix = source.suffix().toLower();
    if (!QStringList{u"png"_s, u"jpg"_s, u"jpeg"_s, u"webp"_s}.contains(sourceSuffix)) {
        return false;
    }
    const QUrl destinationUrl(destinationUrlOrPath);
    const auto destinationPath = destinationUrl.isLocalFile()
        ? destinationUrl.toLocalFile() : destinationUrlOrPath;
    const QFileInfo destination(destinationPath);
    if (!destination.isAbsolute() || destination.absoluteFilePath() == source.absoluteFilePath()) {
        return false;
    }
    QFile input(source.absoluteFilePath());
    QSaveFile output(destination.absoluteFilePath());
    if (!input.open(QIODevice::ReadOnly) || !output.open(QIODevice::WriteOnly)) return false;
    while (!input.atEnd()) {
        const auto chunk = input.read(1024 * 1024);
        if (chunk.isEmpty() && input.error() != QFile::NoError) {
            output.cancelWriting();
            return false;
        }
        if (output.write(chunk) != chunk.size()) {
            output.cancelWriting();
            return false;
        }
    }
    return output.commit();
}

bool AppController::openThemeDirectory() const
{
    const auto base = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    if (base.isEmpty()) return false;
    QDir directory(base);
    if (!directory.mkpath(u"themes"_s)) return false;
    return QDesktopServices::openUrl(QUrl::fromLocalFile(directory.filePath(u"themes"_s)));
}

QString AppController::captureScreenRegion(int x, int y, int width, int height,
                                           const QString &gameTitle) const
{
    if (width < 64 || height < 64 || width > 16384 || height > 16384) return {};
    const QRect requested(x, y, width, height);
    const QPoint center = requested.center();
    auto *screen = QGuiApplication::screenAt(center);
    if (!screen) screen = QGuiApplication::primaryScreen();
    if (!screen) return {};
    const auto captured = requested.intersected(screen->geometry());
    if (captured.width() < 64 || captured.height() < 64) return {};
    const auto local = captured.translated(-screen->geometry().topLeft());
    const auto image = screen->grabWindow(0, local.x(), local.y(), local.width(), local.height());
    if (image.isNull()) return {};

    auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    if (pictures.isEmpty()) return {};
    QDir directory(pictures);
    if (!directory.mkpath(u"OpenNOW/Screenshots"_s)
            || !directory.cd(u"OpenNOW"_s)
            || !directory.cd(u"Screenshots"_s)) {
        return {};
    }
    auto safeTitle = gameTitle.trimmed();
    safeTitle.replace(QRegularExpression(uR"([^A-Za-z0-9._-]+)"_s), u"-"_s);
    safeTitle = safeTitle.left(72).trimmed();
    if (safeTitle.isEmpty()) safeTitle = u"OpenNOW"_s;
    const auto stamp = QDateTime::currentDateTimeUtc().toString(u"yyyyMMdd-HHmmss-zzz"_s);
    const auto path = directory.filePath(safeTitle + u"-"_s + stamp + u".png"_s);
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || !image.save(&file, "PNG") || !file.commit()) {
        file.cancelWriting();
        return {};
    }
    return path;
}

bool AppController::captureScreenRegionTo(int x, int y, int width, int height,
                                          const QString &outputPath) const
{
    if (width < 64 || height < 64 || width > 16384 || height > 16384) return false;
    const QFileInfo output(outputPath);
    if (!output.isAbsolute() || output.suffix().compare(u"jpg"_s, Qt::CaseInsensitive) != 0
            || !output.completeBaseName().endsWith(u"-thumb"_s)) {
        return false;
    }
    const auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    if (pictures.isEmpty()) return false;
    QDir directory(pictures);
    if (!directory.mkpath(u"OpenNOW/Recordings"_s)
            || !directory.cd(u"OpenNOW"_s)
            || !directory.cd(u"Recordings"_s)
            || QDir::cleanPath(output.absolutePath()) != QDir::cleanPath(directory.absolutePath())) {
        return false;
    }

    const QRect requested(x, y, width, height);
    auto *screen = QGuiApplication::screenAt(requested.center());
    if (!screen) screen = QGuiApplication::primaryScreen();
    if (!screen) return false;
    const auto captured = requested.intersected(screen->geometry());
    if (captured.width() < 64 || captured.height() < 64) return false;
    const auto local = captured.translated(-screen->geometry().topLeft());
    const auto image = screen->grabWindow(0, local.x(), local.y(), local.width(), local.height());
    if (image.isNull()) return false;

    QSaveFile file(output.absoluteFilePath());
    if (!file.open(QIODevice::WriteOnly) || !image.save(&file, "JPG", 88) || !file.commit()) {
        file.cancelWriting();
        return false;
    }
    return true;
}

bool AppController::ensureDirectLaunchAssociation() const
{
#ifdef Q_OS_WIN
    const auto executable = QDir::toNativeSeparators(QCoreApplication::applicationFilePath());
    if (executable.isEmpty()) return false;
    QSettings protocol(u"HKEY_CURRENT_USER\\Software\\Classes\\opennow"_s,
                       QSettings::NativeFormat);
    protocol.setValue(u"."_s, u"URL:OpenNOW Protocol"_s);
    protocol.setValue(u"URL Protocol"_s, QString());
    protocol.setValue(u"DefaultIcon/."_s, u"\"%1\",0"_s.arg(executable));
    protocol.setValue(u"shell/open/command/."_s, u"\"%1\" \"%2\""_s.arg(executable, u"%1"_s));
    protocol.sync();
    return protocol.status() == QSettings::NoError;
#else
    return true;
#endif
}

void AppController::activateWindow()
{
    emit activationRequested();
}

void AppController::quitApplication()
{
    QCoreApplication::quit();
}

bool AppController::handleArguments(const QStringList &arguments)
{
    static const QStringList appIdFlags{u"--launch-app-id"_s, u"--app-id"_s};
    static const QStringList titleFlags{
        u"--launch-title"_s,
        u"--launch-game"_s,
        u"--game-title"_s,
        u"--game"_s,
    };
    QString appId;
    QString title;
    for (qsizetype index = 0; index < arguments.size(); ++index) {
        const auto argument = arguments.at(index);
        const QUrl directLaunchUrl(argument);
        if (directLaunchUrl.isValid() && directLaunchUrl.scheme() == u"opennow"_s
                && directLaunchUrl.host() == u"launch"_s) {
            const QUrlQuery query(directLaunchUrl);
            auto urlAppId = query.queryItemValue(u"appId"_s).trimmed();
            if (urlAppId.isEmpty()) {
                auto path = directLaunchUrl.path();
                if (path.startsWith(u'/')) path.removeFirst();
                urlAppId = path.section(u'/', 0, 0).trimmed();
            }
            bool numeric = false;
            urlAppId.toULongLong(&numeric);
            if (numeric) appId = urlAppId;
            const auto urlTitle = query.queryItemValue(u"title"_s).trimmed();
            if (!urlTitle.isEmpty()) title = urlTitle;
            continue;
        }
        const auto separator = argument.indexOf(u'=');
        const auto flag = separator < 0 ? argument : argument.first(separator);
        auto value = separator < 0
            ? (index + 1 < arguments.size() && !arguments.at(index + 1).startsWith(u"--"_s)
                   ? arguments.at(index + 1)
                   : QString{})
            : argument.sliced(separator + 1);
        value = value.trimmed();
        if (value.size() >= 2
                && ((value.front() == u'"' && value.back() == u'"')
                    || (value.front() == u'\'' && value.back() == u'\''))) {
            value = value.sliced(1, value.size() - 2).trimmed();
        }
        if (appIdFlags.contains(flag)) {
            bool numeric = false;
            value.toULongLong(&numeric);
            if (numeric) appId = value;
        } else if (titleFlags.contains(flag) && !value.isEmpty()) {
            title = value;
        }
    }
    if (appId.isEmpty() && title.isEmpty()) return false;
    emit directLaunchRequested(appId, title);
    return true;
}

void AppController::setReducedMotion(bool reducedMotion)
{
    if (m_reducedMotion == reducedMotion) {
        return;
    }
    m_reducedMotion = reducedMotion;
    emit reducedMotionChanged();
}

void AppController::setControllerCount(int count)
{
    const auto safeCount = std::max(0, count);
    if (m_controllerCount == safeCount) {
        return;
    }
    m_controllerCount = safeCount;
    emit controllerCountChanged();
}

void AppController::setInputMode(const QString &mode)
{
    if (mode != u"controller"_s && mode != u"keyboard"_s && mode != u"pointer"_s) return;
    if (m_inputMode == mode) return;
    m_inputMode = mode;
    emit inputModeChanged();
}

const QStringList &AppController::routes()
{
    static const QStringList value{
        u"home"_s,
        u"library"_s,
        u"store"_s,
        u"friends"_s,
        u"theme-store"_s,
        u"controllers"_s,
        u"settings"_s,
        u"settings-account"_s,
        u"settings-streaming"_s,
        u"settings-video"_s,
        u"settings-video-dropdown"_s,
        u"settings-input"_s,
        u"settings-network"_s,
        u"settings-themes"_s,
        u"settings-advanced"_s,
        u"settings-advanced-dropdown"_s,
        u"game-detail"_s,
        u"game-detail-platform-dropdown"_s,
        u"sign-in"_s,
        u"joining"_s,
        u"inserting"_s,
        u"stream"_s,
        u"accounts"_s,
        u"profile-pin"_s,
        u"game-accounts"_s,
        u"persistent-storage"_s,
        u"media"_s,
        u"diagnostics"_s,
        u"updates"_s,
        u"feedback"_s,
    };
    return value;
}

const QStringList &AppController::primaryRoutes()
{
    static const QStringList value{
        u"home"_s,
        u"library"_s,
        u"store"_s,
        u"friends"_s,
        u"controllers"_s,
        u"settings"_s,
    };
    return value;
}

const QStringList &AppController::overlays()
{
    static const QStringList value{
        u"friends"_s,
        u"friend-actions"_s,
        u"quick-settings"_s,
        u"session-conflict"_s,
        u"queue-ad"_s,
        u"desktop-stream-menu"_s,
        u"desktop-stream-exit-confirm"_s,
        u"desktop-stream-stats"_s,
        u"desktop-stream-stats-expanded"_s,
        u"stream-stats"_s,
        u"stream-stats-expanded"_s,
        u"guide-session"_s,
        u"guide-controls"_s,
        u"guide-media"_s,
        u"guide-shortcuts"_s,
    };
    return value;
}
