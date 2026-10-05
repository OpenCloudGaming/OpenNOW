#include "app/platform/WindowTheme.h"

#include <QGuiApplication>
#include <QPlatformSurfaceEvent>

#include <utility>

#ifdef Q_OS_WIN
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <dwmapi.h>
#endif

namespace {
void applyNativeTheme(QWindow *window, bool darkMode)
{
#ifdef Q_OS_WIN
    if (QGuiApplication::platformName() != QStringLiteral("windows")) return;
    const auto hwnd = reinterpret_cast<HWND>(window->winId());
    const BOOL dark = darkMode ? TRUE : FALSE;
    constexpr DWORD immersiveDarkMode = 20;
    constexpr DWORD immersiveDarkModeBefore20H1 = 19;
    auto result = DwmSetWindowAttribute(hwnd, immersiveDarkMode, &dark, sizeof(dark));
    if (result == E_INVALIDARG)
        result = DwmSetWindowAttribute(hwnd, immersiveDarkModeBefore20H1, &dark, sizeof(dark));
    if (FAILED(result) && result != E_INVALIDARG)
        qWarning("Could not update the native titlebar theme: HRESULT 0x%08lx", static_cast<unsigned long>(result));
#else
    Q_UNUSED(window);
    Q_UNUSED(darkMode);
#endif
}
}

WindowTheme::WindowTheme(QObject *parent)
    : WindowTheme(applyNativeTheme, parent)
{
}

WindowTheme::WindowTheme(std::function<void(QWindow *, bool)> applyTheme, QObject *parent)
    : QObject(parent), m_applyTheme(std::move(applyTheme))
{
}

void WindowTheme::setTargetWindow(QWindow *window)
{
    if (m_window == window) return;
    if (m_window) m_window->removeEventFilter(this);
    m_window = window;
    if (m_window) m_window->installEventFilter(this);
    emit targetWindowChanged();
    apply();
}

void WindowTheme::setDarkMode(bool darkMode)
{
    if (m_darkMode == darkMode) return;
    m_darkMode = darkMode;
    emit darkModeChanged();
    apply();
}

bool WindowTheme::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_window) {
        if (event->type() == QEvent::PlatformSurface
                && static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType()
                    == QPlatformSurfaceEvent::SurfaceCreated) {
            apply();
        } else if (event->type() == QEvent::ApplicationPaletteChange
                || event->type() == QEvent::ThemeChange) {
            QMetaObject::invokeMethod(this, &WindowTheme::apply, Qt::QueuedConnection);
        }
    }
    return QObject::eventFilter(watched, event);
}

void WindowTheme::apply()
{
    if (m_window && m_window->handle()) m_applyTheme(m_window, m_darkMode);
}
