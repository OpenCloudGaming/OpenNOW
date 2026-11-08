#include "streaming/rendering/FrameGenerationBenchmark.h"
#include "streaming/rendering/FlyHighNetwork.h"

#include <QTest>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QVariantList>
#include <atomic>
#include <limits>
#include <memory>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>

// evaluate() is a pure decision; measure() runs the real engine on a private QRhi.
class FrameGenerationBenchmarkTest : public QObject
{
    Q_OBJECT
private slots:
    void unsupportedRequestedEngineIsRejected_data()
    {
        QTest::addColumn<QString>("engine");
        QTest::newRow("flyhigh") << QStringLiteral("flyhigh");
        QTest::newRow("unknown") << QStringLiteral("unknown");
        QTest::newRow("empty") << QString();
    }

    void unsupportedRequestedEngineIsRejected()
    {
        QFETCH(QString, engine);
        const auto result = FrameGenerationBenchmark::measure(
            {{QStringLiteral("engine"), engine}, {QStringLiteral("width"), 64},
             {QStringLiteral("height"), 64}}, nullptr, {}, std::numeric_limits<quint64>::max());
        QVERIFY(!result.value(QStringLiteral("ok")).toBool());
        QCOMPARE(result.value(QStringLiteral("error")).toString(),
                 QStringLiteral("Unsupported frame-generation benchmark engine: %1. Select motion.").arg(engine));
        QVERIFY(!result.contains(QStringLiteral("engine")));
    }

    void explicitFlyHighRefusalDoesNotBlameTheGraphicsAdapter()
    {
#ifdef Q_OS_WIN
        const bool hadFlyHigh = qEnvironmentVariableIsSet("OPENNOW_FLYHIGH");
        const auto previousFlyHigh = qgetenv("OPENNOW_FLYHIGH");
        const bool hadEngine = qEnvironmentVariableIsSet("OPENNOW_FRAME_GENERATION_ENGINE");
        const auto previousEngine = qgetenv("OPENNOW_FRAME_GENERATION_ENGINE");
        const auto restore = qScopeGuard([&] {
            if (hadFlyHigh) qputenv("OPENNOW_FLYHIGH", previousFlyHigh);
            else qunsetenv("OPENNOW_FLYHIGH");
            if (hadEngine) qputenv("OPENNOW_FRAME_GENERATION_ENGINE", previousEngine);
            else qunsetenv("OPENNOW_FRAME_GENERATION_ENGINE");
        });
        qputenv("OPENNOW_FLYHIGH", "1");
        qunsetenv("OPENNOW_FRAME_GENERATION_ENGINE");
        const auto result = FrameGenerationBenchmark::measure(
            {{QStringLiteral("width"), 64}, {QStringLiteral("height"), 64}},
            nullptr, {}, std::numeric_limits<quint64>::max());
        QVERIFY(!result.value(QStringLiteral("ok")).toBool());
        QCOMPARE(result.value(QStringLiteral("error")).toString(), FlyHighNetwork::unavailableReason());
        QVERIFY(result.value(QStringLiteral("error")).toString().contains(QStringLiteral("OPENNOW_FLYHIGH")));
#else
        QSKIP("FlyHigh selection is Windows-only");
#endif
    }

    void selectedAdapterIsUsedAndMismatchesFailClosed()
    {
#ifdef Q_OS_WIN
        const bool hadOverride = qEnvironmentVariableIsSet("QT_D3D_ADAPTER_INDEX");
        const auto previousOverride = qgetenv("QT_D3D_ADAPTER_INDEX");
        const auto restore = qScopeGuard([&] {
            if (hadOverride) qputenv("QT_D3D_ADAPTER_INDEX", previousOverride);
            else qunsetenv("QT_D3D_ADAPTER_INDEX");
        });
        qputenv("QT_D3D_ADAPTER_INDEX", "0");
        QRhiD3D11InitParams params;
        const auto device = std::unique_ptr<QRhi>(QRhi::create(QRhi::D3D11, &params));
        if (!device) QSKIP("D3D11 is unavailable");
        const auto *handles = static_cast<const QRhiD3D11NativeHandles *>(device->nativeHandles());
        QVERIFY(handles);
        const quint64 luid = (quint64(quint32(handles->adapterLuidHigh)) << 32) | handles->adapterLuidLow;
        QVERIFY(luid);
        const auto name = QString::fromUtf8(device->driverInfo().deviceName).trimmed();
        FrameGenerationBenchmark selected(nullptr, luid);
        QCOMPARE(selected.gpuName(), name);
        const QVariantMap request{{QStringLiteral("width"), 64}, {QStringLiteral("height"), 64},
                                  {QStringLiteral("seconds"), 0.5}};
        const auto measured = FrameGenerationBenchmark::measure(request, nullptr, {}, luid);
        QVERIFY2(measured.value(QStringLiteral("ok")).toBool(),
                 qPrintable(measured.value(QStringLiteral("error")).toString()));
        QCOMPARE(measured.value(QStringLiteral("gpu")).toString().trimmed(), name);
        const quint64 different = luid == 1 ? 2 : luid ^ 1;
        QVERIFY(different);
        FrameGenerationBenchmark mismatched(nullptr, different);
        QVERIFY(mismatched.gpuName().isEmpty());
        const auto rejected = FrameGenerationBenchmark::measure(request, nullptr, {}, different);
        QVERIFY(!rejected.value(QStringLiteral("ok")).toBool());
        QVERIFY(rejected.value(QStringLiteral("error")).toString().contains(QStringLiteral("selected graphics adapter")));
        QSignalSpy finished(&mismatched, &FrameGenerationBenchmark::finished);
        QVERIFY(mismatched.start(request));
        QTRY_COMPARE(finished.count(), 1);
        const auto asynchronous = finished.first().first().toMap();
        QVERIFY(!asynchronous.value(QStringLiteral("ok")).toBool());
        QVERIFY(asynchronous.value(QStringLiteral("error")).toString().contains(QStringLiteral("selected graphics adapter")));
#else
        QSKIP("Selected-adapter acceptance requires Windows D3D11");
#endif
    }

    void rejectsIncompleteMeasurements_data()
    {
        QTest::addColumn<QVariantMap>("measurement");
        const QVariantMap complete{{QStringLiteral("ok"), true}, {QStringLiteral("frames"), 30},
                                  {QStringLiteral("p95Ms"), 1.0}, {QStringLiteral("engineMaxFactor"), 4}};
        for (const auto &field : {QStringLiteral("frames"), QStringLiteral("p95Ms")}) {
            auto missing = complete;
            missing.remove(field);
            QTest::newRow(qPrintable(field + QStringLiteral("-missing"))) << missing;
            for (double value : {0.0, -1.0, std::numeric_limits<double>::quiet_NaN(),
                                 std::numeric_limits<double>::infinity()}) {
                auto invalid = complete;
                invalid.insert(field, value);
                QTest::newRow(qPrintable(field + QString::number(value))) << invalid;
            }
        }
        auto insufficient = complete;
        insufficient.insert(QStringLiteral("frames"), 9);
        QTest::newRow("insufficient-samples") << insufficient;
    }

    void rejectsIncompleteMeasurements()
    {
        QFETCH(QVariantMap, measurement);
        const auto verdict = FrameGenerationBenchmark::evaluate(measurement, 60, 240.0);
        QVERIFY(verdict.value(QStringLiteral("allowed")).toList().isEmpty());
        const auto reasons = verdict.value(QStringLiteral("reasons")).toMap();
        for (const auto &factor : {QStringLiteral("2"), QStringLiteral("3"), QStringLiteral("4")})
            QCOMPARE(reasons.value(factor).toString(), QStringLiteral("untested"));
    }

    void gpuIdentityCanBeResolvedAndDestroyed()
    {
        FrameGenerationBenchmark benchmark;
        const auto name = benchmark.gpuName();
        if (name.isEmpty()) QSKIP("No GPU backend available for identity lookup");
        QCOMPARE(benchmark.gpuName(), name);
    }

    void allowsOnlyFactorsTheBudgetAndDisplayAfford()
    {
        QVariantMap measurement{{QStringLiteral("ok"), true}, {QStringLiteral("p95Ms"), 2.5},
                                {QStringLiteral("frames"), 30},
                                {QStringLiteral("engineMaxFactor"), 4}};
        // 60 FPS: 16.67 ms interval, 10 ms budget -> 2x (2.5) and 3x (5.0) and 4x (7.5) fit the GPU;
        // the display decides: 120 Hz allows 2x only, 180 Hz adds 3x, 240 Hz adds 4x.
        auto verdict = FrameGenerationBenchmark::evaluate(measurement, 60, 120.0);
        QCOMPARE(verdict.value(QStringLiteral("allowed")).toList(), QVariantList{2});
        QCOMPARE(verdict.value(QStringLiteral("reasons")).toMap().value(QStringLiteral("3")).toString(),
                 QStringLiteral("display"));
        verdict = FrameGenerationBenchmark::evaluate(measurement, 60, 180.0);
        QCOMPARE(verdict.value(QStringLiteral("allowed")).toList(), (QVariantList{2, 3}));
        verdict = FrameGenerationBenchmark::evaluate(measurement, 60, 240.0);
        QCOMPARE(verdict.value(QStringLiteral("allowed")).toList(), (QVariantList{2, 3, 4}));
        // Unknown refresh skips the display gate; the GPU budget still rules.
        measurement.insert(QStringLiteral("p95Ms"), 6.0);
        verdict = FrameGenerationBenchmark::evaluate(measurement, 60, 0.0);
        QCOMPARE(verdict.value(QStringLiteral("allowed")).toList(), QVariantList{2});
        QCOMPARE(verdict.value(QStringLiteral("reasons")).toMap().value(QStringLiteral("3")).toString(),
                 QStringLiteral("gpu"));
        // A 120 FPS source halves the budget: 6 ms per generated frame no longer fits even 2x.
        verdict = FrameGenerationBenchmark::evaluate(measurement, 120, 360.0);
        QVERIFY(verdict.value(QStringLiteral("allowed")).toList().isEmpty());
        // The block-matching engine only synthesises midpoints.
        measurement.insert(QStringLiteral("p95Ms"), 1.0);
        measurement.insert(QStringLiteral("engineMaxFactor"), 2);
        verdict = FrameGenerationBenchmark::evaluate(measurement, 60, 360.0);
        QCOMPARE(verdict.value(QStringLiteral("allowed")).toList(), QVariantList{2});
        QCOMPARE(verdict.value(QStringLiteral("reasons")).toMap().value(QStringLiteral("4")).toString(),
                 QStringLiteral("engine"));
        // Without a measurement nothing is allowed.
        verdict = FrameGenerationBenchmark::evaluate({}, 60, 240.0);
        QVERIFY(verdict.value(QStringLiteral("allowed")).toList().isEmpty());
        // Measured per-level costs replace the (N - 1) extrapolation: the flow is shared,
        // so 4x costs far less than three 2x passes here.
        const QVariantMap measured{{QStringLiteral("ok"), true}, {QStringLiteral("p95Ms"), 2.5},
                                   {QStringLiteral("frames"), 30},
                                   {QStringLiteral("p95Ms3"), 3.0}, {QStringLiteral("p95Ms4"), 3.5},
                                   {QStringLiteral("engineMaxFactor"), 4}};
        verdict = FrameGenerationBenchmark::evaluate(measured, 120, 480.0);
        QCOMPARE(verdict.value(QStringLiteral("allowed")).toList(), (QVariantList{2, 3, 4}));
        QCOMPARE(verdict.value(QStringLiteral("costMs")).toMap().value(QStringLiteral("4")).toDouble(), 3.5);
        verdict = FrameGenerationBenchmark::evaluate(measured, 240, 960.0);
        QCOMPARE(verdict.value(QStringLiteral("reasons")).toMap().value(QStringLiteral("4")).toString(),
                 QStringLiteral("gpu"));
    }

    void measuresTheEngineOnThisGpu()
    {
        std::atomic_bool cancelled = false;
        double lastProgress = 0.0;
        const auto result = FrameGenerationBenchmark::measure(
            {{QStringLiteral("engine"), QStringLiteral("motion")}, {QStringLiteral("width"), 640},
             {QStringLiteral("height"), 360}, {QStringLiteral("factor"), 2}, {QStringLiteral("seconds"), 1.0}},
            &cancelled, [&lastProgress](double value) { lastProgress = value; });
        if (!result.value(QStringLiteral("ok")).toBool())
            QSKIP(qPrintable(QStringLiteral("GPU benchmark unavailable here: ")
                             + result.value(QStringLiteral("error")).toString()));
        QVERIFY(result.value(QStringLiteral("frames")).toInt() >= 10);
        QCOMPARE(result.value(QStringLiteral("engine")).toString(), QStringLiteral("motion"));
        QVERIFY(result.value(QStringLiteral("p50Ms")).toDouble() > 0.0);
        QVERIFY(result.value(QStringLiteral("p95Ms")).toDouble() >= result.value(QStringLiteral("p50Ms")).toDouble());
        QVERIFY(result.value(QStringLiteral("maxMs")).toDouble() >= result.value(QStringLiteral("p95Ms")).toDouble());
        QVERIFY(!result.value(QStringLiteral("gpu")).toString().isEmpty());
        QCOMPARE(result.value(QStringLiteral("engineMaxFactor")).toInt(), 4);
        QVERIFY(result.value(QStringLiteral("p95Ms3")).toDouble() > 0.0);
        QVERIFY(result.value(QStringLiteral("p95Ms4")).toDouble() > 0.0);
        QVERIFY(lastProgress > 0.0);
        qInfo() << "capability test:" << result.value(QStringLiteral("gpu")).toString()
                << result.value(QStringLiteral("backend")).toString()
                << "p50" << result.value(QStringLiteral("p50Ms")).toDouble() << "ms"
                << "p95 2x" << result.value(QStringLiteral("p95Ms")).toDouble() << "ms"
                << "3x" << result.value(QStringLiteral("p95Ms3")).toDouble() << "ms"
                << "4x" << result.value(QStringLiteral("p95Ms4")).toDouble() << "ms"
                << "gpuTimed" << result.value(QStringLiteral("gpuTimed")).toBool();
    }

    void cancellationStopsTheMeasurement()
    {
        std::atomic_bool cancelled = true;
        const auto result = FrameGenerationBenchmark::measure(
            {{QStringLiteral("width"), 320}, {QStringLiteral("height"), 180}}, &cancelled, {});
        QVERIFY(!result.value(QStringLiteral("ok")).toBool());
    }
};

QTEST_MAIN(FrameGenerationBenchmarkTest)
#include "tst_framegenerationbenchmark.moc"
