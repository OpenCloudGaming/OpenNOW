#include "AppController.h"

#include <QSignalSpy>
#include <QClipboard>
#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QtTest>

class AppControllerTest final : public QObject
{
    Q_OBJECT

private slots:
    void directLaunchAssociationIsAvailable()
    {
        AppController controller;
        QVERIFY(controller.ensureDirectLaunchAssociation());
    }

    void rejectsUnknownRoutes();
    void acceptsQtOwnedStreamOverlays();
    void overlayGuardCommitsOnlyAfterNativeHandoff();
    void closesOverlayBeforeNavigatingBack();
    void gameDetailsReturnToTheirPrimaryOrigin();
    void sessionExitDiscardsTransientRouteHistory();
    void cyclesPrimaryRoutesDeterministically();
    void cyclesGuidePagesDeterministically();
    void parsesDirectLaunchArguments();
    void clipboardReadIsBounded();
    void clipboardWriteIsBounded();
    void screenshotExportIsScoped();
};

void AppControllerTest::rejectsUnknownRoutes()
{
    AppController controller;
    QCOMPARE(controller.route(), QStringLiteral("home"));
    QVERIFY(!controller.navigate(QStringLiteral("unknown")));
    QCOMPARE(controller.route(), QStringLiteral("home"));
}

void AppControllerTest::acceptsQtOwnedStreamOverlays()
{
    AppController controller;
    QVERIFY(controller.showOverlay(QStringLiteral("stream-stats")));
    QCOMPARE(controller.overlay(), QStringLiteral("stream-stats"));
    QVERIFY(controller.showOverlay(QStringLiteral("stream-stats-expanded")));
    QCOMPARE(controller.overlay(), QStringLiteral("stream-stats-expanded"));
    QVERIFY(controller.showOverlay(QStringLiteral("desktop-stream-exit-confirm")));
    QCOMPARE(controller.overlay(), QStringLiteral("desktop-stream-exit-confirm"));
}

void AppControllerTest::overlayGuardCommitsOnlyAfterNativeHandoff()
{
    AppController controller;
    bool allowOpening = false;
    controller.setOverlayTransitionGuard(
        [&allowOpening](bool opening) { return !opening || allowOpening; });

    QVERIFY(!controller.showOverlay(QStringLiteral("stream-stats")));
    QVERIFY(controller.overlay().isEmpty());
    allowOpening = true;
    QVERIFY(controller.showOverlay(QStringLiteral("stream-stats")));
    QCOMPARE(controller.overlay(), QStringLiteral("stream-stats"));
    allowOpening = false;
    QVERIFY(controller.showOverlay({}));
    QVERIFY(controller.overlay().isEmpty());
}

void AppControllerTest::closesOverlayBeforeNavigatingBack()
{
    AppController controller;
    QVERIFY(controller.navigate(QStringLiteral("library")));
    QVERIFY(controller.showOverlay(QStringLiteral("friends")));

    QVERIFY(controller.goBack());
    QCOMPARE(controller.overlay(), QString());
    QCOMPARE(controller.route(), QStringLiteral("library"));

    QVERIFY(controller.goBack());
    QCOMPARE(controller.route(), QStringLiteral("home"));
}

void AppControllerTest::gameDetailsReturnToTheirPrimaryOrigin()
{
    AppController controller;
    QVERIFY(controller.navigateFromLastPrimary(QStringLiteral("game-detail")));
    QCOMPARE(controller.route(), QStringLiteral("game-detail"));
    QCOMPARE(controller.backRoute(), QStringLiteral("home"));

    QVERIFY(controller.goBack());
    QCOMPARE(controller.route(), QStringLiteral("home"));
}

void AppControllerTest::sessionExitDiscardsTransientRouteHistory()
{
    AppController controller;
    QVERIFY(controller.navigate(QStringLiteral("library")));
    QVERIFY(controller.navigateFromLastPrimary(QStringLiteral("game-detail")));
    QVERIFY(controller.navigate(QStringLiteral("inserting")));
    QVERIFY(controller.navigate(QStringLiteral("stream")));

    QVERIFY(controller.navigateFromLastPrimary(QStringLiteral("game-detail")));
    QCOMPARE(controller.backRoute(), QStringLiteral("library"));
    QVERIFY(controller.goBack());
    QCOMPARE(controller.route(), QStringLiteral("library"));
    QVERIFY(controller.goBack());
    QCOMPARE(controller.route(), QStringLiteral("home"));
}

void AppControllerTest::cyclesPrimaryRoutesDeterministically()
{
    AppController controller;
    const QStringList expectedRoutes{
        QStringLiteral("home"),
        QStringLiteral("library"),
        QStringLiteral("store"),
        QStringLiteral("friends"),
        QStringLiteral("controllers"),
        QStringLiteral("settings"),
    };

    QCOMPARE(controller.route(), expectedRoutes.constFirst());
    for (qsizetype index = 1; index < expectedRoutes.size(); ++index) {
        QVERIFY(controller.cyclePrimaryRoute(1));
        QCOMPARE(controller.route(), expectedRoutes.at(index));
    }
    QVERIFY(controller.cyclePrimaryRoute(1));
    QCOMPARE(controller.route(), expectedRoutes.constFirst());
    QVERIFY(controller.cyclePrimaryRoute(-1));
    QCOMPARE(controller.route(), expectedRoutes.constLast());
}

void AppControllerTest::cyclesGuidePagesDeterministically()
{
    AppController controller;
    QVERIFY(!controller.cycleGuidePage(1));
    QVERIFY(controller.showOverlay(QStringLiteral("guide-session")));
    QVERIFY(controller.cycleGuidePage(1));
    QCOMPARE(controller.overlay(), QStringLiteral("guide-controls"));
    QVERIFY(controller.cycleGuidePage(-1));
    QCOMPARE(controller.overlay(), QStringLiteral("guide-session"));
    QVERIFY(controller.cycleGuidePage(-1));
    QCOMPARE(controller.overlay(), QStringLiteral("guide-shortcuts"));
}

void AppControllerTest::parsesDirectLaunchArguments()
{
    AppController controller;
    QSignalSpy launches(&controller, &AppController::directLaunchRequested);
    QVERIFY(!controller.handleArguments({QStringLiteral("opennow")}));
    QVERIFY(controller.handleArguments({
        QStringLiteral("opennow"),
        QStringLiteral("--launch-app-id=12345"),
        QStringLiteral("--game"),
        QStringLiteral("Cyber Sample"),
    }));
    QCOMPARE(launches.count(), 1);
    QCOMPARE(launches.first().at(0).toString(), QStringLiteral("12345"));
    QCOMPARE(launches.first().at(1).toString(), QStringLiteral("Cyber Sample"));
    QVERIFY(controller.handleArguments({
        QStringLiteral("opennow"),
        QStringLiteral("--app-id"),
        QStringLiteral("not-numeric"),
        QStringLiteral("--launch-title='Quoted Game'"),
    }));
    QCOMPARE(launches.last().at(0).toString(), QString());
    QCOMPARE(launches.last().at(1).toString(), QStringLiteral("Quoted Game"));

    QVERIFY(controller.handleArguments({
        QStringLiteral("opennow"),
        QStringLiteral("opennow://launch/67890?title=Controller%20Quest"),
    }));
    QCOMPARE(launches.last().at(0).toString(), QStringLiteral("67890"));
    QCOMPARE(launches.last().at(1).toString(), QStringLiteral("Controller Quest"));

    QVERIFY(!controller.handleArguments({
        QStringLiteral("opennow"),
        QStringLiteral("https://example.com/launch/123"),
    }));
}

void AppControllerTest::clipboardReadIsBounded()
{
    AppController controller;
    QGuiApplication::clipboard()->setText(QString(70'000, u'x'));
    QCOMPARE(controller.readClipboardText().size(), 65'536);
}

void AppControllerTest::clipboardWriteIsBounded()
{
    AppController controller;
    QVERIFY(!controller.writeClipboardText({}));
    auto value = QString(70'000, u'y');
    value[100] = QChar::Null;
    QVERIFY(controller.writeClipboardText(value));
    QCOMPARE(QGuiApplication::clipboard()->text().size(), 65'535);
    QVERIFY(!QGuiApplication::clipboard()->text().contains(QChar::Null));
}

void AppControllerTest::screenshotExportIsScoped()
{
    AppController controller;
    const auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    QDir directory(pictures);
    QVERIFY(directory.mkpath(QStringLiteral("OpenNOW/Screenshots")));
    const auto source = directory.filePath(QStringLiteral("OpenNOW/Screenshots/contract-test.png"));
    const auto target = directory.filePath(QStringLiteral("OpenNOW/contract-export.png"));
    QFile file(source);
    QVERIFY(file.open(QIODevice::WriteOnly));
    QCOMPARE(file.write("fixture"), 7);
    file.close();
    QVERIFY(controller.copyScreenshotTo(source, QUrl::fromLocalFile(target).toString()));
    QVERIFY(QFileInfo::exists(target));
    QVERIFY(!controller.copyScreenshotTo(directory.filePath(QStringLiteral("OpenNOW/contract-export.png")),
                                         QUrl::fromLocalFile(source).toString()));
    QFile::remove(source);
    QFile::remove(target);
}

QTEST_MAIN(AppControllerTest)
#include "tst_appcontroller.moc"
