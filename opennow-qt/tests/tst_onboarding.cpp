#include <QQmlEngine>
#include <QtQuickTest/quicktest.h>

class OnboardingTestSetup final : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        qmlRegisterType(QUrl::fromLocalFile(QStringLiteral(OPENNOW_QML_SOURCE_DIR)
            + "/state/settings/OnboardingState.qml"), "OpenNOW.OnboardingTests", 1, 0, "OnboardingState");
    }
};

QUICK_TEST_MAIN_WITH_SETUP(onboarding, OnboardingTestSetup)
#include "tst_onboarding.moc"
