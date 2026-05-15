#pragma once

#include <QtCore/QHash>
#include <QtCore/QString>
#include <QtWidgets/QMainWindow>

class QButtonGroup;
class QLabel;
class QPushButton;
class QStackedWidget;
class QVBoxLayout;
class QWidget;

namespace OpnQt {

enum class Page {
    SignIn = 0,
    Library,
    Store,
    Settings,
    Stream,
};

class AppWindow final : public QMainWindow {
public:
    explicit AppWindow(QWidget *parent = nullptr);
    ~AppWindow() override = default;

protected:
    void closeEvent(QCloseEvent *event) override;

private:
    QWidget *buildRoot();
    QWidget *buildNavigation();
    QWidget *buildPageHost();
    QWidget *buildSignInPage();
    QWidget *buildLibraryPage();
    QWidget *buildStorePage();
    QWidget *buildSettingsPage();
    QWidget *buildStreamPage();

    void addNavigationButton(Page page, const QString &title, const QString &description);
    void selectPage(Page page);
    void restoreWindowState();
    void persistWindowState() const;
    void applyTheme();

    QLabel *m_sectionTitle = nullptr;
    QLabel *m_sectionDescription = nullptr;
    QButtonGroup *m_navigationButtons = nullptr;
    QVBoxLayout *m_navigationLayout = nullptr;
    QStackedWidget *m_pages = nullptr;
    QHash<int, QString> m_pageTitles;
    QHash<int, QString> m_pageDescriptions;
};

} // namespace OpnQt
