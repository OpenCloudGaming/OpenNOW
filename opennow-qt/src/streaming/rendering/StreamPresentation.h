#pragma once

#include <QObject>
#include <memory>

class NativeStreamRuntime;
class QQuickWindow;

class StreamPresentation final : public QObject
{
public:
    explicit StreamPresentation(NativeStreamRuntime *runtime, QObject *parent = nullptr);
    ~StreamPresentation() override;
    void attach(QQuickWindow *window);

protected:
    bool eventFilter(QObject *object, QEvent *event) override;

private:
    void snapshotWindow();
    void updateOutput();
    struct State;
    std::unique_ptr<State> d;
};
