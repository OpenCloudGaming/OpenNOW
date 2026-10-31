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
    Q_PROPERTY(bool clientDecorations READ clientDecorations NOTIFY clientDecorationsChanged)

public:
    static bool prepareWindow(QWindow *window);
    explicit WindowTheme(QObject *parent = nullptr);
    explicit WindowTheme(std::function<void(QWindow *, bool)> applyTheme, QObject *parent = nullptr);

    QWindow *targetWindow() const { return m_window; }
    bool darkMode() const { return m_darkMode; }
    bool clientDecorations() const { return m_clientDecorations; }
    void setTargetWindow(QWindow *window);
    void setDarkMode(bool darkMode);

signals:
    void targetWindowChanged();
    void darkModeChanged();
    void clientDecorationsChanged();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void apply();

    QPointer<QWindow> m_window;
    bool m_darkMode = false;
    bool m_clientDecorations = false;
    std::function<void(QWindow *, bool)> m_applyTheme;
};
