#include "streaming/rendering/FlyHighNetwork.h"
#include "streaming/rendering/StreamFrameInterpolator.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QProcessEnvironment>
#include <QRegularExpression>
#include <QTemporaryDir>
#include <QTest>

#if defined(Q_OS_WIN)
#include <QGuiApplication>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>
#endif

class FlyHighPolicyTest : public QObject
{
    Q_OBJECT
private slots:
    void selection_data()
    {
        QTest::addColumn<bool>("production");
        QTest::addColumn<int>("modelFiles");
        QTest::addColumn<QString>("flag");
        QTest::addColumn<QString>("engine");
        QTest::addColumn<bool>("windowsEnabled");
        QTest::newRow("production-missing-bundle") << true << 0 << QString() << QString() << false;
        QTest::newRow("production-partial-bundle") << true << 2 << QString() << QString() << false;
        QTest::newRow("production-complete-bundle") << true << 3 << QString() << QString() << false;
        QTest::newRow("test-missing-bundle") << false << 0 << QString() << QString() << false;
        QTest::newRow("test-complete-bundle") << false << 3 << QString() << QString() << false;
        QTest::newRow("explicit-disable") << true << 3 << QStringLiteral("0") << QString() << false;
        QTest::newRow("explicit-enable-missing-bundle") << false << 0 << QStringLiteral("1") << QString() << true;
        QTest::newRow("production-explicit-enable-missing-dll") << true << 3 << QStringLiteral("1") << QString() << true;
        QTest::newRow("invalid-enable-value") << true << 3 << QStringLiteral("yes") << QString() << false;
        QTest::newRow("explicit-motion") << true << 3 << QStringLiteral("1") << QStringLiteral("motion") << false;
    }

    void selection()
    {
        QFETCH(bool, production);
        QFETCH(int, modelFiles);
        QFETCH(QString, flag);
        QFETCH(QString, engine);
        QFETCH(bool, windowsEnabled);
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        const QString appDir = directory.path() + QStringLiteral("/app");
        const QString externalDir = directory.path() + QStringLiteral("/external");
        QVERIFY(QDir().mkpath(appDir + QStringLiteral("/flyhigh")));
        QVERIFY(QDir().mkpath(externalDir));
        const QStringList files = {QStringLiteral("graph.json"), QStringLiteral("flyhighfg.json"),
                                   QStringLiteral("flyhighfg.bin")};
        for (int i = 0; i < files.size(); ++i) {
            QFile external(externalDir + u'/' + files[i]);
            QVERIFY(external.open(QIODevice::WriteOnly));
            if (i < modelFiles) {
                QFile bundled(appDir + QStringLiteral("/flyhigh/") + files[i]);
                QVERIFY(bundled.open(QIODevice::WriteOnly));
            }
        }
        QString executable = appDir + (production ? QStringLiteral("/OpenNOW") : QStringLiteral("/policy-test"));
#if defined(Q_OS_WIN)
        executable += QStringLiteral(".exe");
#endif
        QVERIFY(QFile::copy(QCoreApplication::applicationFilePath(), executable));
        auto environment = QProcessEnvironment::systemEnvironment();
        environment.insert(QStringLiteral("OPENNOW_FLYHIGH_DIR"), externalDir);
        environment.insert(QStringLiteral("OPENNOW_FLYHIGH"), flag);
        environment.insert(QStringLiteral("OPENNOW_FRAME_GENERATION_ENGINE"), engine);
        environment.insert(QStringLiteral("PATH"), QCoreApplication::applicationDirPath()
            + QDir::listSeparator() + environment.value(QStringLiteral("PATH")));
        QProcess process;
        process.setProcessEnvironment(environment);
        process.setWorkingDirectory(externalDir);
        process.start(executable, {QStringLiteral("--policy-probe")});
        QVERIFY2(process.waitForFinished(), qPrintable(process.errorString()));
        QCOMPARE(process.exitStatus(), QProcess::NormalExit);
        QCOMPARE(process.exitCode(), 0);
        const auto result = QJsonDocument::fromJson(process.readAllStandardOutput()).object();
        QVERIFY(!result.isEmpty());
#if defined(Q_OS_WIN)
        QCOMPARE(result.value(QStringLiteral("enabled")).toBool(), windowsEnabled);
        QVERIFY(!result.value(QStringLiteral("directInitialize")).toBool());
        QVERIFY(result.value(QStringLiteral("directError")).toString().contains(QStringLiteral("bridge is verified")));
        QVERIFY(result.value(QStringLiteral("rhiCreated")).toBool());
        QVERIFY(!result.value(QStringLiteral("directRhiInitialize")).toBool());
        QVERIFY(result.value(QStringLiteral("directRhiError")).toString().contains(QStringLiteral("bridge is verified")));
        QCOMPARE(result.value(QStringLiteral("firstInitialize")).toBool(), !windowsEnabled);
        QCOMPARE(result.value(QStringLiteral("secondInitialize")).toBool(), !windowsEnabled);
        if (!windowsEnabled)
            QCOMPARE(result.value(QStringLiteral("engine")).toString(), QStringLiteral("motion"));
#else
        Q_UNUSED(windowsEnabled);
        QVERIFY(!result.value(QStringLiteral("enabled")).toBool());
#endif
        const QString expectedWeights = modelFiles == 3
            ? QFileInfo(appDir).canonicalFilePath() + QStringLiteral("/flyhigh/flyhighfg.bin") : QString();
        QCOMPARE(result.value(QStringLiteral("weights")).toString(), expectedWeights);
    }

    void loadingAndBuildPolicy()
    {
        QFile network(QStringLiteral(OPENNOW_QT_SOURCE_DIR "/src/streaming/rendering/FlyHighNetwork.cpp"));
        QVERIFY(network.open(QIODevice::ReadOnly));
        const auto runtime = network.readAll();
        QVERIFY(!runtime.contains("E:/"));
        QVERIFY(!runtime.contains("OPENNOW_FLYHIGH_DIR"));
        QVERIFY(runtime.contains("LoadLibraryExW"));
        QVERIFY(runtime.contains("LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32"));
        QFile sources(QStringLiteral(OPENNOW_QT_SOURCE_DIR "/cmake/Sources.cmake"));
        QVERIFY(sources.open(QIODevice::ReadOnly));
        const auto build = sources.readAll();
        QVERIFY(!build.contains("OPENNOW_FLYHIGH_ENGINE"));
        QVERIFY(!build.contains("flyhigh.dll"));
        QVERIFY(!build.contains("flyhighfg.bin"));
    }

    void unavailableBridgeCannotCreateOrPublishFrames()
    {
        FlyHighNetwork network;
        QVERIFY(!network.initialize(nullptr));
#if defined(Q_OS_WIN)
        QVERIFY(network.error().contains(QStringLiteral("bridge is verified")));
#else
        QVERIFY(network.error().contains(QStringLiteral("Windows only")));
#endif
        QVERIFY(!network.pair(nullptr, nullptr, nullptr, 1));
        QVERIFY(!network.frame(nullptr, 1));
        QVERIFY(!network.output(1));
        QCOMPARE(network.count(), 0);
        QCOMPARE(network.networkSize(), QSize());
        QCOMPARE(network.lastGpuMs(), 0.0);
        network.release();
        QVERIFY(!network.initialize(nullptr));
    }

    void initializationFailureReleasesState()
    {
        QFile interpolator(QStringLiteral(OPENNOW_QT_SOURCE_DIR "/src/streaming/rendering/StreamFrameInterpolator.cpp"));
        QVERIFY(interpolator.open(QIODevice::ReadOnly));
        const auto failure = QRegularExpression(QStringLiteral(
            "if \\(!m_flyHigh->initialize\\(rhi\\)\\) \\{(.*?)return false;"),
            QRegularExpression::DotMatchesEverythingOption).match(QString::fromUtf8(interpolator.readAll()));
        QVERIFY(failure.hasMatch());
        QVERIFY(failure.captured(1).contains(QStringLiteral("release();")));
    }
};

int main(int argc, char **argv)
{
#if defined(Q_OS_WIN)
    QGuiApplication application(argc, argv);
#else
    QCoreApplication application(argc, argv);
#endif
    if (application.arguments().contains(QStringLiteral("--policy-probe"))) {
        QJsonObject result{{QStringLiteral("enabled"), FlyHighNetwork::enabled()},
                           {QStringLiteral("weights"), FlyHighNetwork::weightsPath()}};
#if defined(Q_OS_WIN)
        // Even direct self-test entry points must stop before loading a DLL or
        // executing the quarantined render-thread synchronization prototype.
        FlyHighNetwork direct;
        result.insert(QStringLiteral("directInitialize"), direct.initialize(nullptr));
        result.insert(QStringLiteral("directError"), direct.error());
        QRhiD3D11InitParams params;
        std::unique_ptr<QRhi> rhi(QRhi::create(QRhi::D3D11, &params, QRhi::PreferSoftwareRenderer));
        result.insert(QStringLiteral("rhiCreated"), bool(rhi));
        if (rhi) {
            FlyHighNetwork directRhi;
            result.insert(QStringLiteral("directRhiInitialize"), directRhi.initialize(rhi.get()));
            result.insert(QStringLiteral("directRhiError"), directRhi.error());
            StreamFrameInterpolator interpolator;
            result.insert(QStringLiteral("firstInitialize"), interpolator.initialize(rhi.get()));
            result.insert(QStringLiteral("secondInitialize"), interpolator.initialize(rhi.get()));
            result.insert(QStringLiteral("engine"), interpolator.engine());
        }
#endif
        QFile output;
        if (!output.open(stdout, QIODevice::WriteOnly)) return 1;
        output.write(QJsonDocument(result).toJson(QJsonDocument::Compact));
        return 0;
    }
    FlyHighPolicyTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_flyhighpolicy.moc"
