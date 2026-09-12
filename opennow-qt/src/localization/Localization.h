#pragma once

#include <QHash>
#include <QJsonObject>
#include <QStringList>
#include <QTranslator>
#include <QVariantMap>

class Localization final : public QTranslator
{
    Q_OBJECT
    Q_PROPERTY(QString locale READ locale WRITE setLocale NOTIFY localeChanged)
    Q_PROPERTY(QString effectiveLocale READ effectiveLocale NOTIFY localeChanged)
    Q_PROPERTY(QStringList availableLocales READ availableLocales CONSTANT)
    Q_PROPERTY(quint64 revision READ revision NOTIFY localeChanged)

public:
    explicit Localization(QObject *parent = nullptr);

    [[nodiscard]] QString locale() const;
    [[nodiscard]] QString effectiveLocale() const;
    [[nodiscard]] QStringList availableLocales() const;
    [[nodiscard]] quint64 revision() const;
    [[nodiscard]] bool isEmpty() const override;
    [[nodiscard]] QString translate(const char *context,
                                    const char *sourceText,
                                    const char *disambiguation = nullptr,
                                    int n = -1) const override;

    Q_INVOKABLE void setLocale(const QString &locale);
    Q_INVOKABLE QString source(const QString &sourceText) const;
    Q_INVOKABLE QString source(const QString &sourceText, quint64 revision) const;
    Q_INVOKABLE QString text(const QString &key, const QVariantMap &values = {}) const;

signals:
    void localeChanged();

private:
    static QString normalizeLocale(const QString &locale);
    static void flatten(const QJsonObject &object,
                        const QString &prefix,
                        QHash<QString, QString> *target);
    static QHash<QString, QString> loadLocale(const QString &locale);
    static QString interpolate(QString value, const QVariantMap &values);

    QStringList m_availableLocales;
    QHash<QString, QString> m_fallback;
    QHash<QString, QString> m_active;
    QHash<QString, QString> m_sourceToKey;
    QString m_locale = QStringLiteral("system");
    QString m_effectiveLocale = QStringLiteral("en");
    quint64 m_revision = 0;
};
