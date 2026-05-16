#include "OpnQtCatalogView.h"

#include <QtCore/QDateTime>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtGui/QKeyEvent>
#include <QtGui/QLinearGradient>
#include <QtGui/QMouseEvent>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QRadialGradient>
#include <QtGui/QWheelEvent>
#include <QtGui/QCursor>
#include <QtWidgets/QApplication>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkReply>
#include <QtNetwork/QNetworkRequest>

#include <algorithm>
#include <cmath>
#include <optional>

namespace OpnQt {
namespace {

const QColor kBackground(0x05, 0x07, 0x0A);
const QColor kPanel(0x14, 0x18, 0x20);
const QColor kPanelRaised(0x1D, 0x22, 0x2D);
const QColor kTextPrimary(0xF5, 0xF7, 0xFB);
const QColor kTextSecondary(0xA9, 0xB1, 0xC1);
const QColor kTextMuted(0x72, 0x7B, 0x8D);
const QColor kFocusBlue(0x8D, 0xD7, 0xFF);
const QColor kFocusGreen(0x7C, 0xF1, 0xB1);

QColor withAlpha(QColor color, int alpha) {
    color.setAlpha(alpha);
    return color;
}

QFont uiFont(int pointSize, QFont::Weight weight) {
    QFont font(QStringLiteral("Avenir Next"));
    font.setPointSize(pointSize);
    font.setWeight(weight);
    font.setStyleStrategy(QFont::PreferAntialias);
    return font;
}

quint32 stableHash(const QString &value) {
    quint32 hash = 2166136261u;
    const QByteArray bytes = value.toUtf8();
    for (char byte : bytes) {
        hash ^= static_cast<quint8>(byte);
        hash *= 16777619u;
    }
    return hash;
}

QColor colorForTitle(const QString &title) {
    const quint32 hash = stableHash(title);
    const int hue = static_cast<int>(hash % 360u);
    const int saturation = 120 + static_cast<int>((hash >> 9) % 70u);
    const int value = 155 + static_cast<int>((hash >> 17) % 75u);
    return QColor::fromHsv(hue, saturation, value);
}

QPainterPath roundedRect(const QRectF &rect, qreal radius) {
    QPainterPath path;
    path.addRoundedRect(rect, radius, radius);
    return path;
}

QList<CatalogGame> randomizedPreviewGames(const CatalogFolder &folder, int folderIndex) {
    QList<CatalogGame> games = folder.games;
    if (games.size() <= 1) return games;
    const int offset = static_cast<int>(stableHash(folder.title + QString::number(folderIndex)) % static_cast<quint32>(games.size()));
    std::rotate(games.begin(), games.begin() + offset, games.end());
    return games;
}

QString currentTimeText() {
    return QDateTime::currentDateTime().toString(QStringLiteral("h:mm AP"));
}

QSizeF fittedImageSize(const QSize &imageSize, qreal maxWidth, qreal maxHeight) {
    if (imageSize.isEmpty() || maxWidth <= 0.0 || maxHeight <= 0.0) return QSizeF(maxWidth, maxHeight);
    const qreal imageRatio = static_cast<qreal>(imageSize.width()) / static_cast<qreal>(imageSize.height());
    qreal width = maxWidth;
    qreal height = width / imageRatio;
    if (height > maxHeight) {
        height = maxHeight;
        width = height * imageRatio;
    }
    return QSizeF(width, height);
}

struct KeyMapping {
    quint16 vk = 0;
    quint16 scancode = 0;
};

std::optional<KeyMapping> streamKeyMapping(int key) {
    switch (key) {
        case Qt::Key_A: return KeyMapping{0x41, 0x001e};
        case Qt::Key_B: return KeyMapping{0x42, 0x0030};
        case Qt::Key_C: return KeyMapping{0x43, 0x002e};
        case Qt::Key_D: return KeyMapping{0x44, 0x0020};
        case Qt::Key_E: return KeyMapping{0x45, 0x0012};
        case Qt::Key_F: return KeyMapping{0x46, 0x0021};
        case Qt::Key_G: return KeyMapping{0x47, 0x0022};
        case Qt::Key_H: return KeyMapping{0x48, 0x0023};
        case Qt::Key_I: return KeyMapping{0x49, 0x0017};
        case Qt::Key_J: return KeyMapping{0x4a, 0x0024};
        case Qt::Key_K: return KeyMapping{0x4b, 0x0025};
        case Qt::Key_L: return KeyMapping{0x4c, 0x0026};
        case Qt::Key_M: return KeyMapping{0x4d, 0x0032};
        case Qt::Key_N: return KeyMapping{0x4e, 0x0031};
        case Qt::Key_O: return KeyMapping{0x4f, 0x0018};
        case Qt::Key_P: return KeyMapping{0x50, 0x0019};
        case Qt::Key_Q: return KeyMapping{0x51, 0x0010};
        case Qt::Key_R: return KeyMapping{0x52, 0x0013};
        case Qt::Key_S: return KeyMapping{0x53, 0x001f};
        case Qt::Key_T: return KeyMapping{0x54, 0x0014};
        case Qt::Key_U: return KeyMapping{0x55, 0x0016};
        case Qt::Key_V: return KeyMapping{0x56, 0x002f};
        case Qt::Key_W: return KeyMapping{0x57, 0x0011};
        case Qt::Key_X: return KeyMapping{0x58, 0x002d};
        case Qt::Key_Y: return KeyMapping{0x59, 0x0015};
        case Qt::Key_Z: return KeyMapping{0x5a, 0x002c};
        default: break;
    }
    if (key >= Qt::Key_0 && key <= Qt::Key_9) return KeyMapping{static_cast<quint16>(0x30 + key - Qt::Key_0), static_cast<quint16>(key == Qt::Key_0 ? 0x000b : 0x0001 + key - Qt::Key_0)};
    switch (key) {
        case Qt::Key_Escape: return KeyMapping{0x1b, 0x0001};
        case Qt::Key_Backspace: return KeyMapping{0x08, 0x000e};
        case Qt::Key_Tab: return KeyMapping{0x09, 0x000f};
        case Qt::Key_Return:
        case Qt::Key_Enter: return KeyMapping{0x0d, 0x001c};
        case Qt::Key_Space: return KeyMapping{0x20, 0x0039};
        case Qt::Key_Left: return KeyMapping{0x25, 0xe04b};
        case Qt::Key_Up: return KeyMapping{0x26, 0xe048};
        case Qt::Key_Right: return KeyMapping{0x27, 0xe04d};
        case Qt::Key_Down: return KeyMapping{0x28, 0xe050};
        case Qt::Key_Shift: return KeyMapping{0xa0, 0x002a};
        case Qt::Key_Control: return KeyMapping{0xa2, 0x001d};
        case Qt::Key_Alt: return KeyMapping{0xa4, 0x0038};
        default: return std::nullopt;
    }
}

quint16 streamModifiers(Qt::KeyboardModifiers modifiers) {
    quint16 out = 0;
    if (modifiers.testFlag(Qt::ShiftModifier)) out |= 0x0001;
    if (modifiers.testFlag(Qt::ControlModifier)) out |= 0x0002;
    if (modifiers.testFlag(Qt::AltModifier)) out |= 0x0004;
    if (modifiers.testFlag(Qt::MetaModifier)) out |= 0x0008;
    return out;
}

qint16 clampI16(qreal value) {
    return static_cast<qint16>(std::clamp<int>(static_cast<int>(std::round(value)), -32768, 32767));
}

quint8 streamMouseButton(Qt::MouseButton button) {
    switch (button) {
        case Qt::LeftButton: return 1;
        case Qt::MiddleButton: return 2;
        case Qt::RightButton: return 3;
        case Qt::BackButton: return 4;
        case Qt::ForwardButton: return 5;
        default: return 0;
    }
}

qreal folderRailY(int height) {
    return std::max<qreal>(118.0, height * 0.15);
}

qreal gameColumnFirstY(int height) {
    return std::max<qreal>(folderRailY(height) + 260.0, height * 0.39);
}

} // namespace

CatalogView::CatalogView(QWidget *parent) : QWidget(parent), m_network(new QNetworkAccessManager(this)) {
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);

    m_pointerUnlockTimer.setSingleShot(true);
    m_pointerUnlockTimer.setInterval(500);
    m_pointerUnlockTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_pointerUnlockTimer, &QTimer::timeout, this, [this]() {
        if (!m_pointerLocked) return;
        releasePointerLock();
        m_statusText = QStringLiteral("Pointer lock disabled");
        update();
    });

    auto *animationTimer = new QTimer(this);
    animationTimer->setTimerType(Qt::CoarseTimer);
    connect(animationTimer, &QTimer::timeout, this, [this]() {
        m_backgroundPhase = std::fmod(m_backgroundPhase + 0.0035, 1.0);
        update();
    });
    animationTimer->start(50);
}

void CatalogView::setPreviewData() {
    m_loading = false;
    m_error = false;
    m_overlayMessage.clear();
    m_selectedFolder = 0;
    m_selectedGame = 0;
    populatePreviewData();
    update();
}

void CatalogView::setAccountName(const QString &name) {
    m_accountName = name.trimmed().isEmpty() ? QStringLiteral("Player") : name.trimmed();
    update();
}

void CatalogView::setGames(const QList<CatalogGame> &games) {
    m_loading = false;
    m_error = false;
    m_overlayMessage.clear();
    m_selectedFolder = 0;
    m_selectedGame = 0;

    if (games.isEmpty()) {
        m_folders = {CatalogFolder{QStringLiteral("Library"), QStringLiteral("No games found"), {}}};
        m_statusText = QStringLiteral("Library is empty");
        update();
        return;
    }

    auto uniqueByTitle = [](const QList<CatalogGame> &source) {
        QList<CatalogGame> out;
        QSet<QString> seen;
        for (const CatalogGame &game : source) {
            const QString key = game.title.toCaseFolded();
            if (key.isEmpty() || seen.contains(key)) continue;
            seen.insert(key);
            out.append(game);
        }
        return out;
    };

    QList<CatalogFolder> folders;
    folders.append(CatalogFolder{QStringLiteral("All Games"), QStringLiteral("%1 cloud titles").arg(games.size()), uniqueByTitle(games)});

    QHash<QString, QList<CatalogGame>> byStore;
    QHash<QString, QList<CatalogGame>> byGenre;
    QList<CatalogGame> rtxGames;
    for (const CatalogGame &game : games) {
        if (!game.store.isEmpty()) byStore[game.store].append(game);
        for (const QString &tag : game.tags) {
            if (tag.contains(QStringLiteral("RTX"), Qt::CaseInsensitive)) rtxGames.append(game);
            if (tag.size() <= 22 && !tag.contains(QStringLiteral("Controller"), Qt::CaseInsensitive)) byGenre[tag].append(game);
        }
    }

    QStringList storeKeys = byStore.keys();
    storeKeys.sort(Qt::CaseInsensitive);
    for (const QString &store : storeKeys) {
        if (byStore[store].size() >= 1) folders.append(CatalogFolder{store, QStringLiteral("Linked store"), uniqueByTitle(byStore[store])});
    }

    QStringList genreKeys = byGenre.keys();
    std::sort(genreKeys.begin(), genreKeys.end(), [&byGenre](const QString &lhs, const QString &rhs) {
        return byGenre[lhs].size() > byGenre[rhs].size();
    });
    for (const QString &genre : genreKeys.mid(0, 5)) {
        if (byGenre[genre].size() >= 2) folders.append(CatalogFolder{genre, QStringLiteral("Genre folder"), uniqueByTitle(byGenre[genre])});
    }
    if (!rtxGames.isEmpty()) folders.append(CatalogFolder{QStringLiteral("RTX Showcase"), QStringLiteral("Visual upgrades"), uniqueByTitle(rtxGames)});

    m_folders = folders;
    m_statusText = QStringLiteral("Loaded %1 library games").arg(games.size());
    update();
}

void CatalogView::setLoading(const QString &message) {
    m_loading = true;
    m_error = false;
    m_overlayTitle = QStringLiteral("Loading Library");
    m_overlayMessage = message;
    update();
}

void CatalogView::setError(const QString &message) {
    setError(QStringLiteral("Library Load Failed"), message);
}

void CatalogView::setError(const QString &title, const QString &message) {
    m_loading = false;
    m_error = true;
    m_overlayTitle = title;
    m_overlayMessage = message;
    update();
}

void CatalogView::setStreamFrame(const QImage &frame) {
    if (frame.isNull()) return;
    const bool enteringStream = m_streamFrame.isNull();
    m_streamFrame = frame.copy();
    m_loading = false;
    m_error = false;
    m_overlayTitle.clear();
    m_overlayMessage.clear();
        m_statusText = QStringLiteral("Streaming");
    if (enteringStream) {
        setFocus(Qt::OtherFocusReason);
        grabKeyboard();
        m_lastMousePosition = mapFromGlobal(QCursor::pos());
        m_statusText = QStringLiteral("Streaming: click to lock pointer, Esc to release");
        qInfo() << "[Input] Stream keyboard capture active";
    }
    update();
}

void CatalogView::populatePreviewData() {
    const auto game = [](const QString &title, const QString &store, const QString &subtitle, QStringList tags) {
        CatalogGame catalogGame;
        catalogGame.title = title;
        catalogGame.store = store;
        catalogGame.subtitle = subtitle;
        catalogGame.tags = tags;
        catalogGame.accent = colorForTitle(title);
        catalogGame.internalTitle = title;
        catalogGame.selectedStore = store;
        return catalogGame;
    };

    const QList<CatalogGame> allGames = {
        game(QStringLiteral("Cyberpunk 2077"), QStringLiteral("Steam"), QStringLiteral("Ray-traced neon RPG"), {QStringLiteral("RPG"), QStringLiteral("RTX"), QStringLiteral("Controller")}),
        game(QStringLiteral("Baldur's Gate 3"), QStringLiteral("Steam"), QStringLiteral("Party tactics and choices"), {QStringLiteral("RPG"), QStringLiteral("Co-op")}),
        game(QStringLiteral("Apex Legends"), QStringLiteral("EA"), QStringLiteral("Squad battle royale"), {QStringLiteral("Shooter"), QStringLiteral("Online")}),
        game(QStringLiteral("Fortnite"), QStringLiteral("Epic"), QStringLiteral("Zero build and creative"), {QStringLiteral("Shooter"), QStringLiteral("Free")}),
        game(QStringLiteral("Forza Horizon 5"), QStringLiteral("Xbox"), QStringLiteral("Open-world racing"), {QStringLiteral("Racing"), QStringLiteral("HDR")}),
        game(QStringLiteral("Control Ultimate"), QStringLiteral("Epic"), QStringLiteral("Paranatural action"), {QStringLiteral("Action"), QStringLiteral("RTX")}),
        game(QStringLiteral("No Man's Sky"), QStringLiteral("Steam"), QStringLiteral("Endless space survival"), {QStringLiteral("Explore"), QStringLiteral("Co-op")}),
        game(QStringLiteral("Destiny 2"), QStringLiteral("Steam"), QStringLiteral("Shared-world shooter"), {QStringLiteral("Shooter"), QStringLiteral("Online")}),
        game(QStringLiteral("Alan Wake 2"), QStringLiteral("Epic"), QStringLiteral("Survival horror"), {QStringLiteral("Horror"), QStringLiteral("RTX")}),
        game(QStringLiteral("Warframe"), QStringLiteral("Steam"), QStringLiteral("Ninja sci-fi action"), {QStringLiteral("Action"), QStringLiteral("Free")}),
        game(QStringLiteral("Dota 2"), QStringLiteral("Steam"), QStringLiteral("Competitive strategy"), {QStringLiteral("MOBA"), QStringLiteral("Online")}),
        game(QStringLiteral("Starfield"), QStringLiteral("Xbox"), QStringLiteral("Space exploration RPG"), {QStringLiteral("RPG"), QStringLiteral("Sci-Fi")}),
    };

    m_folders = {
        CatalogFolder{QStringLiteral("Recently Played"), QStringLiteral("Jump back in"), allGames.mid(0, 5)},
        CatalogFolder{QStringLiteral("Favorites"), QStringLiteral("Pinned games"), {allGames[1], allGames[4], allGames[6], allGames[8]}},
        CatalogFolder{QStringLiteral("Steam"), QStringLiteral("PC library"), {allGames[0], allGames[1], allGames[6], allGames[7], allGames[9], allGames[10]}},
        CatalogFolder{QStringLiteral("Epic"), QStringLiteral("Storefront"), {allGames[3], allGames[5], allGames[8]}},
        CatalogFolder{QStringLiteral("Xbox"), QStringLiteral("Linked account"), {allGames[4], allGames[11]}},
        CatalogFolder{QStringLiteral("RPG"), QStringLiteral("Long sessions"), {allGames[0], allGames[1], allGames[11]}},
        CatalogFolder{QStringLiteral("Shooters"), QStringLiteral("Fast response"), {allGames[2], allGames[3], allGames[7], allGames[9]}},
        CatalogFolder{QStringLiteral("RTX Showcase"), QStringLiteral("Visual upgrades"), {allGames[0], allGames[5], allGames[8]}},
    };
}

void CatalogView::setLaunchRequestedCallback(std::function<void(const CatalogGame &)> callback) {
    m_onLaunchRequested = std::move(callback);
}

void CatalogView::setStreamInputCallbacks(std::function<void(quint16, quint16, quint16, bool)> keyCallback,
                                          std::function<void(qint16, qint16)> mouseMoveCallback,
                                          std::function<void(quint8, bool)> mouseButtonCallback,
                                          std::function<void(qint16)> mouseWheelCallback) {
    m_onStreamKey = std::move(keyCallback);
    m_onStreamMouseMove = std::move(mouseMoveCallback);
    m_onStreamMouseButton = std::move(mouseButtonCallback);
    m_onStreamMouseWheel = std::move(mouseWheelCallback);
}

void CatalogView::paintEvent(QPaintEvent *) {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing, true);
    drawBackground(painter);
    drawHeader(painter);
    drawFolderRail(painter);
    drawGameColumn(painter);
    drawDetailPanel(painter);
    drawStreamFrame(painter);
    drawFooter(painter);
    if (m_loading || m_error) {
        const QRectF panel(width() * 0.5 - 250.0, height() * 0.5 - 58.0, 500.0, 116.0);
        QPainterPath path = roundedRect(panel, 24.0);
        painter.fillPath(path, withAlpha(kPanel, 224));
        painter.setPen(QPen(m_error ? QColor(0xFF, 0x9B, 0x9B) : kFocusBlue, 1.5));
        painter.drawPath(path);
        painter.setPen(m_error ? QColor(0xFF, 0xC9, 0xC5) : kTextPrimary);
        painter.setFont(uiFont(18, QFont::Bold));
        const QString title = m_overlayTitle.isEmpty()
            ? (m_error ? QStringLiteral("Library Load Failed") : QStringLiteral("Loading Library"))
            : m_overlayTitle;
        painter.drawText(panel.adjusted(24, 18, -24, -58), Qt::AlignCenter, title);
        painter.setPen(kTextSecondary);
        painter.setFont(uiFont(12, QFont::Medium));
        painter.drawText(panel.adjusted(24, 58, -24, -18), Qt::AlignCenter | Qt::TextWordWrap, m_overlayMessage);
    }
}

void CatalogView::drawStreamFrame(QPainter &painter) {
    if (m_streamFrame.isNull()) return;
    const QRectF target = rect();
    const QSize frameSize = m_streamFrame.size();
    if (frameSize.isEmpty()) return;

    const qreal frameRatio = static_cast<qreal>(frameSize.width()) / static_cast<qreal>(frameSize.height());
    const qreal targetRatio = target.width() / target.height();
    QRectF source(0.0, 0.0, frameSize.width(), frameSize.height());
    if (frameRatio > targetRatio) {
        const qreal sourceWidth = frameSize.height() * targetRatio;
        source.setLeft((frameSize.width() - sourceWidth) * 0.5);
        source.setWidth(sourceWidth);
    } else if (frameRatio < targetRatio) {
        const qreal sourceHeight = frameSize.width() / targetRatio;
        source.setTop((frameSize.height() - sourceHeight) * 0.5);
        source.setHeight(sourceHeight);
    }

    painter.save();
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    painter.drawImage(target, m_streamFrame, source);
    painter.restore();
}

void CatalogView::showEvent(QShowEvent *event) {
    QWidget::showEvent(event);
}

void CatalogView::hideEvent(QHideEvent *event) {
    releasePointerLock();
    releaseKeyboard();
    QWidget::hideEvent(event);
}

void CatalogView::keyPressEvent(QKeyEvent *event) {
    if (!m_streamFrame.isNull()) {
        if (event->key() == Qt::Key_Escape && m_pointerLocked) {
            if (!event->isAutoRepeat() && !m_pointerUnlockTimer.isActive()) {
                m_statusText = QStringLiteral("Hold Esc to release pointer lock...");
                m_pointerUnlockTimer.start();
                update();
            }
            event->accept();
            return;
        }
        const std::optional<KeyMapping> mapping = streamKeyMapping(event->key());
        if (mapping && m_onStreamKey && !event->isAutoRepeat()) m_onStreamKey(mapping->vk, mapping->scancode, streamModifiers(event->modifiers()), true);
        event->accept();
        return;
    }
    switch (event->key()) {
        case Qt::Key_Left:
        case Qt::Key_A:
            moveFolder(-1);
            event->accept();
            return;
        case Qt::Key_Right:
        case Qt::Key_D:
            moveFolder(1);
            event->accept();
            return;
        case Qt::Key_Up:
        case Qt::Key_W:
            moveGame(-1);
            event->accept();
            return;
        case Qt::Key_Down:
        case Qt::Key_S:
            moveGame(1);
            event->accept();
            return;
        case Qt::Key_Return:
        case Qt::Key_Enter:
        case Qt::Key_Space:
            if (!m_folders.isEmpty() && !m_folders[m_selectedFolder].games.isEmpty()) {
                const CatalogGame &game = m_folders[m_selectedFolder].games[m_selectedGame];
                m_statusText = QStringLiteral("Launching: %1").arg(game.title);
                if (m_onLaunchRequested) m_onLaunchRequested(game);
                update();
            }
            event->accept();
            return;
        case Qt::Key_Escape:
        case Qt::Key_Backspace:
            m_statusText = QStringLiteral("Back placeholder: Library root");
            update();
            event->accept();
            return;
        default:
            QWidget::keyPressEvent(event);
            return;
    }
}

void CatalogView::keyReleaseEvent(QKeyEvent *event) {
    if (!m_streamFrame.isNull()) {
        if (event->key() == Qt::Key_Escape && m_pointerUnlockTimer.isActive()) {
            m_pointerUnlockTimer.stop();
            if (m_pointerLocked) m_statusText = QStringLiteral("Pointer locked: Esc releases");
            update();
            event->accept();
            return;
        }
        const std::optional<KeyMapping> mapping = streamKeyMapping(event->key());
        if (mapping && m_onStreamKey && !event->isAutoRepeat()) m_onStreamKey(mapping->vk, mapping->scancode, streamModifiers(event->modifiers()), false);
        event->accept();
        return;
    }
    QWidget::keyReleaseEvent(event);
}

void CatalogView::mousePressEvent(QMouseEvent *event) {
    if (!m_streamFrame.isNull()) {
        if (!m_pointerLocked) activatePointerLock();
        const quint8 button = streamMouseButton(event->button());
        if (button != 0 && m_onStreamMouseButton) m_onStreamMouseButton(button, true);
        event->accept();
        return;
    }
    event->ignore();
}

void CatalogView::mouseReleaseEvent(QMouseEvent *event) {
    if (!m_streamFrame.isNull()) {
        const quint8 button = streamMouseButton(event->button());
        if (button != 0 && m_onStreamMouseButton) m_onStreamMouseButton(button, false);
        event->accept();
        return;
    }
    event->ignore();
}

void CatalogView::mouseMoveEvent(QMouseEvent *event) {
    if (!m_streamFrame.isNull()) {
        if (m_ignoringPointerWarp) {
            m_ignoringPointerWarp = false;
            event->accept();
            return;
        }
        if (m_pointerLocked) {
            const QPointF center = pointerLockCenter();
            const QPointF delta = event->position() - center;
            const qint16 dx = clampI16(delta.x());
            const qint16 dy = clampI16(delta.y());
            if ((dx != 0 || dy != 0) && m_onStreamMouseMove) m_onStreamMouseMove(dx, dy);
            m_ignoringPointerWarp = true;
            QCursor::setPos(mapToGlobal(center.toPoint()));
        } else if (!m_lastMousePosition.isNull()) {
            const QPointF delta = event->position() - m_lastMousePosition;
            const qint16 dx = clampI16(delta.x());
            const qint16 dy = clampI16(delta.y());
            if ((dx != 0 || dy != 0) && m_onStreamMouseMove) m_onStreamMouseMove(dx, dy);
            m_lastMousePosition = event->position();
        }
        event->accept();
        return;
    }
    event->ignore();
}

void CatalogView::wheelEvent(QWheelEvent *event) {
    if (!m_streamFrame.isNull()) {
        const QPoint delta = event->angleDelta();
        if (delta.y() != 0 && m_onStreamMouseWheel) m_onStreamMouseWheel(clampI16(-delta.y()));
        event->accept();
        return;
    }
    QWidget::wheelEvent(event);
}

void CatalogView::activatePointerLock() {
    if (m_streamFrame.isNull() || m_pointerLocked) return;
    setFocus(Qt::MouseFocusReason);
    grabKeyboard();
    grabMouse(Qt::BlankCursor);
    m_pointerLocked = true;
    m_ignoringPointerWarp = true;
    const QPointF center = pointerLockCenter();
    m_lastMousePosition = center;
    QCursor::setPos(mapToGlobal(center.toPoint()));
    m_statusText = QStringLiteral("Pointer locked: Esc releases");
    qInfo() << "[Input] Pointer lock active";
    update();
}

void CatalogView::releasePointerLock() {
    if (!m_pointerLocked) return;
    m_pointerUnlockTimer.stop();
    releaseMouse();
    m_pointerLocked = false;
    m_ignoringPointerWarp = false;
    m_lastMousePosition = mapFromGlobal(QCursor::pos());
    if (!m_streamFrame.isNull()) m_statusText = QStringLiteral("Streaming: click to lock pointer, Esc to release");
    qInfo() << "[Input] Pointer lock released";
    update();
}

QPointF CatalogView::pointerLockCenter() const {
    return QPointF(width() * 0.5, height() * 0.5);
}

void CatalogView::moveFolder(int delta) {
    if (m_folders.isEmpty()) return;
    m_selectedFolder = (m_selectedFolder + delta + m_folders.size()) % m_folders.size();
    m_selectedGame = 0;
    m_statusText = QStringLiteral("Folder: %1").arg(m_folders[m_selectedFolder].title);
    update();
}

void CatalogView::moveGame(int delta) {
    if (m_folders.isEmpty()) return;
    const int count = m_folders[m_selectedFolder].games.size();
    if (count <= 0) return;
    m_selectedGame = (m_selectedGame + delta + count) % count;
    m_statusText = QStringLiteral("Selected: %1").arg(m_folders[m_selectedFolder].games[m_selectedGame].title);
    update();
}

void CatalogView::drawBackground(QPainter &painter) {
    QLinearGradient base(rect().topLeft(), rect().bottomLeft());
    base.setColorAt(0.0, QColor(0x0B, 0x12, 0x20));
    base.setColorAt(0.44, kBackground);
    base.setColorAt(1.0, QColor(0x02, 0x04, 0x08));
    painter.fillRect(rect(), base);

    QRadialGradient blueGlow(QPointF(width() * 0.52, height() * 0.18), width() * 0.62);
    blueGlow.setColorAt(0.0, withAlpha(QColor(0x5E, 0xC8, 0xFF), 44));
    blueGlow.setColorAt(1.0, withAlpha(QColor(0x5E, 0xC8, 0xFF), 0));
    painter.fillRect(rect(), blueGlow);

    QRadialGradient lowerGlow(QPointF(width() * 0.78, height() * 0.86), width() * 0.46);
    lowerGlow.setColorAt(0.0, withAlpha(kFocusGreen, 28));
    lowerGlow.setColorAt(1.0, withAlpha(kFocusGreen, 0));
    painter.fillRect(rect(), lowerGlow);

    painter.setPen(QPen(withAlpha(kFocusBlue, 16), 1.0));
    for (int band = 0; band < 9; ++band) {
        QPainterPath wave;
        const qreal drift = std::fmod(m_backgroundPhase * (width() * 0.36) + band * 31.0, width() + 160.0);
        const qreal yBase = height() * (0.24 + band * 0.062);
        wave.moveTo(-80.0, yBase);
        for (int point = 0; point <= 48; ++point) {
            const qreal t = point / 48.0;
            const qreal x = t * (width() + 160.0) - 80.0;
            const qreal y = yBase + std::sin(t * 8.2 + band * 0.68 + m_backgroundPhase * 1.57079632679) * (18.0 + band * 1.6);
            wave.lineTo(x + drift - width() * 0.18, y);
        }
        painter.drawPath(wave);

        QPainterPath wrapWave;
        wrapWave.moveTo(-80.0, yBase);
        for (int point = 0; point <= 48; ++point) {
            const qreal t = point / 48.0;
            const qreal x = t * (width() + 160.0) - 80.0;
            const qreal y = yBase + std::sin(t * 8.2 + band * 0.68 + m_backgroundPhase * 1.57079632679) * (18.0 + band * 1.6);
            wrapWave.lineTo(x + drift - width() * 0.18 - width() - 160.0, y);
        }
        painter.drawPath(wrapWave);
    }

    for (int i = 0; i < 36; ++i) {
        const qreal seed = static_cast<qreal>((i * 37) % 101) / 101.0;
        const qreal lane = static_cast<qreal>((i * 19) % 100) / 100.0;
        const qreal speed = 0.075 + static_cast<qreal>((i * 11) % 17) / 260.0;
        const qreal travel = std::fmod(seed + m_backgroundPhase * speed, 1.0);
        const qreal x = travel * (width() + 140.0) - 70.0;
        const qreal yBase = height() * (0.22 + lane * 0.56);
        const qreal y = yBase + std::sin(m_backgroundPhase * 5.0 + i * 0.73) * (5.0 + (i % 4));
        const qreal pulse = 0.55 + 0.45 * std::sin(m_backgroundPhase * 7.0 + i * 1.31);
        const qreal radius = 0.7 + (i % 3) * 0.28 + pulse * 0.42;
        QColor sparkle = (i % 3 == 0) ? kFocusGreen : kFocusBlue;
        sparkle.setAlphaF(0.08 + pulse * 0.18);
        painter.setPen(Qt::NoPen);
        painter.setBrush(sparkle);
        painter.drawEllipse(QPointF(x, y), radius, radius);
    }

    painter.fillRect(QRectF(0, 0, width(), height()), withAlpha(Qt::black, 56));
}

void CatalogView::drawHeader(QPainter &painter) {
    painter.setPen(kTextPrimary);
    painter.setFont(uiFont(20, QFont::DemiBold));
    painter.drawText(QRectF(34, 24, 240, 30), QStringLiteral("OpenNOW"));

    painter.setPen(kTextMuted);
    painter.setFont(uiFont(11, QFont::Medium));
    painter.drawText(QRectF(35, 52, 280, 20), QStringLiteral("Library / Controller Mode"));

    painter.setPen(kTextSecondary);
    painter.setFont(uiFont(12, QFont::DemiBold));
    painter.drawText(QRectF(width() - 310, 28, 180, 22), Qt::AlignRight, m_accountName);
    painter.setPen(kTextMuted);
    painter.setFont(uiFont(11, QFont::Medium));
    painter.drawText(QRectF(width() - 118, 28, 86, 22), Qt::AlignRight, currentTimeText());
}

void CatalogView::drawFolderRail(QPainter &painter) {
    if (m_folders.isEmpty()) return;
    const qreal centerX = width() * 0.27;
    const qreal railY = folderRailY(height());
    const qreal gap = std::clamp<qreal>(width() * 0.09, 184.0, 280.0);

    for (int i = 0; i < m_folders.size(); ++i) {
        const int offset = i - m_selectedFolder;
        if (std::abs(offset) > 4) continue;
        const bool selected = i == m_selectedFolder;
        const QSizeF size = selected ? QSizeF(174, 132) : QSizeF(130, 98);
        const qreal x = centerX + offset * gap - size.width() / 2.0;
        const qreal y = railY + (selected ? 0.0 : 16.0);
        drawFolder(painter, m_folders[i], QRectF(x, y, size.width(), size.height()), selected, i);
    }
}

void CatalogView::drawFolder(QPainter &painter, const CatalogFolder &folder, const QRectF &rect, bool selected, int folderIndex) {
    painter.save();
    if (!selected) painter.setOpacity(0.58);

    const QColor edge = selected ? kFocusBlue : withAlpha(Qt::white, 42);
    const QRectF tab(rect.left() + rect.width() * 0.09, rect.top(), rect.width() * 0.42, rect.height() * 0.24);
    QPainterPath tabPath = roundedRect(tab, 9.0);
    painter.fillPath(tabPath, selected ? withAlpha(kFocusBlue, 46) : withAlpha(kPanelRaised, 220));

    const QRectF body(rect.left(), rect.top() + rect.height() * 0.12, rect.width(), rect.height() * 0.88);
    QPainterPath bodyPath = roundedRect(body, selected ? 22.0 : 17.0);
    QLinearGradient bodyGradient(body.topLeft(), body.bottomRight());
    bodyGradient.setColorAt(0.0, selected ? QColor(0x23, 0x36, 0x4A) : QColor(0x1B, 0x22, 0x2D));
    bodyGradient.setColorAt(1.0, QColor(0x0A, 0x0D, 0x13));
    painter.fillPath(bodyPath, bodyGradient);

    if (selected) {
        QRadialGradient glow(body.center(), body.width() * 0.85);
        glow.setColorAt(0.0, withAlpha(kFocusBlue, 82));
        glow.setColorAt(1.0, withAlpha(kFocusBlue, 0));
        painter.fillPath(bodyPath, glow);
    }

    const QList<CatalogGame> previews = randomizedPreviewGames(folder, folderIndex);
    const int previewCount = std::min(6, static_cast<int>(previews.size()));
    const qreal innerPad = selected ? 14.0 : 11.0;
    const qreal gridGap = selected ? 6.0 : 5.0;
    const int columns = 3;
    const int rows = 2;
    const qreal cellWidth = (body.width() - innerPad * 2.0 - gridGap * (columns - 1)) / columns;
    const qreal cellHeight = (body.height() - innerPad * 2.0 - gridGap * (rows - 1)) / rows;
    for (int i = 0; i < previewCount; ++i) {
        const int column = i % columns;
        const int row = i / columns;
        const QRectF thumb(body.left() + innerPad + column * (cellWidth + gridGap),
                           body.top() + innerPad + row * (cellHeight + gridGap),
                           cellWidth,
                           cellHeight);
        QPainterPath thumbPath = roundedRect(thumb, selected ? 8.0 : 6.0);
        const QString imageUrl = !previews[i].imageUrl.isEmpty() ? previews[i].imageUrl : previews[i].heroImageUrl;
        const QPixmap *pixmap = cachedImage(imageUrl);
        if (pixmap) {
            painter.save();
            painter.setClipPath(thumbPath);
            painter.drawPixmap(thumb, *pixmap, pixmap->rect());
            painter.restore();
        } else {
            requestImage(imageUrl);
            QLinearGradient thumbGradient(thumb.topLeft(), thumb.bottomRight());
            thumbGradient.setColorAt(0.0, previews[i].accent.lighter(128));
            thumbGradient.setColorAt(1.0, previews[i].accent.darker(210));
            painter.fillPath(thumbPath, thumbGradient);
        }
        painter.setPen(QPen(withAlpha(Qt::white, 40), 1.0));
        painter.drawPath(thumbPath);
    }

    painter.setPen(QPen(edge, selected ? 2.0 : 1.0));
    painter.drawPath(bodyPath);
    painter.restore();

    painter.setPen(selected ? kTextPrimary : kTextSecondary);
    painter.setFont(uiFont(selected ? 16 : 12, selected ? QFont::Bold : QFont::DemiBold));
    painter.drawText(QRectF(rect.left() - 30, rect.bottom() + 12, rect.width() + 60, 22), Qt::AlignCenter, folder.title);
    if (selected) {
        painter.setPen(kTextMuted);
        painter.setFont(uiFont(10, QFont::Medium));
        painter.drawText(QRectF(rect.left() - 30, rect.bottom() + 34, rect.width() + 60, 18), Qt::AlignCenter, folder.subtitle);
    }
}

void CatalogView::drawGameColumn(QPainter &painter) {
    if (m_folders.isEmpty()) return;
    const CatalogFolder &folder = m_folders[m_selectedFolder];
    const qreal x = std::max<qreal>(92.0, width() * 0.19 - 28.0);
    const qreal firstY = gameColumnFirstY(height());
    const qreal rowGap = std::max<qreal>(82.0, height() * 0.082);
    for (int i = 0; i < folder.games.size(); ++i) {
        const int offset = i - m_selectedGame;
        if (offset < -1 || offset > 4) continue;
        const QRectF rect(x + (offset == 0 ? -16.0 : 0.0), firstY + offset * rowGap, offset == 0 ? 270.0 : 226.0, offset == 0 ? 72.0 : 60.0);
        drawGameCard(painter, folder.games[i], rect, i == m_selectedGame, i);
    }
}

void CatalogView::drawGameCard(QPainter &painter, const CatalogGame &game, const QRectF &rect, bool selected, int gameIndex) {
    painter.save();
    if (!selected) painter.setOpacity(0.58);
    QPainterPath path = roundedRect(rect, selected ? 18.0 : 14.0);
    painter.fillPath(path, withAlpha(kPanelRaised, 214));

    QLinearGradient fade(rect.topLeft(), rect.bottomLeft());
    fade.setColorAt(0.0, withAlpha(Qt::black, 8));
    fade.setColorAt(1.0, withAlpha(Qt::black, selected ? 172 : 204));
    painter.fillPath(path, fade);
    painter.setPen(QPen(selected ? kFocusGreen : withAlpha(Qt::white, 42), selected ? 2.0 : 1.0));
    painter.drawPath(path);

    painter.setPen(kTextPrimary);
    painter.setFont(uiFont(selected ? 16 : 13, QFont::Bold));
    painter.drawText(QRectF(rect.left() + 18, rect.top() + 13, rect.width() - 36, 22), game.title);
    painter.setPen(kTextSecondary);
    painter.setFont(uiFont(10, QFont::DemiBold));
    painter.drawText(QRectF(rect.left() + 18, rect.bottom() - 29, rect.width() - 36, 16), QStringLiteral("%1  /  %2").arg(game.store, game.subtitle));
    painter.setPen(withAlpha(kTextMuted, 180));
    painter.setFont(uiFont(9, QFont::Medium));
    painter.drawText(QRectF(rect.right() - 44, rect.top() + 10, 28, 16), Qt::AlignRight, QString::number(gameIndex + 1).rightJustified(2, QLatin1Char('0')));
    painter.restore();
}

void CatalogView::drawDetailPanel(QPainter &painter) {
    if (m_folders.isEmpty() || m_folders[m_selectedFolder].games.isEmpty()) return;
    const CatalogFolder &folder = m_folders[m_selectedFolder];
    const CatalogGame &game = folder.games[m_selectedGame];
    const qreal listX = std::max<qreal>(92.0, width() * 0.19 - 28.0);
    const qreal listRight = listX - 16.0 + 270.0;
    const qreal panelLeft = std::max<qreal>(listRight + 96.0, width() * 0.56);
    const qreal panelWidth = std::clamp<qreal>(width() - panelLeft - 76.0, 420.0, 700.0);
    const qreal panelTop = gameColumnFirstY(height()) - 26.0;
    const qreal panelHeight = std::clamp<qreal>(height() - panelTop - 142.0, 330.0, 430.0);
    const QRectF panel(panelLeft, panelTop, panelWidth, panelHeight);
    QPainterPath path = roundedRect(panel, 26.0);
    painter.fillPath(path, withAlpha(kPanel, 232));
    painter.setPen(QPen(withAlpha(Qt::white, 28), 1.0));
    painter.drawPath(path);

    const QString heroUrl = !game.heroImageUrl.isEmpty() ? game.heroImageUrl : game.imageUrl;
    const QPixmap *heroPixmap = cachedImage(heroUrl);
    const qreal heroMaxWidth = panel.width() - 48.0;
    const qreal heroMaxHeight = std::min<qreal>(180.0, panel.height() * 0.42);
    const QSizeF heroSize = heroPixmap
        ? fittedImageSize(heroPixmap->size(), heroMaxWidth, heroMaxHeight)
        : QSizeF(heroMaxWidth, std::min<qreal>(150.0, heroMaxHeight));
    const QRectF hero(panel.left() + 24.0, panel.top() + 24.0, heroSize.width(), heroSize.height());
    QPainterPath heroPath = roundedRect(hero, 20.0);
    if (heroPixmap) {
        painter.save();
        painter.setClipPath(heroPath);
        painter.drawPixmap(hero, *heroPixmap, heroPixmap->rect());
        painter.restore();
    } else {
        requestImage(heroUrl);
        QLinearGradient heroGradient(hero.topLeft(), hero.bottomRight());
        heroGradient.setColorAt(0.0, game.accent.lighter(138));
        heroGradient.setColorAt(0.48, game.accent.darker(145));
        heroGradient.setColorAt(1.0, QColor(0x03, 0x05, 0x09));
        painter.fillPath(heroPath, heroGradient);
    }
    QRadialGradient heroGlow(QPointF(hero.right() - 40, hero.top() + 22), hero.width() * 0.72);
    heroGlow.setColorAt(0.0, withAlpha(Qt::white, 46));
    heroGlow.setColorAt(1.0, withAlpha(Qt::white, 0));
    painter.fillPath(heroPath, heroGlow);

    painter.setPen(kTextPrimary);
    const qreal titleY = hero.bottom() + 28.0;
    painter.setFont(uiFont(25, QFont::Bold));
    painter.drawText(QRectF(panel.left() + 24.0, titleY, panel.width() - 48.0, 34.0), game.title);

    painter.setPen(kTextSecondary);
    painter.setFont(uiFont(13, QFont::Medium));
    const QString description = !game.description.isEmpty()
        ? game.description
        : game.subtitle + QStringLiteral(" from ") + game.store + QStringLiteral(".");
    const QRectF descriptionRect(panel.left() + 24.0, titleY + 42.0, panel.width() - 48.0, std::max<qreal>(48.0, panel.bottom() - titleY - 128.0));
    painter.save();
    painter.setClipRect(descriptionRect);
    painter.drawText(descriptionRect, Qt::TextWordWrap, description);
    painter.restore();

    qreal chipX = panel.left() + 24.0;
    for (const QString &tag : game.tags) {
        if (chipX + 84.0 > panel.right() - 24.0) break;
        const QRectF chip(chipX, panel.bottom() - 58.0, 84.0, 30.0);
        QPainterPath chipPath = roundedRect(chip, 15.0);
        painter.fillPath(chipPath, withAlpha(kFocusBlue, 24));
        painter.setPen(kTextSecondary);
        painter.setFont(uiFont(10, QFont::DemiBold));
        painter.drawText(chip, Qt::AlignCenter, tag);
        chipX += 92.0;
    }
}

void CatalogView::requestImage(const QString &url) {
    if (url.isEmpty() || m_imageCache.contains(url) || m_pendingImages.contains(url) || !m_network) return;
    m_pendingImages.insert(url);
    QNetworkReply *reply = m_network->get(QNetworkRequest(QUrl(url)));
    connect(reply, &QNetworkReply::finished, this, [this, reply, url]() {
        m_pendingImages.remove(url);
        QPixmap pixmap;
        const QByteArray data = reply->readAll();
        if (reply->error() == QNetworkReply::NoError && pixmap.loadFromData(data)) {
            m_imageCache.insert(url, pixmap);
            update();
        }
        reply->deleteLater();
    });
}

const QPixmap *CatalogView::cachedImage(const QString &url) const {
    if (url.isEmpty()) return nullptr;
    auto it = m_imageCache.constFind(url);
    return it == m_imageCache.constEnd() ? nullptr : &it.value();
}

void CatalogView::drawFooter(QPainter &painter) {
    const QRectF footer(34.0, height() - 56.0, width() - 68.0, 26.0);
    painter.setPen(kTextMuted);
    painter.setFont(uiFont(11, QFont::DemiBold));
    painter.drawText(footer, Qt::AlignLeft | Qt::AlignVCenter, QStringLiteral("D-Pad / Left Stick: Navigate    A / Enter: Launch    B / Esc: Back"));
    painter.drawText(footer, Qt::AlignRight | Qt::AlignVCenter, m_statusText);
}

} // namespace OpnQt
