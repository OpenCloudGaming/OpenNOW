#pragma once

#include "OpnQtAuthService.h"
#include "OpnQtCatalogView.h"

#include <QtWidgets/QMainWindow>

class QWidget;
class QLabel;
class QPushButton;
class QCheckBox;
class QKeyEvent;
class QStackedWidget;

namespace OpnQt {

struct AuthSession;

class AppWindow final : public QMainWindow {
public:
    explicit AppWindow(QWidget *parent = nullptr);
    ~AppWindow() override = default;

protected:
    void closeEvent(QCloseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    QWidget *buildLoginWindow();
    void showCatalog(const AuthSession &session);
    void showCatalogPreview();
    void loadGamesIntoCatalog(bool allowAuthRefresh);
    void probeLaunchSignaling(const CatalogGame &game);
    void forceLoginAfterAuthFailure(const QString &message);
    void wireAuthUi();
    void checkSavedSessionOnStartup();
    void beginBrowserSignIn();
    void setAuthBusy(const QString &message);
    void setAuthReady(const QString &message = QString());
    void setAuthError(const QString &message);
    void setAuthenticated(const AuthSession &session);
    void restoreWindowState();
    void persistWindowState() const;
    void applyTheme();

    QLabel *m_messageLabel = nullptr;
    QLabel *m_statusLabel = nullptr;
    QCheckBox *m_stayLoggedIn = nullptr;
    QPushButton *m_browserButton = nullptr;
    QStackedWidget *m_stack = nullptr;
    CatalogView *m_catalogView = nullptr;
    AuthSession m_currentSession;
};

} // namespace OpnQt
