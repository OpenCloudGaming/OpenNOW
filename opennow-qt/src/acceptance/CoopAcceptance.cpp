#include "acceptance/AcceptanceSession.h"

#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QPointF>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QDeadlineTimer>
#include <qpa/qwindowsysteminterface.h>

#include <cstdlib>

using namespace Qt::StringLiterals;

// Co-op and Friends (C2) acceptance: tests/CoopAcceptance.qml seeds FriendsState and CoopState
// through a mock core client and a mock streamer, then walks every C2 scene. --coop-scene <name>
// settles one scene; --screenshot <png> captures it and --screenshot-dir <dir> captures every
// walked scene as <dir>/<scene>.png.
int AcceptanceSession::startCoopWorkload()
{
    QQmlComponent component(&m_engine, QUrl(u"qrc:/acceptance/CoopAcceptance.qml"_s));
    auto *fixture = component.create();
    if (!fixture) {
        qCritical() << component.errors();
        return EXIT_FAILURE;
    }
    fixture->setParent(&m_engine);
    const auto option = [this](const QString &name) {
        const auto index = m_arguments.indexOf(name);
        return index >= 0 && index + 1 < m_arguments.size() ? m_arguments.at(index + 1) : QString{};
    };
    fixture->setProperty("scene", option(u"--coop-scene"_s));
    fixture->setProperty("consoleSurface", m_arguments.contains(u"--console"_s));
    fixture->setProperty("checkScreens", !m_arguments.contains(u"--coop-states-only"_s));
    const auto directory = option(u"--screenshot-dir"_s);
    if (!directory.isEmpty() && (!QFileInfo(directory).isAbsolute() || !QDir().mkpath(directory))) {
        qCritical("--screenshot-dir must be an absolute, writable directory");
        return EXIT_FAILURE;
    }
    QTimer::singleShot(300, this, [this, fixture, directory] {
        auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().value(0));
        if (window && m_arguments.contains(u"--smoke-coop-fullscreen"_s))
            window->showFullScreen();
        QVariant started;
        if (!window || !QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, started),
                Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) || !started.toBool()
                || m_qmlWarningOccurred) {
            qCritical("Co-op acceptance could not start");
            m_application.exit(EXIT_FAILURE);
            return;
        }
        auto *timer = new QTimer(this);
        timer->setInterval(25);
        connect(timer, &QTimer::timeout, this,
                [this, fixture, window, timer, directory, deadline = QDeadlineTimer(120000)] {
            const auto key = fixture->property("key").toInt();
            if (key != 0) {
                fixture->setProperty("key", 0);
                const auto modifiers = Qt::KeyboardModifiers(fixture->property("modifiers").toInt());
                QKeyEvent press(QEvent::KeyPress, key, modifiers);
                QKeyEvent release(QEvent::KeyRelease, key, modifiers);
                QGuiApplication::sendEvent(window, &press);
                QGuiApplication::sendEvent(window, &release);
            }
            if (const auto clickValue = fixture->property("click"); clickValue.isValid()
                    && clickValue.toPointF().x() >= 0) {
                const auto click = clickValue.toPointF();
                fixture->setProperty("click", QPointF(-1, -1));
                const auto button = fixture->property("clickButton").toInt() == Qt::RightButton
                    ? Qt::RightButton : Qt::LeftButton;
                const auto global = window->mapToGlobal(click);
                using Delivery = QWindowSystemInterface::SynchronousDelivery;
                QWindowSystemInterface::handleMouseEvent<Delivery>(window, click, global,
                    Qt::NoButton, Qt::NoButton, QEvent::MouseMove);
                QWindowSystemInterface::handleMouseEvent<Delivery>(window, click, global,
                    button, button, QEvent::MouseButtonPress);
                QWindowSystemInterface::handleMouseEvent<Delivery>(window, click, global,
                    Qt::NoButton, button, QEvent::MouseButtonRelease);
            }
            if (const auto hoverValue = fixture->property("hover"); hoverValue.isValid()
                    && hoverValue.toPointF().x() >= 0) {
                const auto hover = hoverValue.toPointF();
                fixture->setProperty("hover", QPointF(-1, -1));
                QWindowSystemInterface::handleMouseEvent<QWindowSystemInterface::SynchronousDelivery>(
                    window, hover, window->mapToGlobal(hover), Qt::NoButton, Qt::NoButton, QEvent::MouseMove);
            }
            const auto capture = fixture->property("captureName").toString();
            if (!capture.isEmpty()) {
                fixture->setProperty("captureName", QString{});
                const bool saved = directory.isEmpty()
                    || window->grabWindow().save(QDir(directory).filePath(capture + u".png"_s));
                if (!saved) {
                    timer->stop();
                    qCritical("Co-op acceptance could not save %s", qUtf8Printable(capture));
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
            }
            QVariant state;
            const bool advanced = QMetaObject::invokeMethod(fixture, "advance", Q_RETURN_ARG(QVariant, state));
            if (advanced && state.toInt() == 0 && !m_qmlWarningOccurred && !deadline.hasExpired()) return;
            timer->stop();
            bool success = advanced && state.toInt() == 1 && !m_qmlWarningOccurred;
            const auto shot = m_arguments.indexOf(u"--screenshot"_s);
            if (success && shot >= 0)
                success = shot + 1 < m_arguments.size() && QFileInfo(m_arguments.at(shot + 1)).isAbsolute()
                    && window->grabWindow().save(m_arguments.at(shot + 1));
            if (!success)
                qCritical("Co-op acceptance failed: %s", qUtf8Printable(fixture->property("failure").toString()));
            m_application.exit(success ? EXIT_SUCCESS : EXIT_FAILURE);
        });
        timer->start();
    });
    return EXIT_SUCCESS;
}
