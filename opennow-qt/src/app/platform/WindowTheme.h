#pragma once

#include <QObject>
#include <QPointer>
#include <QWindow>

#include <functional>

class WindowTheme : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QWindow *targetWindow READ targetWindow WRITE setTargetWindow NOTIFY targetWindowChanged)
    Q_PROPERTY(bool darkMode READ darkMode WRITE setDarkMode NOTIFY darkModeChanged)

public:
    explicit WindowTheme(QObject *parent = nullptr);
    explicit WindowTheme(std::function<void(QWindow *, bool)> applyTheme, QObject *parent = nullptr);

    QWindow *targetWindow() const { return m_window; }
    bool darkMode() const { return m_darkMode; }
    void setTargetWindow(QWindow *window);
    void setDarkMode(bool darkMode);

signals:
    void targetWindowChanged();
    void darkModeChanged();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void apply();

    QPointer<QWindow> m_window;
    bool m_darkMode = false;
    std::function<void(QWindow *, bool)> m_applyTheme;
};
