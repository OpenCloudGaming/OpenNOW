#include "WindowsWindowMoveLoop.h"

#include <QGuiApplication>
#include <QPlatformSurfaceEvent>

#include <utility>

namespace {
constexpr quint32 destroyMessage = 0x0082;
constexpr quint32 systemCommandMessage = 0x0112;
constexpr quintptr moveCommand = 0xf010;
constexpr quintptr sizeCommand = 0xf000;
}

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <commctrl.h>

namespace {
class WindowsOperations final : public WindowsWindowMoveLoop::NativeOperations
{
public:
    bool install(quintptr window, WindowsWindowMoveLoop *owner) override
    {
        return SetWindowSubclass(reinterpret_cast<HWND>(window), procedure,
                                 reinterpret_cast<UINT_PTR>(owner),
                                 reinterpret_cast<DWORD_PTR>(owner)) != FALSE;
    }
    void remove(quintptr window, WindowsWindowMoveLoop *owner) override
    {
        RemoveWindowSubclass(reinterpret_cast<HWND>(window), procedure,
                             reinterpret_cast<UINT_PTR>(owner));
    }
    qintptr forward(quintptr window, quint32 message, quintptr wParam, qintptr lParam) override
    {
        return DefSubclassProc(reinterpret_cast<HWND>(window), message, wParam, lParam);
    }
    void setCursorPosition(int x, int y) override { SetCursorPos(x, y); }

private:
    static LRESULT CALLBACK procedure(HWND, UINT message, WPARAM wParam, LPARAM lParam,
                                      UINT_PTR, DWORD_PTR data)
    {
        return reinterpret_cast<WindowsWindowMoveLoop *>(data)->dispatch(message, wParam, lParam);
    }
};
}
#endif

WindowsWindowMoveLoop::WindowsWindowMoveLoop(std::function<bool()> enabled,
                                           std::function<void(bool)> activeChanged,
                                           std::unique_ptr<NativeOperations> operations)
    : m_operations(std::move(operations)), m_enabled(std::move(enabled)),
      m_activeChanged(std::move(activeChanged))
{
#if defined(Q_OS_WIN)
    if (!m_operations && QGuiApplication::platformName() == QStringLiteral("windows"))
        m_operations = std::make_shared<WindowsOperations>();
#endif
}

WindowsWindowMoveLoop::~WindowsWindowMoveLoop()
{
    detach();
}

void WindowsWindowMoveLoop::setWindow(QWindow *window)
{
    if (!m_operations || m_window == window) return;
    detach();
    if (m_window) m_window->removeEventFilter(this);
    m_window = window;
    if (m_window) m_window->installEventFilter(this);
    attach();
}

bool WindowsWindowMoveLoop::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_window && event->type() == QEvent::PlatformSurface) {
        const auto type = static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType();
        if (type == QPlatformSurfaceEvent::SurfaceCreated) attach();
        else detach();
    }
    return QObject::eventFilter(watched, event);
}

void WindowsWindowMoveLoop::attach()
{
    if (!m_operations || m_handle || !m_window || !m_window->handle()) return;
    const auto handle = static_cast<quintptr>(m_window->winId());
    if (m_operations->install(handle, this)) m_handle = handle;
}

void WindowsWindowMoveLoop::detach()
{
    if (const auto handle = std::exchange(m_handle, 0)) m_operations->remove(handle, this);
}

qintptr WindowsWindowMoveLoop::dispatch(quint32 message, quintptr wParam, qintptr lParam)
{
    const auto operations = m_operations;
    const auto handle = m_handle;
    if (message == destroyMessage) detach();
    const auto command = wParam & 0xfff0;
    if (message != systemCommandMessage || (command != moveCommand && command != sizeCommand)
        || !m_enabled())
        return operations->forward(handle, message, wParam, lParam);
    const QPointer<WindowsWindowMoveLoop> self(this);
    m_activeChanged(true);
    if (wParam & 0x000f)
        operations->setCursorPosition(static_cast<qint16>(lParam & 0xffff),
                                      static_cast<qint16>((lParam >> 16) & 0xffff));
    const auto result = operations->forward(handle, message, wParam, lParam);
    if (self) m_activeChanged(false);
    return result;
}
