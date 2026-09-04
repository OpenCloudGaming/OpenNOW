#pragma once

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include <functional>

class AppController final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString route READ route NOTIFY routeChanged)
    Q_PROPERTY(QString backRoute READ backRoute NOTIFY routeChanged)
    Q_PROPERTY(QString overlay READ overlay NOTIFY overlayChanged)
    Q_PROPERTY(bool reducedMotion READ reducedMotion WRITE setReducedMotion NOTIFY reducedMotionChanged)
    Q_PROPERTY(int controllerCount READ controllerCount WRITE setControllerCount NOTIFY controllerCountChanged)
    Q_PROPERTY(QString inputMode READ inputMode WRITE setInputMode NOTIFY inputModeChanged)

public:
    explicit AppController(QObject *parent = nullptr);

    [[nodiscard]] QString route() const;
    [[nodiscard]] QString backRoute() const;
    [[nodiscard]] QString overlay() const;
    [[nodiscard]] bool reducedMotion() const;
    [[nodiscard]] int controllerCount() const;
    [[nodiscard]] QString inputMode() const;

    Q_INVOKABLE bool navigate(const QString &route);
    Q_INVOKABLE bool navigateFromLastPrimary(const QString &route);
    Q_INVOKABLE bool showOverlay(const QString &overlay);
    Q_INVOKABLE bool goBack();
    Q_INVOKABLE bool cyclePrimaryRoute(int direction);
    Q_INVOKABLE bool cycleGuidePage(int direction);
    Q_INVOKABLE QStringList supportedRoutes() const;
    Q_INVOKABLE bool openExternalUrl(const QString &url) const;
    Q_INVOKABLE bool openLocalPath(const QString &path, bool reveal = false) const;
    Q_INVOKABLE QString readClipboardText() const;
    Q_INVOKABLE bool writeClipboardText(const QString &text) const;
    Q_INVOKABLE QString normalizeNativeStreamerExecutable(const QString &urlOrPath) const;
    Q_INVOKABLE bool copyScreenshotTo(const QString &sourcePath,
                                      const QString &destinationUrlOrPath) const;
    Q_INVOKABLE bool openThemeDirectory() const;
    Q_INVOKABLE QString captureScreenRegion(int x, int y, int width, int height,
                                            const QString &gameTitle) const;
    Q_INVOKABLE bool captureScreenRegionTo(int x, int y, int width, int height,
                                           const QString &outputPath) const;
    Q_INVOKABLE bool ensureDirectLaunchAssociation() const;
    Q_INVOKABLE void activateWindow();
    Q_INVOKABLE void quitApplication();
    Q_INVOKABLE bool handleArguments(const QStringList &arguments);

    void setOverlayTransitionGuard(std::function<bool(bool)> guard);

public slots:
    void setReducedMotion(bool reducedMotion);
    void setControllerCount(int count);
    void setInputMode(const QString &mode);

signals:
    void routeChanged();
    void overlayChanged();
    void reducedMotionChanged();
    void controllerCountChanged();
    void inputModeChanged();
    void activationRequested();
    void directLaunchRequested(const QString &appId, const QString &title);

private:
    bool applyOverlay(const QString &overlay);
    static const QStringList &routes();
    static const QStringList &primaryRoutes();
    static const QStringList &overlays();

    QString m_route;
    QString m_overlay;
    bool m_reducedMotion = false;
    int m_controllerCount = 0;
    QString m_inputMode = QStringLiteral("keyboard");
    QVector<QString> m_routeHistory;
    std::function<bool(bool)> m_overlayTransitionGuard;
};
