#pragma once

#include <QObject>
#include <QSet>

class AppController;

class InputModeTracker final : public QObject
{
public:
    explicit InputModeTracker(AppController *controller, QObject *parent = nullptr);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    AppController *m_controller;
    QSet<quint64> m_pressedKeys;
    QSet<quint64> m_cancelledKeys;
    Qt::MouseButtons m_pressedButtons;
    Qt::MouseButtons m_cancelledButtons;
    bool m_touchActive = false;
    bool m_tabletActive = false;
    void updateLaunchDrain();
};
