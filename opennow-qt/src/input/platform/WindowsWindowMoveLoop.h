#pragma once

#include <QObject>
#include <QPointer>
#include <QWindow>

#include <functional>
#include <memory>

class WindowsWindowMoveLoop final : public QObject
{
public:
    class NativeOperations
    {
    public:
        virtual ~NativeOperations() = default;
        virtual bool install(quintptr window, WindowsWindowMoveLoop *owner) = 0;
        virtual void remove(quintptr window, WindowsWindowMoveLoop *owner) = 0;
        virtual qintptr forward(quintptr window, quint32 message, quintptr wParam, qintptr lParam) = 0;
        virtual void setCursorPosition(int x, int y) = 0;
    };

    WindowsWindowMoveLoop(std::function<bool()> enabled, std::function<void(bool)> activeChanged,
                          std::unique_ptr<NativeOperations> operations = {});
    ~WindowsWindowMoveLoop() override;
    void setWindow(QWindow *window);
    qintptr dispatch(quint32 message, quintptr wParam, qintptr lParam);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void attach();
    void detach();
    QPointer<QWindow> m_window;
    quintptr m_handle = 0;
    std::shared_ptr<NativeOperations> m_operations;
    std::function<bool()> m_enabled;
    std::function<void(bool)> m_activeChanged;
};
