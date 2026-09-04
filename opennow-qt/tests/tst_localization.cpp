#include "Localization.h"

#include <QDirIterator>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSet>
#include <QTest>

class LocalizationTest final : public QObject
{
    Q_OBJECT

private slots:
    void loadsAvailableLocalesAndUsesEnglishFallback()
    {
        Localization localization;
        QVERIFY(localization.availableLocales().contains(QStringLiteral("en")));
        QVERIFY(localization.availableLocales().contains(QStringLiteral("de")));

        localization.setLocale(QStringLiteral("unsupported-region"));
        QCOMPARE(localization.locale(), QStringLiteral("unsupported-region"));
        QCOMPARE(localization.effectiveLocale(), QStringLiteral("en"));
        QCOMPARE(localization.translate(nullptr, "Back"), QStringLiteral("Back"));
    }

    void translatesSourceTextAndInterpolatesKeyedValues()
    {
        Localization localization;
        localization.setLocale(QStringLiteral("de-DE"));
        QCOMPARE(localization.effectiveLocale(), QStringLiteral("de"));
        QCOMPARE(localization.translate(nullptr, "Back"), QStringLiteral("Zurück"));

        localization.setLocale(QStringLiteral("en"));
        QCOMPARE(localization.text(QStringLiteral("library.gameCount"), {{QStringLiteral("count"), 2}}),
                 QStringLiteral("2 games"));
    }

    void everySupportedLocaleLoadsWithFallback()
    {
        Localization localization;
        auto revision = localization.revision();
        for (const auto &locale : localization.availableLocales()) {
            localization.setLocale(locale);
            QCOMPARE(localization.effectiveLocale(), locale);
            QVERIFY(!localization.translate(nullptr, "Back").isEmpty());
            QVERIFY(localization.revision() > revision);
            revision = localization.revision();
        }
    }

    void everyExtractedQmlSourceExistsInEnglishCatalog()
    {
        QFile catalog(QStringLiteral(OPENNOW_SOURCE_DIR "/locales/en.json"));
        QVERIFY(catalog.open(QIODevice::ReadOnly));
        const auto document = QJsonDocument::fromJson(catalog.readAll());
        QVERIFY(document.isObject());
        QSet<QString> englishSources;
        const auto collect = [&englishSources](const auto &self, const QJsonObject &object) -> void {
            for (auto iterator = object.begin(); iterator != object.end(); ++iterator) {
                if (iterator->isString()) englishSources.insert(iterator->toString());
                else if (iterator->isObject()) self(self, iterator->toObject());
            }
        };
        collect(collect, document.object());

        const QRegularExpression extracted(
            QStringLiteral("qsTr\\((\"(?:\\\\.|[^\"\\\\])*\")\\)"));
        QDirIterator files(QStringLiteral(OPENNOW_SOURCE_DIR "/opennow-qt/qml"),
                           {QStringLiteral("*.qml")}, QDir::Files,
                           QDirIterator::Subdirectories);
        QStringList missing;
        while (files.hasNext()) {
            QFile file(files.next());
            QVERIFY(file.open(QIODevice::ReadOnly));
            const auto contents = QString::fromUtf8(file.readAll());
            auto matches = extracted.globalMatch(contents);
            while (matches.hasNext()) {
                const auto literal = matches.next().captured(1).toUtf8();
                const auto decoded = QJsonDocument::fromJson("[" + literal + "]");
                QVERIFY(decoded.isArray() && !decoded.array().isEmpty());
                const auto source = decoded.array().first().toString();
                if (!source.isEmpty() && !englishSources.contains(source)) {
                    missing.push_back(file.fileName() + QStringLiteral(": ") + source);
                }
            }
        }
        QVERIFY2(missing.isEmpty(), qPrintable(missing.join(u'\n')));
    }
};

QTEST_GUILESS_MAIN(LocalizationTest)
#include "tst_localization.moc"
