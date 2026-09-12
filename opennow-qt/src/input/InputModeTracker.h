#pragma once

#include <QObject>

class AppController;

class InputModeTracker final : public QObject
{
public:
    explicit InputModeTracker(AppController *controller, QObject *parent = nullptr);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    AppController *m_controller;
};
