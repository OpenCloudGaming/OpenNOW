#include "OpnQtAppWindow.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QString>
#include <QtWidgets/QApplication>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QCoreApplication::setOrganizationName(QStringLiteral("OpenNOW"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("opennow.local"));
    QCoreApplication::setApplicationName(QStringLiteral("OpenNOW Qt"));
    QCoreApplication::setApplicationVersion(QStringLiteral("0.1.0"));

    OpnQt::AppWindow window;
    window.show();

    return QApplication::exec();
}
