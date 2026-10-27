#include <QGuiApplication>
#include <QTimer>
#include <QWaylandCompositor>
#include <QWaylandOutput>
#include <QWaylandOutputMode>
#include <QWaylandXdgDecorationManagerV1>
#include <QWaylandXdgShell>

#include <cstdio>

int main(int argc, char **argv)
{
    QGuiApplication application(argc, argv);
    QWaylandCompositor compositor;
    compositor.setSocketName("opennow-windowtheme-test");
    compositor.setUseHardwareIntegrationExtension(false);
    QWaylandXdgShell shell(&compositor);
    QObject::connect(&shell, &QWaylandXdgShell::toplevelCreated, &application,
                     [](QWaylandXdgToplevel *toplevel) {
        QObject::connect(toplevel, &QWaylandXdgToplevel::setFullscreen, toplevel, [toplevel] {
            toplevel->sendFullscreen(QSize(1280, 720));
        });
        QObject::connect(toplevel, &QWaylandXdgToplevel::unsetFullscreen, toplevel, [toplevel] {
            toplevel->sendUnmaximized(QSize(640, 480));
        });
    });
    QWaylandOutput output(&compositor, nullptr);
    const QWaylandOutputMode mode(QSize(1280, 720), 60000);
    output.addMode(mode, true);
    output.setCurrentMode(mode);
    QWaylandXdgDecorationManagerV1 decorations;
    decorations.setExtensionContainer(&compositor);
    decorations.setPreferredMode(application.arguments().contains("--server-decorations")
        ? QWaylandXdgToplevel::ServerSideDecoration : QWaylandXdgToplevel::ClientSideDecoration);
    compositor.create();
    if (!application.arguments().contains("--no-decoration-manager")) decorations.initialize();
    std::puts("READY");
    std::fflush(stdout);
    QTimer::singleShot(15000, &application, &QCoreApplication::quit);
    return application.exec();
}
