#include "Localization.h"

#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QLocale>
#include <QRegularExpression>

using namespace Qt::StringLiterals;

Localization::Localization(QObject *parent)
    : QTranslator(parent)
    , m_availableLocales(QDir(u":/locales"_s).entryList({u"*.json"_s}, QDir::Files))
    , m_fallback(loadLocale(u"en"_s))
{
    for (auto &locale : m_availableLocales) {
        locale.chop(5);
    }
    m_availableLocales.sort();
    if (!m_availableLocales.contains(u"en"_s)) {
        m_availableLocales.push_front(u"en"_s);
    }
    auto fallbackKeys = m_fallback.keys();
    fallbackKeys.sort();
    for (const auto &key : fallbackKeys) {
        const auto source = m_fallback.value(key);
        if (!m_sourceToKey.contains(source)) {
            m_sourceToKey.insert(source, key);
        }
    }
    setLocale(u"system"_s);
}

QString Localization::locale() const { return m_locale; }
QString Localization::effectiveLocale() const { return m_effectiveLocale; }
QStringList Localization::availableLocales() const { return m_availableLocales; }
quint64 Localization::revision() const { return m_revision; }
bool Localization::isEmpty() const { return false; }

QString Localization::translate(const char *, const char *sourceText, const char *, int) const
{
    if (!sourceText) return {};
    const auto source = QString::fromUtf8(sourceText);
    const auto key = m_sourceToKey.value(source);
    if (key.isEmpty()) return source;
    return m_active.value(key, m_fallback.value(key, source));
}

void Localization::setLocale(const QString &locale)
{
    auto requested = locale.trimmed().toLower().replace(u'_', u'-');
    if (requested.isEmpty()) requested = u"system"_s;
    auto effective = requested == u"system"_s
        ? normalizeLocale(QLocale::system().name())
        : normalizeLocale(requested);
    if (!m_availableLocales.contains(effective)) effective = u"en"_s;
    if (m_locale == requested && m_effectiveLocale == effective && !m_active.isEmpty()) return;
    m_locale = requested;
    m_effectiveLocale = effective;
    m_active = effective == u"en"_s ? m_fallback : loadLocale(effective);
    ++m_revision;
    emit localeChanged();
}

QString Localization::source(const QString &sourceText) const
{
    return source(sourceText, m_revision);
}

QString Localization::source(const QString &sourceText, quint64) const
{
    const auto key = m_sourceToKey.value(sourceText);
    return key.isEmpty() ? sourceText : m_active.value(key, m_fallback.value(key, sourceText));
}

QString Localization::text(const QString &key, const QVariantMap &values) const
{
    const auto resolvedKey = values.value(u"count"_s).isValid()
            && values.value(u"count"_s).toDouble() != 1.0
        ? key + u"_plural"_s
        : key;
    auto value = m_active.value(resolvedKey);
    if (value.isEmpty()) value = m_active.value(key);
    if (value.isEmpty()) value = m_fallback.value(resolvedKey);
    if (value.isEmpty()) value = m_fallback.value(key, key);
    return interpolate(value, values);
}

QString Localization::normalizeLocale(const QString &locale)
{
    const auto normalized = locale.trimmed().toLower().replace(u'_', u'-');
    return normalized.section(u'-', 0, 0).isEmpty() ? u"en"_s : normalized.section(u'-', 0, 0);
}

void Localization::flatten(const QJsonObject &object,
                           const QString &prefix,
                           QHash<QString, QString> *target)
{
    for (auto iterator = object.begin(); iterator != object.end(); ++iterator) {
        const auto key = prefix.isEmpty() ? iterator.key() : prefix + u'.' + iterator.key();
        if (iterator->isString()) {
            target->insert(key, iterator->toString());
        } else if (iterator->isObject()) {
            flatten(iterator->toObject(), key, target);
        }
    }
}

QHash<QString, QString> Localization::loadLocale(const QString &locale)
{
    QFile file(u":/locales/%1.json"_s.arg(locale));
    if (!file.open(QIODevice::ReadOnly) || file.size() > 2 * 1024 * 1024) return {};
    const auto document = QJsonDocument::fromJson(file.readAll());
    QHash<QString, QString> flattened;
    if (document.isObject()) flatten(document.object(), {}, &flattened);
    return flattened;
}

QString Localization::interpolate(QString value, const QVariantMap &values)
{
    static const QRegularExpression placeholder(uR"(\{\{\s*([\w.]+)\s*\}\})"_s);
    auto match = placeholder.match(value);
    while (match.hasMatch()) {
        const auto token = match.captured(1);
        const auto replacement = values.value(token);
        if (replacement.isValid()) {
            value.replace(match.capturedStart(), match.capturedLength(), replacement.toString());
        }
        match = placeholder.match(value, match.capturedStart() + 1);
    }
    return value;
}
