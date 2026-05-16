#include "OpnQtAppWindow.h"

#include "OpnQtAuthService.h"
#include "OpnQtCatalogView.h"
#include "OpnQtGameService.h"
#include "OpnQtSessionManager.h"
#include "OpnQtSignalingClient.h"
#include "OpnQtStreamPreferences.h"
#include "OpnQtStreamTypes.h"
#include "OpnQtWebRtcStreamSession.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QDebug>
#include <QtCore/QSettings>
#include <QtGui/QCloseEvent>
#include <QtGui/QFont>
#include <QtGui/QKeyEvent>
#include <QtGui/QLinearGradient>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QPixmap>
#include <QtGui/QRadialGradient>
#include <QtWidgets/QApplication>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QFrame>
#include <QtWidgets/QGraphicsDropShadowEffect>
#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QStyle>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>

namespace OpnQt {
namespace {

constexpr int kDefaultWindowWidth = 1024;
constexpr int kDefaultWindowHeight = 768;
constexpr int kLoginContentWidth = 480;
constexpr int kLoginContentHeight = 460;
constexpr int kLogoHeight = 88;

const QColor kBackground(0x10, 0x11, 0x13);
const QColor kBackgroundB(0x17, 0x18, 0x1B);
const QColor kBrandGreen(0x34, 0xC7, 0x59);

QColor withAlpha(QColor color, int alpha) {
    color.setAlpha(alpha);
    return color;
}

QFont uiFont(int pointSize, QFont::Weight weight) {
    QFont font(QStringLiteral("Inter"));
    font.setPointSize(pointSize);
    font.setWeight(weight);
    font.setStyleStrategy(QFont::PreferAntialias);
    return font;
}

QLabel *label(const QString &text, int pointSize, QFont::Weight weight, const QString &objectName) {
    auto *view = new QLabel(text);
    view->setObjectName(objectName);
    view->setFont(uiFont(pointSize, weight));
    view->setWordWrap(true);
    return view;
}

QString assetPath(const QString &relativePath) {
    QDir dir(QCoreApplication::applicationDirPath());
    for (int depth = 0; depth < 8; ++depth) {
        const QString candidate = dir.absoluteFilePath(relativePath);
        if (QFileInfo::exists(candidate)) {
            return candidate;
        }
        dir.cdUp();
    }
    return QString();
}

class LoginBackdrop final : public QWidget {
public:
    explicit LoginBackdrop(QWidget *parent = nullptr) : QWidget(parent) {
        setAutoFillBackground(false);
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);

        QLinearGradient base(rect().topLeft(), rect().bottomLeft());
        base.setColorAt(0.0, kBackgroundB);
        base.setColorAt(0.48, kBackground);
        base.setColorAt(1.0, QColor(0x0C, 0x0D, 0x10));
        painter.fillRect(rect(), base);

        QRadialGradient topGlow(QPointF(width() * 0.5, -285.0), 720.0);
        topGlow.setColorAt(0.0, withAlpha(Qt::white, 12));
        topGlow.setColorAt(1.0, withAlpha(Qt::white, 0));
        painter.fillRect(rect(), topGlow);

        QRadialGradient lowerGlow(QPointF(width() - 245.0, height() - 126.0), 330.0);
        lowerGlow.setColorAt(0.0, withAlpha(kBrandGreen, 13));
        lowerGlow.setColorAt(1.0, withAlpha(kBrandGreen, 0));
        painter.fillRect(rect(), lowerGlow);
    }
};

class LoginCard final : public QFrame {
public:
    explicit LoginCard(QWidget *parent = nullptr) : QFrame(parent) {
        setObjectName(QStringLiteral("loginCard"));
        setFixedSize(400, 332);

        auto *shadow = new QGraphicsDropShadowEffect(this);
        shadow->setBlurRadius(24.0);
        shadow->setColor(withAlpha(Qt::black, 66));
        shadow->setOffset(0.0, 14.0);
        setGraphicsEffect(shadow);
    }

protected:
    void paintEvent(QPaintEvent *event) override {
        QFrame::paintEvent(event);

        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);
        const QRectF bounds = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
        QPainterPath cardPath;
        cardPath.addRoundedRect(bounds, 22.0, 22.0);

        painter.setPen(Qt::NoPen);
        QRadialGradient highlight(QPointF(bounds.center().x(), bounds.top() - 16.0), 260.0);
        highlight.setColorAt(0.0, withAlpha(Qt::white, 16));
        highlight.setColorAt(1.0, withAlpha(Qt::white, 0));
        painter.fillPath(cardPath, highlight);
    }
};

} // namespace

AppWindow::AppWindow(QWidget *parent)
    : QMainWindow(parent) {
    setWindowTitle(QStringLiteral("OpenNOW"));
    setMinimumSize(720, 560);
    resize(kDefaultWindowWidth, kDefaultWindowHeight);

    applyTheme();
    m_stack = new QStackedWidget();
    m_stack->addWidget(buildLoginWindow());
    m_catalogView = new CatalogView();
    m_catalogView->setLaunchRequestedCallback([this](const CatalogGame &game) {
        probeLaunchSignaling(game);
    });
    m_stack->addWidget(m_catalogView);
    setCentralWidget(m_stack);
    wireAuthUi();
    restoreWindowState();
    checkSavedSessionOnStartup();
}

void AppWindow::closeEvent(QCloseEvent *event) {
    persistWindowState();
    QMainWindow::closeEvent(event);
}

void AppWindow::keyPressEvent(QKeyEvent *event) {
    if (event->key() == Qt::Key_F6) {
        showCatalogPreview();
        event->accept();
        return;
    }
    QMainWindow::keyPressEvent(event);
}

QWidget *AppWindow::buildLoginWindow() {
    auto *root = new LoginBackdrop();
    auto *rootLayout = new QVBoxLayout(root);
    rootLayout->setContentsMargins(0, 0, 0, 0);
    rootLayout->setAlignment(Qt::AlignCenter);

    auto *content = new QWidget();
    content->setFixedSize(kLoginContentWidth, kLoginContentHeight);

    auto *contentLayout = new QVBoxLayout(content);
    contentLayout->setContentsMargins(40, 12, 40, 32);
    contentLayout->setSpacing(18);

    auto *brand = new QLabel();
    brand->setObjectName(QStringLiteral("brandLogo"));
    brand->setAlignment(Qt::AlignCenter);
    const QString logoPath = assetPath(QStringLiteral("assets/logo.png"));
    const QPixmap logo(logoPath);
    if (!logo.isNull()) {
        brand->setPixmap(logo.scaledToHeight(kLogoHeight, Qt::SmoothTransformation));
    } else {
        brand->setText(QStringLiteral("OpenNOW"));
        brand->setFont(uiFont(20, QFont::DemiBold));
    }
    brand->setFixedHeight(kLogoHeight);
    contentLayout->addWidget(brand);

    auto *card = new LoginCard();
    auto *cardLayout = new QVBoxLayout(card);
    cardLayout->setContentsMargins(56, 48, 56, 60);
    cardLayout->setSpacing(0);

    m_messageLabel = label(QStringLiteral("Access your cloud gaming library with your NVIDIA account."),
                           13,
                           QFont::Normal,
                           QStringLiteral("mutedText"));
    m_messageLabel->setAlignment(Qt::AlignCenter);
    m_messageLabel->setFixedHeight(42);
    cardLayout->addWidget(m_messageLabel);
    cardLayout->addStretch(1);

    m_statusLabel = label(QString(), 12, QFont::Medium, QStringLiteral("statusText"));
    m_statusLabel->setAlignment(Qt::AlignCenter);
    m_statusLabel->setFixedHeight(28);
    m_statusLabel->hide();
    cardLayout->addWidget(m_statusLabel);
    cardLayout->addSpacing(12);

    m_stayLoggedIn = new QCheckBox(QStringLiteral("Keep me signed in"));
    m_stayLoggedIn->setObjectName(QStringLiteral("stayLoggedIn"));
    m_stayLoggedIn->setChecked(AuthService::shared().stayLoggedIn());
    m_stayLoggedIn->setFixedHeight(24);
    cardLayout->addWidget(m_stayLoggedIn);
    cardLayout->addSpacing(32);

    m_browserButton = new QPushButton(QStringLiteral("Continue with Browser"));
    m_browserButton->setObjectName(QStringLiteral("browserButton"));
    m_browserButton->setCursor(Qt::PointingHandCursor);
    m_browserButton->setFixedHeight(48);
    cardLayout->addWidget(m_browserButton);

    contentLayout->addWidget(card, 0, Qt::AlignCenter);

    auto *footer = label(QStringLiteral("Open-source cloud gaming client for macOS"),
                         12,
                         QFont::Normal,
                         QStringLiteral("mutedText"));
    footer->setAlignment(Qt::AlignCenter);
    footer->setFixedHeight(20);
    contentLayout->addWidget(footer);

    rootLayout->addWidget(content, 0, Qt::AlignCenter);
    return root;
}

void AppWindow::wireAuthUi() {
    if (m_stayLoggedIn) {
        connect(m_stayLoggedIn, &QCheckBox::toggled, this, [](bool checked) {
            AuthService::shared().setStayLoggedIn(checked);
        });
    }
    if (m_browserButton) {
        connect(m_browserButton, &QPushButton::clicked, this, [this]() {
            beginBrowserSignIn();
        });
    }
}

void AppWindow::checkSavedSessionOnStartup() {
    AuthService &auth = AuthService::shared();
    const AuthSession saved = auth.loadSavedSession();
    if (!saved.isAuthenticated || !auth.stayLoggedIn()) {
        setAuthReady();
        return;
    }

    if (saved.isAccessTokenValid() && saved.isClientTokenValid()) {
        setAuthenticated(saved);
        return;
    }

    const bool canRefresh = saved.isAccessTokenValid() || !saved.refreshToken.isEmpty() || !saved.clientToken.isEmpty();
    if (!canRefresh) {
        setAuthReady();
        return;
    }

    setAuthBusy(QStringLiteral("Refreshing saved session..."));
    auth.refreshSession([this, saved](bool success, const AuthSession &fresh, const QString &error) {
        if (success) {
            setAuthenticated(fresh);
            return;
        }
        if (saved.isAuthenticated && saved.isAccessTokenValid()) {
            setAuthenticated(saved);
            return;
        }
        setAuthError(error.isEmpty() ? QStringLiteral("Saved session refresh failed") : error);
    });
}

void AppWindow::beginBrowserSignIn() {
    AuthService &auth = AuthService::shared();
    const bool shouldPersist = m_stayLoggedIn ? m_stayLoggedIn->isChecked() : auth.stayLoggedIn();
    auth.setStayLoggedIn(shouldPersist);

    setAuthBusy(QStringLiteral("Opening browser for sign in..."));
    auth.startOAuthLogin([this, shouldPersist](bool success, const AuthSession &session, const QString &error) {
        if (!success) {
            setAuthError(error.isEmpty() ? QStringLiteral("Sign in failed") : error);
            return;
        }
        if (shouldPersist) {
            AuthService::shared().saveSession(session);
        }
        setAuthenticated(session);
    });
}

void AppWindow::setAuthBusy(const QString &message) {
    if (m_statusLabel) {
        m_statusLabel->setProperty("state", QStringLiteral("busy"));
        m_statusLabel->setText(message);
        m_statusLabel->show();
        m_statusLabel->style()->unpolish(m_statusLabel);
        m_statusLabel->style()->polish(m_statusLabel);
    }
    if (m_browserButton) {
        m_browserButton->setEnabled(false);
        m_browserButton->setText(QStringLiteral("Waiting for Browser..."));
    }
}

void AppWindow::setAuthReady(const QString &message) {
    if (m_statusLabel) {
        if (message.isEmpty()) {
            m_statusLabel->hide();
        } else {
            m_statusLabel->setProperty("state", QStringLiteral("ready"));
            m_statusLabel->setText(message);
            m_statusLabel->show();
            m_statusLabel->style()->unpolish(m_statusLabel);
            m_statusLabel->style()->polish(m_statusLabel);
        }
    }
    if (m_browserButton) {
        m_browserButton->setEnabled(true);
        m_browserButton->setText(QStringLiteral("Continue with Browser"));
    }
}

void AppWindow::setAuthError(const QString &message) {
    if (m_statusLabel) {
        m_statusLabel->setProperty("state", QStringLiteral("error"));
        m_statusLabel->setText(message);
        m_statusLabel->show();
        m_statusLabel->style()->unpolish(m_statusLabel);
        m_statusLabel->style()->polish(m_statusLabel);
    }
    if (m_browserButton) {
        m_browserButton->setEnabled(true);
        m_browserButton->setText(QStringLiteral("Try Again"));
    }
}

void AppWindow::setAuthenticated(const AuthSession &session) {
    m_currentSession = session;
    const QString name = !session.displayName.isEmpty()
        ? session.displayName
        : (!session.email.isEmpty() ? session.email : QStringLiteral("Account"));
    if (m_messageLabel) {
        m_messageLabel->setText(QStringLiteral("Signed in as %1.").arg(name));
    }
    if (m_statusLabel) {
        m_statusLabel->setProperty("state", QStringLiteral("success"));
        m_statusLabel->setText(QStringLiteral("Authenticated. Catalog UI is next."));
        m_statusLabel->show();
        m_statusLabel->style()->unpolish(m_statusLabel);
        m_statusLabel->style()->polish(m_statusLabel);
    }
    if (m_browserButton) {
        m_browserButton->setEnabled(true);
        m_browserButton->setText(QStringLiteral("Signed In"));
    }
    showCatalog(session);
}

void AppWindow::showCatalog(const AuthSession &session) {
    if (!m_stack || !m_catalogView) return;
    m_currentSession = session;
    const QString name = !session.displayName.isEmpty()
        ? session.displayName
        : (!session.email.isEmpty() ? session.email : QStringLiteral("Player"));
    m_catalogView->setAccountName(name);
    m_stack->setCurrentWidget(m_catalogView);
    m_catalogView->setFocus(Qt::OtherFocusReason);
    if (!session.accessToken.isEmpty()) {
        if (session.displayName.isEmpty()) {
            m_catalogView->setLoading(QStringLiteral("Fetching account details..."));
            AuthService::shared().fetchUserInfo(session.accessToken, [this](bool success, const QVariantMap &info, const QString &) {
                if (success) {
                    const QString preferred = info.value(QStringLiteral("preferred_username")).toString();
                    const QString email = info.value(QStringLiteral("email")).toString();
                    const QString name = !preferred.isEmpty()
                        ? preferred
                        : (!email.isEmpty() ? email.section(QLatin1Char('@'), 0, 0) : QString());
                    if (!name.isEmpty()) {
                        m_currentSession.displayName = name;
                        if (!email.isEmpty()) m_currentSession.email = email;
                        if (AuthService::shared().stayLoggedIn()) AuthService::shared().saveSession(m_currentSession);
                        if (m_catalogView) m_catalogView->setAccountName(name);
                    }
                }
                loadGamesIntoCatalog(true);
            });
        } else {
            loadGamesIntoCatalog(true);
        }
    }
}

void AppWindow::loadGamesIntoCatalog(bool allowAuthRefresh) {
    if (!m_catalogView) return;
    m_catalogView->setLoading(QStringLiteral("Fetching your GeForce NOW library..."));
    const QString apiToken = !m_currentSession.idToken.isEmpty() ? m_currentSession.idToken : m_currentSession.accessToken;
    GameService::shared().setAccessToken(apiToken);
    GameService::shared().fetchLibraryGames([this, allowAuthRefresh](bool success, const QList<CatalogGame> &games, const QString &error) {
        if (!m_catalogView) return;
        if (success) {
            m_catalogView->setGames(games);
            return;
        }

        const QString normalizedError = error.toCaseFolded();
        const bool authenticationFailed = normalizedError.contains(QStringLiteral("authentication")) ||
            normalizedError.contains(QStringLiteral("unauthorized")) ||
            normalizedError.contains(QStringLiteral("401"));
        if (allowAuthRefresh && authenticationFailed) {
            m_catalogView->setLoading(QStringLiteral("Refreshing session and retrying library..."));
            AuthService::shared().refreshSession([this](bool refreshSuccess, const AuthSession &fresh, const QString &refreshError) {
                if (!m_catalogView) return;
                if (refreshSuccess) {
                    AuthService::shared().saveSession(fresh);
                    m_currentSession = fresh;
                    loadGamesIntoCatalog(false);
                    return;
                }
                forceLoginAfterAuthFailure(refreshError.isEmpty()
                    ? QStringLiteral("Authentication expired. Sign in again to load your library.")
                    : refreshError);
            }, true);
            return;
        }

        if (authenticationFailed) {
            forceLoginAfterAuthFailure(QStringLiteral("Authentication expired. Sign in again to load your library."));
            return;
        }

        m_catalogView->setError(error.isEmpty() ? QStringLiteral("Unable to fetch library games") : error);
    });
}

void AppWindow::forceLoginAfterAuthFailure(const QString &message) {
    AuthService::shared().clearSession();
    GameService::shared().setAccessToken(QString());
    m_currentSession = AuthSession{};
    if (m_stack) {
        m_stack->setCurrentIndex(0);
    }
    if (m_messageLabel) {
        m_messageLabel->setText(QStringLiteral("Access your cloud gaming library with your NVIDIA account."));
    }
    if (m_stayLoggedIn) {
        m_stayLoggedIn->setChecked(AuthService::shared().stayLoggedIn());
    }
    setAuthError(message.isEmpty() ? QStringLiteral("Authentication expired. Sign in again.") : message);
}

void AppWindow::probeLaunchSignaling(const CatalogGame &game) {
    if (!m_catalogView) return;
    const QString appId = !game.launchAppId.isEmpty() ? game.launchAppId : game.title;
    if (m_currentSession.accessToken.isEmpty() && m_currentSession.idToken.isEmpty()) {
        m_catalogView->setError(QStringLiteral("Sign in before launching %1.").arg(game.title));
        return;
    }
    if (appId.isEmpty() || appId == game.title) {
        m_catalogView->setError(QStringLiteral("%1 does not have a launchable app id yet.").arg(game.title));
        return;
    }

    const QString apiToken = !m_currentSession.idToken.isEmpty() ? m_currentSession.idToken : m_currentSession.accessToken;
    StreamSettings settings;
    settings.selectedStore = game.selectedStore;
    settings.accountLinked = true;
    const QString streamingBaseUrl = loadSelectedStreamingBaseUrl();
    GameService::shared().setAccessToken(apiToken);
    GameService::shared().setStreamingBaseUrl(streamingBaseUrl);
    qInfo() << "[StreamProbe] Using streaming base URL" << streamingBaseUrl;
    m_catalogView->setLoading(QStringLiteral("Creating cloud session for %1...").arg(game.title));

    GameService::shared().launchGame(appId, game.internalTitle, settings,
        [this, game](const QString &message, const SessionInfo &progressSession) {
            Q_UNUSED(progressSession);
            if (!m_catalogView) return;
            m_catalogView->setLoading(QStringLiteral("%1 %2...").arg(message, game.title));
        },
        [this, game, settings](bool success, const SessionInfo &session, const QString &error) {
            if (!m_catalogView) return;
            if (!success) {
                m_catalogView->setError(QStringLiteral("Launch Failed"), error.isEmpty()
                    ? QStringLiteral("Launch session creation failed")
                    : QStringLiteral("Launch session creation failed: %1").arg(error));
                return;
            }
            qInfo() << "[StreamProbe] Session created"
                    << "id" << session.sessionId
                    << "status" << session.status
                    << "server" << session.serverIp
                    << "signaling" << session.signalingUrl;
            if (session.signalingServer.isEmpty() && session.signalingUrl.isEmpty()) {
                m_catalogView->setError(QStringLiteral("Launch Failed"), QStringLiteral("Session created, but no signaling endpoint was returned."));
                return;
            }
            m_catalogView->setLoading(QStringLiteral("Connecting signaling for %1...").arg(game.title));
            auto *signaling = new SignalingClient(session.signalingServer, session.sessionId, session.signalingUrl, this);
            const QString resolution = session.negotiatedStreamProfile.resolution.isEmpty()
                ? QStringLiteral("1920x1080")
                : session.negotiatedStreamProfile.resolution;
            signaling->setPeerResolution(resolution);
            auto *webRtcSession = new WebRtcStreamSession(this);
            webRtcSession->onAnswerReady([signaling](const SendAnswerRequest &answer) {
                qInfo() << "[WebRTC] Answer ready" << "length" << answer.sdp.size();
                signaling->sendAnswer(answer);
            });
            webRtcSession->onIceCandidateReady([signaling](const IceCandidatePayload &candidate) {
                qInfo() << "[WebRTC] Local ICE candidate" << candidate.sdpMid << candidate.sdpMLineIndex;
                signaling->sendIceCandidate(candidate);
            });
            signaling->onOffer([this, signaling, webRtcSession, session, settings, game](const QString &sdp) {
                qInfo() << "[StreamProbe] Offer received" << "length" << sdp.size();
                qInfo() << "[WebRTC] Backend availability" << WebRtcStreamSession::availabilityDescription();
                webRtcSession->start(session, sdp, settings, [this, signaling, webRtcSession, game](bool connected, const QString &streamError) {
                    if (connected) {
                        if (m_catalogView) m_catalogView->setError(QStringLiteral("Streaming Connected"), QStringLiteral("WebRTC connected for %1. Rendering is next.").arg(game.title));
                        return;
                    }
                    if (m_catalogView) {
                        const QString message = streamError.isEmpty()
                            ? QStringLiteral("Streaming probe reached offer SDP for %1. Media backend is next.").arg(game.title)
                            : QStringLiteral("Streaming probe reached offer SDP for %1. %2").arg(game.title, streamError);
                        m_catalogView->setError(QStringLiteral("Streaming Probe Ready"), message);
                    }
                    signaling->disconnectFromServer();
                    signaling->deleteLater();
                    webRtcSession->deleteLater();
                });
            });
            signaling->onIceCandidate([webRtcSession](const IceCandidatePayload &candidate) {
                qInfo() << "[StreamProbe] Remote ICE candidate" << candidate.sdpMid << candidate.sdpMLineIndex;
                webRtcSession->addRemoteIceCandidate(candidate);
            });
            signaling->connectToServer([this, signaling, game](bool connected, const QString &connectError) {
                if (!m_catalogView) return;
                if (!connected) {
                    m_catalogView->setError(QStringLiteral("Launch Failed"), connectError.isEmpty() ? QStringLiteral("Signaling connection failed") : connectError);
                    signaling->deleteLater();
                    return;
                }
                qInfo() << "[StreamProbe] Signaling connected";
                m_catalogView->setLoading(QStringLiteral("Waiting for stream offer for %1...").arg(game.title));
            });
        });
}

void AppWindow::showCatalogPreview() {
    AuthSession preview;
    preview.displayName = QStringLiteral("Preview Player");
    showCatalog(preview);
    if (m_catalogView) m_catalogView->setPreviewData();
}

void AppWindow::restoreWindowState() {
    QSettings settings;
    const QSize size = settings.value(QStringLiteral("qt/loginWindowSize"), QSize(kDefaultWindowWidth, kDefaultWindowHeight)).toSize();
    const QPoint position = settings.value(QStringLiteral("qt/loginWindowPosition")).toPoint();
    resize(size.expandedTo(minimumSize()));
    if (!position.isNull()) {
        move(position);
    }
}

void AppWindow::persistWindowState() const {
    QSettings settings;
    settings.setValue(QStringLiteral("qt/loginWindowSize"), size());
    settings.setValue(QStringLiteral("qt/loginWindowPosition"), pos());
}

void AppWindow::applyTheme() {
    qApp->setStyleSheet(QStringLiteral(R"(
        QMainWindow, QWidget {
            background: transparent;
            color: #F5F5F7;
            font-family: Inter, "SF Pro Display", "Segoe UI", Arial, sans-serif;
        }

        QLabel#brandLogo {
            color: #F5F5F7;
            font-weight: 600;
        }

        QLabel#mutedText {
            color: #787A82;
            line-height: 140%;
        }

        QLabel#statusText {
            padding: 5px 10px;
            border-radius: 10px;
            font-size: 12px;
        }

        QLabel#statusText[state="busy"] {
            color: #B7B8BE;
            background: rgba(255, 255, 255, 0.06);
        }

        QLabel#statusText[state="success"] {
            color: #D9FFE8;
            background: rgba(52, 199, 89, 0.14);
        }

        QLabel#statusText[state="error"] {
            color: #FFB1AA;
            background: rgba(255, 69, 58, 0.14);
        }

        QFrame#loginCard {
            background: rgba(29, 30, 34, 0.86);
            border: 1px solid rgba(255, 255, 255, 0.10);
            border-radius: 22px;
        }

        QCheckBox#stayLoggedIn {
            color: #B7B8BE;
            font-size: 13px;
            font-weight: 500;
            spacing: 9px;
        }

        QCheckBox#stayLoggedIn::indicator {
            width: 17px;
            height: 17px;
            border-radius: 4px;
            border: 1px solid rgba(255, 255, 255, 0.22);
            background: #24262B;
        }

        QCheckBox#stayLoggedIn::indicator:checked {
            border: 1px solid #34C759;
            background: #34C759;
            image: none;
        }

        QPushButton#browserButton {
            background: #34C759;
            color: #06140A;
            border: none;
            border-radius: 10px;
            font-size: 14px;
            font-weight: 600;
        }

        QPushButton#browserButton:hover {
            background: #49D56B;
        }

        QPushButton#browserButton:pressed {
            background: #2FB14F;
        }

        QPushButton#browserButton:disabled {
            background: #34363C;
            color: #787A82;
        }
    )"));
}

} // namespace OpnQt
