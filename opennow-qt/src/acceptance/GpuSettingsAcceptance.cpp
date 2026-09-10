#include "acceptance/AcceptanceSession.h"

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>

#include <cstdlib>

using namespace Qt::StringLiterals;

int AcceptanceSession::startGpuSettingsWorkload()
{
    QQmlComponent component(&m_engine, QUrl(u"qrc:/acceptance/GpuSettingsAcceptance.qml"_s));
    auto *fixture = component.create();
    if (!fixture) {
        qCritical() << component.errors();
        return EXIT_FAILURE;
    }
    fixture->setParent(&m_engine);
    QTimer::singleShot(500, this, [this, fixture] {
        auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().value(0));
        QVariant passed;
        const bool valid = window && QMetaObject::invokeMethod(fixture, "run", Q_RETURN_ARG(QVariant, passed),
            Q_ARG(QVariant, QVariant::fromValue(window->contentItem()))) && passed.toBool();
        if (!valid || m_qmlWarningOccurred) {
            m_application.exit(EXIT_FAILURE);
            return;
        }
        QTimer::singleShot(250, this, [this, window] {
            const auto screenshot = m_arguments.indexOf(u"--screenshot"_s);
            const bool saved = screenshot < 0 || (screenshot + 1 < m_arguments.size()
                && window->grabWindow().save(m_arguments.at(screenshot + 1)));
            m_application.exit(saved && !m_qmlWarningOccurred ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    });
    return EXIT_SUCCESS;
}
