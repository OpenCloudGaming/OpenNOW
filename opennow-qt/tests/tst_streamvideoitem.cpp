#include "StreamVideoItem.h"
#include "NativeStreamRuntime.h"

#include <QGuiApplication>
#include <QtQml/qqml.h>
#include <QQuickRhiItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>

#include <atomic>
#include <memory>

class TestRenderCallback final : public StreamVideoRenderCallback
{
public:
    void initialize(QRhi *rhi,
                    QRhiCommandBuffer *commandBuffer,
                    QRhiRenderTarget *renderTarget) override
    {
        validContext.store(rhi && commandBuffer && renderTarget);
        ++initializeCount;
    }

    void recordFrame(QRhiCommandBuffer *commandBuffer, const QRect &viewport) override
    {
        validContext.store(validContext.load() && commandBuffer);
        viewportWidth.store(viewport.width());
        viewportHeight.store(viewport.height());
        ++frameCount;
    }

    void prepareFrame(QRhiCommandBuffer *commandBuffer) override
    {
        validContext.store(validContext.load() && commandBuffer);
        ++prepareCount;
    }

    void finishFrame() override
    {
        ++finishCount;
    }

    void releaseResources() override
    {
        ++releaseCount;
    }

    std::atomic_bool validContext = false;
    std::atomic_int initializeCount = 0;
    std::atomic_int frameCount = 0;
    std::atomic_int prepareCount = 0;
    std::atomic_int finishCount = 0;
    std::atomic_int releaseCount = 0;
    std::atomic_int viewportWidth = 0;
    std::atomic_int viewportHeight = 0;
};

class StreamVideoItemTest final : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase()
    {
        registerStreamVideoItemQmlType();
    }

    void calculatesCenteredAspectFitViewport()
    {
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1920, 1080), QSize(1000, 1000)),
                 QRect(0, 219, 1000, 562));
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1000, 1000), QSize(1920, 1080)),
                 QRect(420, 0, 1080, 1080));
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1280, 720), QSize(2560, 1440)),
                 QRect(0, 0, 2560, 1440));
    }

    void handlesUnknownAndInvalidSizesPredictably()
    {
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(), QSize(640, 360)),
                 QRect(0, 0, 640, 360));
        QCOMPARE(StreamVideoItem::aspectFitRect(QSize(1920, 1080), QSize()), QRect());
    }

    void mapsScaledStreamBoundsIntoNativeClientCoordinates()
    {
        QCOMPARE(StreamVideoItem::scaledCaptureRect(
                     QRectF(80, 0, 1840, 1080), QSizeF(2000, 1080),
                     QRect(100, 50, 2500, 1350)),
                 QRect(200, 50, 2300, 1350));
        QCOMPARE(StreamVideoItem::scaledCaptureRect(
                     QRectF(), QSizeF(1920, 1080), QRect(0, 0, 1920, 1080)),
                 QRect());
    }

    void mapsAbsoluteMouseAgainstTheRenderedViewport()
    {
        QCOMPARE(StreamVideoItem::absoluteMouseCoordinates(
                     QPointF(500, 500), QSize(1920, 1080), QSizeF(1000, 1000)),
                 QRect(500, 281, 1000, 562));
        QCOMPARE(StreamVideoItem::absoluteMouseCoordinates(
                     QPointF(1920, 1080), QSize(1920, 1080), QSizeF(2560, 1440)),
                 QRect(1920, 1080, 2560, 1440));
        QCOMPARE(StreamVideoItem::absoluteMouseCoordinates(
                     QPointF(-50, 2000), QSize(1920, 1080), QSizeF(2560, 1440)),
                 QRect(0, 1439, 2560, 1440));
    }

    void parsesAndMapsRemoteCursorMetadata()
    {
        QByteArray systemCursor;
        systemCursor.append(char(0));
        systemCursor.append(char(12));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0));
        systemCursor.append(char(0x00));
        systemCursor.append(char(0x80));
        systemCursor.append(char(0xff));
        systemCursor.append(char(0xff));
        const auto system = StreamVideoItem::remoteCursorMetadata(systemCursor);
        QCOMPARE(system.imageOffset, qsizetype(7));
        QCOMPARE(system.imageLength, qsizetype(0));
        QVERIFY(system.normalizedPosition.has_value());
        QCOMPARE(*system.normalizedPosition, QPoint(32768, 65535));
        QCOMPARE(system.scale, 1.0);
        QCOMPARE(StreamVideoItem::mapRemoteCursorPosition(
                     *system.normalizedPosition, QSize(1920, 1080), QSizeF(2560, 1440)),
                 QPoint(1280, 1439));

        QByteArray scaledCursor = systemCursor;
        scaledCursor[0] = char(1);
        scaledCursor.append(char(200));
        scaledCursor.append(char(0));
        const auto scaled = StreamVideoItem::remoteCursorMetadata(scaledCursor);
        QCOMPARE(scaled.scale, 2.0);

        const auto malformed = StreamVideoItem::remoteCursorMetadata(QByteArray::fromHex("01000000000400"));
        QCOMPARE(malformed.imageOffset, qsizetype(-1));
        QVERIFY(!malformed.normalizedPosition.has_value());
    }

    void mapsQtKeyboardStateToTypedGfnInputFields()
    {
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_W), quint16(0x57));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_Escape), quint16(0x1b));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_F24), quint16(0x87));
        QCOMPARE(StreamVideoItem::windowsVirtualKey(Qt::Key_unknown), quint16(0));
        QCOMPARE(StreamVideoItem::inputModifiers(
                     Qt::ShiftModifier | Qt::ControlModifier, Qt::Key_W), quint16(0x03));
        QCOMPARE(StreamVideoItem::inputModifiers(Qt::ShiftModifier, Qt::Key_Shift), quint16(0));
    }

    void inputEnablementIsExplicitAndObservable()
    {
        StreamVideoItem item;
        QSignalSpy changes(&item, &StreamVideoItem::inputEnabledChanged);
        QVERIFY(item.inputEnabled());
        item.setInputEnabled(false);
        QVERIFY(!item.inputEnabled());
        QCOMPARE(changes.size(), 1);
        item.setInputEnabled(false);
        QCOMPARE(changes.size(), 1);
    }

    void matchesShellShortcutsWithExactModifiersAndAliases()
    {
        const QVariantMap bindings{
            {QStringLiteral("guide"), QVariantList{QStringLiteral("Ctrl+G")}},
            {QStringLiteral("request-exit"), QVariantList{QStringLiteral("Escape")}},
            {QStringLiteral("toggle-fullscreen"), QVariantList{QStringLiteral("F11")}},
            {QStringLiteral("toggle-stats"),
             QVariantList{QStringLiteral("F3"), QStringLiteral("Ctrl+N")}},
        };
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_F3, Qt::NoModifier), QStringLiteral("toggle-stats"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_N, Qt::ControlModifier), QStringLiteral("toggle-stats"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_F11, Qt::NoModifier), QStringLiteral("toggle-fullscreen"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_G, Qt::ControlModifier), QStringLiteral("guide"));
        QCOMPARE(StreamVideoItem::shortcutActionForInput(
                     bindings, Qt::Key_Escape, Qt::NoModifier), QStringLiteral("request-exit"));
        QVERIFY(StreamVideoItem::shortcutActionForInput(
                    bindings, Qt::Key_F3, Qt::ShiftModifier).isEmpty());
        QVERIFY(StreamVideoItem::shortcutActionForInput(
                    bindings, Qt::Key_G, Qt::NoModifier).isEmpty());
    }

    void shortcutBindingsAreExplicitAndObservable()
    {
        StreamVideoItem item;
        QSignalSpy changes(&item, &StreamVideoItem::shortcutBindingsChanged);
        const QVariantMap bindings{
            {QStringLiteral("toggle-pointer-lock"), QVariantList{QStringLiteral("F8")}},
        };
        item.setShortcutBindings(bindings);
        QCOMPARE(item.shortcutBindings(), bindings);
        QCOMPARE(changes.size(), 1);
        item.setShortcutBindings(bindings);
        QCOMPARE(changes.size(), 1);
    }

    void normalizesVideoSizeAndTracksCallbackAvailability()
    {
        StreamVideoItem item;
        QSignalSpy sizeChanges(&item, &StreamVideoItem::videoSizeChanged);
        QSignalSpy callbackChanges(&item, &StreamVideoItem::renderCallbackAvailableChanged);

        item.setVideoSize(QSize(-1, 1080));
        QCOMPARE(item.videoSize(), QSize());
        QCOMPARE(sizeChanges.size(), 0);

        item.setVideoSize(QSize(1920, 1080));
        QCOMPARE(item.videoSize(), QSize(1920, 1080));
        QCOMPARE(sizeChanges.size(), 1);

        const auto callback = std::make_shared<TestRenderCallback>();
        item.setRenderCallback(callback);
        QVERIFY(item.renderCallbackAvailable());
        QVERIFY(item.renderCallback() == callback);
        QCOMPARE(callbackChanges.size(), 1);

        item.setRenderCallback(callback);
        QCOMPARE(callbackChanges.size(), 1);
        item.setRenderCallback({});
        QVERIFY(!item.renderCallbackAvailable());
        QCOMPARE(callbackChanges.size(), 2);
    }

    void registersConcreteQmlSceneGraphType()
    {
        QVERIFY(qmlTypeId("OpenNOW", 1, 0, "StreamVideoItem") >= 0);
        StreamVideoItem item;
        QVERIFY(qobject_cast<QQuickRhiItem *>(&item));
        QCOMPARE(item.colorBufferFormat(), QQuickRhiItem::TextureFormat::RGBA8);
    }

    void createsRenderCallbackFromTheSharedNativeRuntime()
    {
        NativeStreamRuntime runtime;
        StreamVideoItem::setNativeStreamRuntime(&runtime);
        {
            StreamVideoItem item;
            QCOMPARE(StreamVideoItem::nativeStreamRuntime(), &runtime);
            QVERIFY(item.renderCallbackAvailable());
        }
        StreamVideoItem::setNativeStreamRuntime(nullptr);
    }

    void drivesCallbackThroughRhiSceneGraph()
    {
        if (QGuiApplication::platformName() == QStringLiteral("offscreen"))
            QSKIP("The offscreen platform plugin does not create a QRhi.");

        const auto callback = std::make_shared<TestRenderCallback>();
        {
            QQuickWindow window;
            window.resize(640, 480);
            auto *item = new StreamVideoItem(window.contentItem());
            item->setSize(QSizeF(640, 480));
            item->setVideoSize(QSize(1920, 1080));
            item->setRenderCallback(callback);
            window.show();
            item->requestFrame();

            QTRY_VERIFY_WITH_TIMEOUT(callback->initializeCount.load() > 0, 5'000);
            QTRY_VERIFY_WITH_TIMEOUT(callback->frameCount.load() > 0, 5'000);
            QVERIFY(callback->prepareCount.load() > 0);
            QTRY_VERIFY_WITH_TIMEOUT(callback->finishCount.load() > 0, 5'000);
            QVERIFY(callback->validContext.load());
            QCOMPARE(callback->viewportWidth.load(), 640);
            QCOMPARE(callback->viewportHeight.load(), 360);
        }
        QTRY_VERIFY_WITH_TIMEOUT(callback->releaseCount.load() > 0, 5'000);
    }
};

QTEST_MAIN(StreamVideoItemTest)
#include "tst_streamvideoitem.moc"
