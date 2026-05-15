#include "OpnQtAppWindow.h"

#include <QtCore/QSettings>
#include <QtCore/QStringList>
#include <QtGui/QCloseEvent>
#include <QtGui/QFont>
#include <QtWidgets/QApplication>
#include <QtWidgets/QButtonGroup>
#include <QtWidgets/QFrame>
#include <QtWidgets/QGridLayout>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QScrollArea>
#include <QtWidgets/QSizePolicy>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QVBoxLayout>

namespace OpnQt {
namespace {

constexpr int kDefaultWindowWidth = 1180;
constexpr int kDefaultWindowHeight = 760;

QString pageKey(Page page) {
    switch (page) {
        case Page::SignIn: return QStringLiteral("signin");
        case Page::Library: return QStringLiteral("library");
        case Page::Store: return QStringLiteral("store");
        case Page::Settings: return QStringLiteral("settings");
        case Page::Stream: return QStringLiteral("stream");
    }
    return QStringLiteral("signin");
}

QLabel *label(const QString &text, int pointSize, QFont::Weight weight, const QString &objectName = QString()) {
    auto *view = new QLabel(text);
    QFont font = view->font();
    font.setPointSize(pointSize);
    font.setWeight(weight);
    view->setFont(font);
    view->setWordWrap(true);
    if (!objectName.isEmpty()) {
        view->setObjectName(objectName);
    }
    return view;
}

QFrame *card(const QString &title, const QString &body) {
    auto *frame = new QFrame();
    frame->setObjectName(QStringLiteral("card"));
    frame->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Minimum);

    auto *layout = new QVBoxLayout(frame);
    layout->setContentsMargins(22, 20, 22, 20);
    layout->setSpacing(8);
    layout->addWidget(label(title, 15, QFont::DemiBold, QStringLiteral("cardTitle")));
    layout->addWidget(label(body, 11, QFont::Normal, QStringLiteral("mutedText")));
    layout->addStretch(1);
    return frame;
}

QFrame *pill(const QString &title, const QString &value) {
    auto *frame = new QFrame();
    frame->setObjectName(QStringLiteral("pill"));
    auto *layout = new QVBoxLayout(frame);
    layout->setContentsMargins(16, 12, 16, 12);
    layout->setSpacing(3);
    layout->addWidget(label(title, 10, QFont::DemiBold, QStringLiteral("mutedText")));
    layout->addWidget(label(value, 16, QFont::Bold, QStringLiteral("pillValue")));
    return frame;
}

QWidget *pageSurface(const QString &eyebrow,
                     const QString &title,
                     const QString &body,
                     const QList<QWidget *> &content) {
    auto *surface = new QWidget();
    auto *layout = new QVBoxLayout(surface);
    layout->setContentsMargins(34, 30, 34, 34);
    layout->setSpacing(20);

    layout->addWidget(label(eyebrow.toUpper(), 10, QFont::DemiBold, QStringLiteral("eyebrow")));
    layout->addWidget(label(title, 26, QFont::Bold, QStringLiteral("pageTitle")));
    layout->addWidget(label(body, 12, QFont::Normal, QStringLiteral("pageBody")));

    for (QWidget *widget : content) {
        layout->addWidget(widget);
    }
    layout->addStretch(1);
    return surface;
}

QWidget *twoColumnCards(const QList<QWidget *> &cards) {
    auto *container = new QWidget();
    auto *grid = new QGridLayout(container);
    grid->setContentsMargins(0, 0, 0, 0);
    grid->setHorizontalSpacing(16);
    grid->setVerticalSpacing(16);

    for (int i = 0; i < cards.size(); ++i) {
        grid->addWidget(cards[i], i / 2, i % 2);
    }
    grid->setColumnStretch(0, 1);
    grid->setColumnStretch(1, 1);
    return container;
}

} // namespace

AppWindow::AppWindow(QWidget *parent)
    : QMainWindow(parent) {
    setWindowTitle(QStringLiteral("OpenNOW Qt"));
    setMinimumSize(920, 560);
    resize(kDefaultWindowWidth, kDefaultWindowHeight);

    applyTheme();
    setCentralWidget(buildRoot());
    restoreWindowState();
    selectPage(Page::SignIn);
}

void AppWindow::closeEvent(QCloseEvent *event) {
    persistWindowState();
    QMainWindow::closeEvent(event);
}

QWidget *AppWindow::buildRoot() {
    auto *root = new QWidget();
    auto *layout = new QHBoxLayout(root);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(buildNavigation());
    layout->addWidget(buildPageHost(), 1);
    return root;
}

QWidget *AppWindow::buildNavigation() {
    auto *nav = new QFrame();
    nav->setObjectName(QStringLiteral("navigation"));
    nav->setFixedWidth(286);

    auto *layout = new QVBoxLayout(nav);
    m_navigationLayout = layout;
    layout->setContentsMargins(24, 26, 24, 24);
    layout->setSpacing(16);

    layout->addWidget(label(QStringLiteral("OpenNOW"), 24, QFont::Black, QStringLiteral("brand")));
    layout->addWidget(label(QStringLiteral("Qt migration track"), 11, QFont::DemiBold, QStringLiteral("mutedText")));

    auto *divider = new QFrame();
    divider->setObjectName(QStringLiteral("divider"));
    divider->setFrameShape(QFrame::HLine);
    layout->addWidget(divider);

    m_navigationButtons = new QButtonGroup(this);
    m_navigationButtons->setExclusive(true);

    addNavigationButton(Page::SignIn, QStringLiteral("Sign In"), QStringLiteral("OAuth and saved accounts"));
    addNavigationButton(Page::Library, QStringLiteral("Library"), QStringLiteral("Catalog and game launch"));
    addNavigationButton(Page::Store, QStringLiteral("Store"), QStringLiteral("Curated browse surfaces"));
    addNavigationButton(Page::Settings, QStringLiteral("Settings"), QStringLiteral("Stream and app preferences"));
    addNavigationButton(Page::Stream, QStringLiteral("Stream"), QStringLiteral("WebRTC session surface"));

    layout->addStretch(1);
    layout->addWidget(card(QStringLiteral("Side-by-side"),
                           QStringLiteral("The AppKit build remains the shipping path while Qt reaches feature parity.")));

    return nav;
}

void AppWindow::addNavigationButton(Page page, const QString &title, const QString &description) {
    auto *button = new QPushButton(title + QStringLiteral("\n") + description);
    button->setObjectName(QStringLiteral("navButton"));
    button->setCheckable(true);
    button->setCursor(Qt::PointingHandCursor);
    button->setMinimumHeight(62);
    button->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);

    const int id = static_cast<int>(page);
    m_navigationButtons->addButton(button, id);
    m_pageTitles.insert(id, title);
    m_pageDescriptions.insert(id, description);

    if (m_navigationLayout) {
        m_navigationLayout->addWidget(button);
    }

    connect(button, &QPushButton::clicked, this, [this, page]() {
        selectPage(page);
    });
}

QWidget *AppWindow::buildPageHost() {
    auto *host = new QWidget();
    auto *layout = new QVBoxLayout(host);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    auto *header = new QFrame();
    header->setObjectName(QStringLiteral("header"));
    auto *headerLayout = new QVBoxLayout(header);
    headerLayout->setContentsMargins(34, 24, 34, 20);
    headerLayout->setSpacing(6);
    m_sectionTitle = label(QStringLiteral("Sign In"), 22, QFont::Bold, QStringLiteral("sectionTitle"));
    m_sectionDescription = label(QStringLiteral("OAuth and saved accounts"), 11, QFont::DemiBold, QStringLiteral("mutedText"));
    headerLayout->addWidget(m_sectionTitle);
    headerLayout->addWidget(m_sectionDescription);

    m_pages = new QStackedWidget();
    m_pages->addWidget(buildSignInPage());
    m_pages->addWidget(buildLibraryPage());
    m_pages->addWidget(buildStorePage());
    m_pages->addWidget(buildSettingsPage());
    m_pages->addWidget(buildStreamPage());

    layout->addWidget(header);
    layout->addWidget(m_pages, 1);
    return host;
}

QWidget *AppWindow::buildSignInPage() {
    return pageSurface(
        QStringLiteral("Authentication"),
        QStringLiteral("Browser OAuth becomes a Qt service boundary"),
        QStringLiteral("This surface maps the current email entry, authenticating, session refresh, and account switching flow into the Qt shell. The next port step is moving OPNAuthService onto Qt network, desktop-services, settings, and cryptographic primitives."),
        {
            twoColumnCards({
                card(QStringLiteral("OAuth launch"), QStringLiteral("Use QDesktopServices for system-browser sign-in and a local callback listener for PKCE completion.")),
                card(QStringLiteral("Saved sessions"), QStringLiteral("Use QSettings first, then move secrets to platform secure storage before this path ships.")),
                card(QStringLiteral("Account switching"), QStringLiteral("Preserve the current multi-account model and refresh behavior from the AppKit coordinator.")),
                card(QStringLiteral("Token refresh"), QStringLiteral("Port the refresh windows and client-token lifecycle without coupling it to UI widgets.")),
            }),
        });
}

QWidget *AppWindow::buildLibraryPage() {
    auto *stats = new QWidget();
    auto *statsLayout = new QHBoxLayout(stats);
    statsLayout->setContentsMargins(0, 0, 0, 0);
    statsLayout->setSpacing(12);
    statsLayout->addWidget(pill(QStringLiteral("Catalog"), QStringLiteral("GraphQL")));
    statsLayout->addWidget(pill(QStringLiteral("Launch"), QStringLiteral("Session API")));
    statsLayout->addWidget(pill(QStringLiteral("Input"), QStringLiteral("Controller aware")));

    return pageSurface(
        QStringLiteral("Library"),
        QStringLiteral("Game catalog and launch flow"),
        QStringLiteral("The AppKit catalog is the largest UI surface. This Qt shell reserves the same flow: library refresh, search/filter/sort, selected variant resolution, account-link checks, and launch handoff into streaming."),
        {
            stats,
            twoColumnCards({
                card(QStringLiteral("Catalog data"), QStringLiteral("Move OPNGameService parsing to Qt JSON while keeping the existing GameInfo data shape.")),
                card(QStringLiteral("Lean-back navigation"), QStringLiteral("Map the current controller-focused catalog behavior onto Qt key and gamepad adapters.")),
                card(QStringLiteral("Artwork loading"), QStringLiteral("Use QNetworkAccessManager with a bounded image cache for hero and poster art.")),
                card(QStringLiteral("Launch continuity"), QStringLiteral("Preserve active-session resume, library overlay, and stream return behavior.")),
            }),
        });
}

QWidget *AppWindow::buildStorePage() {
    return pageSurface(
        QStringLiteral("Store"),
        QStringLiteral("Curated panels and storefront browsing"),
        QStringLiteral("The store page mirrors the current panel loader and controller rail model so service logic can land before visual parity work begins."),
        {
            twoColumnCards({
                card(QStringLiteral("Panels"), QStringLiteral("Port persisted GraphQL panel fetches and retry behavior into a platform-neutral service.")),
                card(QStringLiteral("Rails"), QStringLiteral("Use Qt layouts first, then tune animation and focus states once data is live.")),
                card(QStringLiteral("Variants"), QStringLiteral("Keep store, launch app id, and linked-account selection explicit in the model.")),
                card(QStringLiteral("Picture-in-picture"), QStringLiteral("Carry over the active-stream return path as a Qt widget composition problem.")),
            }),
        });
}

QWidget *AppWindow::buildSettingsPage() {
    return pageSurface(
        QStringLiteral("Settings"),
        QStringLiteral("Cross-platform preferences"),
        QStringLiteral("Stream, video, audio, input, interface, about, and thanks sections should move behind typed settings models before individual controls are rebuilt."),
        {
            twoColumnCards({
                card(QStringLiteral("Stream profile"), QStringLiteral("Region, codec, resolution, frame rate, bitrate, and power-saver choices stay in one typed model.")),
                card(QStringLiteral("Audio devices"), QStringLiteral("Replace CoreAudio enumeration with Qt Multimedia or platform adapters depending on WebRTC requirements.")),
                card(QStringLiteral("Input"), QStringLiteral("Keep protocol encoding portable and isolate native keyboard/controller discovery.")),
                card(QStringLiteral("Interface"), QStringLiteral("Persist theme, fullscreen, and window presentation through QSettings.")),
            }),
        });
}

QWidget *AppWindow::buildStreamPage() {
    return pageSurface(
        QStringLiteral("Streaming"),
        QStringLiteral("WebRTC session host"),
        QStringLiteral("This is the riskiest parity area: native libwebrtc, platform video surfaces, input data channel delivery, stats overlays, recovery, and recording all need small adapters rather than direct AppKit dependencies."),
        {
            twoColumnCards({
                card(QStringLiteral("Signaling"), QStringLiteral("Move OPNSignalingClient to QWebSocket while preserving heartbeat and message sequencing.")),
                card(QStringLiteral("WebRTC"), QStringLiteral("Replace Objective-C WebRTC framework calls with the native C++ libwebrtc API behind IStreamSession.")),
                card(QStringLiteral("Rendering"), QStringLiteral("Host decoded frames in a Qt-compatible hardware surface on each OS.")),
                card(QStringLiteral("Recovery"), QStringLiteral("Keep the current launch generation, ICE grace period, and automatic recovery rules.")),
            }),
        });
}

void AppWindow::selectPage(Page page) {
    const int id = static_cast<int>(page);
    if (!m_pages || id < 0 || id >= m_pages->count()) {
        return;
    }

    m_pages->setCurrentIndex(id);
    if (QPushButton *button = qobject_cast<QPushButton *>(m_navigationButtons->button(id))) {
        button->setChecked(true);
    }
    if (m_sectionTitle) {
        m_sectionTitle->setText(m_pageTitles.value(id));
    }
    if (m_sectionDescription) {
        m_sectionDescription->setText(m_pageDescriptions.value(id));
    }

    QSettings settings;
    settings.setValue(QStringLiteral("qt/activePage"), pageKey(page));
}

void AppWindow::restoreWindowState() {
    QSettings settings;
    const QSize size = settings.value(QStringLiteral("qt/windowSize"), QSize(kDefaultWindowWidth, kDefaultWindowHeight)).toSize();
    const QPoint position = settings.value(QStringLiteral("qt/windowPosition")).toPoint();
    resize(size.expandedTo(minimumSize()));
    if (!position.isNull()) {
        move(position);
    }
}

void AppWindow::persistWindowState() const {
    QSettings settings;
    settings.setValue(QStringLiteral("qt/windowSize"), size());
    settings.setValue(QStringLiteral("qt/windowPosition"), pos());
}

void AppWindow::applyTheme() {
    qApp->setStyleSheet(QStringLiteral(R"(
        QMainWindow, QWidget {
            background: #070a10;
            color: #f6f8ff;
            font-family: Inter, Segoe UI, SF Pro Display, Arial, sans-serif;
        }
        QFrame#navigation {
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #111827, stop:1 #090d16);
            border-right: 1px solid rgba(255,255,255,0.08);
        }
        QFrame#header {
            background: #090d15;
            border-bottom: 1px solid rgba(255,255,255,0.08);
        }
        QLabel#brand {
            color: #ffffff;
            letter-spacing: 1px;
        }
        QLabel#sectionTitle, QLabel#pageTitle, QLabel#cardTitle, QLabel#pillValue {
            color: #ffffff;
        }
        QLabel#mutedText, QLabel#pageBody {
            color: #a9b4c7;
        }
        QLabel#eyebrow {
            color: #67f0a0;
            letter-spacing: 1.4px;
        }
        QFrame#divider {
            color: rgba(255,255,255,0.08);
            background: rgba(255,255,255,0.08);
            max-height: 1px;
        }
        QPushButton#navButton {
            background: rgba(255,255,255,0.045);
            border: 1px solid rgba(255,255,255,0.08);
            border-radius: 14px;
            color: #cbd5e1;
            padding: 10px 14px;
            text-align: left;
            line-height: 150%;
        }
        QPushButton#navButton:hover {
            background: rgba(103,240,160,0.10);
            border-color: rgba(103,240,160,0.36);
            color: #ffffff;
        }
        QPushButton#navButton:checked {
            background: rgba(103,240,160,0.18);
            border-color: rgba(103,240,160,0.70);
            color: #ffffff;
        }
        QFrame#card, QFrame#pill {
            background: rgba(255,255,255,0.055);
            border: 1px solid rgba(255,255,255,0.10);
            border-radius: 18px;
        }
        QStackedWidget {
            background: qradialgradient(cx:0.15, cy:0.08, radius:1.1, fx:0.1, fy:0.0, stop:0 rgba(43,122,84,0.26), stop:0.48 #090d15, stop:1 #070a10);
        }
    )"));
}

} // namespace OpnQt
