#include "streaming/rendering/WindowsHdrDisplay.h"

#include <QtTest>
#include <limits>

class WindowsHdrDisplayTests final : public QObject
{
    Q_OBJECT

private slots:
    void acceptsValidDisplayAndOptionalMetadata()
    {
        const HdrChromaticity primaries{0.68, 0.32, 0.265, 0.69,
                                        0.15, 0.06, 0.3127, 0.329};
        const auto display = validatedWindowsHdrDisplay(12, 0.005, 1000, 600, primaries);
        QVERIFY(display);
        QCOMPARE(display->minimumNits, 0.005);
        QCOMPARE(display->maximumNits, 1000.0);
        QVERIFY(display->maximumFullFrameNits);
        QCOMPARE(*display->maximumFullFrameNits, 600.0);
        QVERIFY(display->chromaticity);
        QVERIFY(*display->chromaticity == primaries);

        const auto missing = validatedWindowsHdrDisplay(12, 0, 400, 0, {});
        QVERIFY(missing);
        QVERIFY(!missing->maximumFullFrameNits);
        QVERIFY(!missing->chromaticity);
    }

    void rejectsInactiveOrInvalidLuminance()
    {
        const HdrChromaticity primaries{0.68, 0.32, 0.265, 0.69,
                                        0.15, 0.06, 0.3127, 0.329};
        const double nan = std::numeric_limits<double>::quiet_NaN();
        const double infinity = std::numeric_limits<double>::infinity();
        QVERIFY(!validatedWindowsHdrDisplay(0, 0, 1000, 600, primaries));
        QVERIFY(!validatedWindowsHdrDisplay(12, -1, 1000, 600, primaries));
        QVERIFY(!validatedWindowsHdrDisplay(12, nan, 1000, 600, primaries));
        QVERIFY(!validatedWindowsHdrDisplay(12, 1000, 1000, 600, primaries));
        QVERIFY(!validatedWindowsHdrDisplay(12, 0, infinity, 600, primaries));
        QVERIFY(!validatedWindowsHdrDisplay(12, 0, 10001, 600, primaries));
        for (double invalid : {-1.0, nan, 1001.0}) {
            const auto display = validatedWindowsHdrDisplay(12, 0, 1000, invalid, primaries);
            QVERIFY(display);
            QVERIFY(!display->maximumFullFrameNits);
            QVERIFY(!display->chromaticity);
        }
    }

    void dropsInvalidChromaticityWithoutInventingValues()
    {
        const HdrChromaticity degenerate{0.1, 0.1, 0.2, 0.2,
                                         0.3, 0.3, 0.25, 0.25};
        const auto invalid = validatedWindowsHdrDisplay(12, 0.005, 1000, 600, degenerate);
        QVERIFY(invalid);
        QVERIFY(!invalid->maximumFullFrameNits);
        QVERIFY(!invalid->chromaticity);
        const auto nan = std::numeric_limits<double>::quiet_NaN();
        const HdrChromaticity nonfinite{0.68, 0.32, 0.265, 0.69,
                                        0.15, 0.06, nan, 0.329};
        QVERIFY(!validatedWindowsHdrDisplay(12, 0, 1000, 600, nonfinite)->chromaticity);
        const HdrChromaticity outOfRange{0.68, 0.32, 0.265, 0.69,
                                         1.1, 0.06, 0.3127, 0.329};
        QVERIFY(!validatedWindowsHdrDisplay(12, 0, 1000, 600, outOfRange)->chromaticity);
        const HdrChromaticity impossible{0.68, 0.32, 0.265, 0.69,
                                         0.15, 0.06, 0.9, 0.329};
        QVERIFY(!validatedWindowsHdrDisplay(12, 0, 1000, 600, impossible)->chromaticity);
    }
};

QTEST_GUILESS_MAIN(WindowsHdrDisplayTests)
#include "tst_windowshdrdisplay.moc"
