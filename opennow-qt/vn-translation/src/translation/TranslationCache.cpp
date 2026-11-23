#include "translation/TranslationCache.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMutexLocker>
#include <QStandardPaths>

namespace {
QString defaultCachePath()
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    return dir + QStringLiteral("/translation-cache.json");
}
} // namespace

TranslationCache::TranslationCache(int maxEntries)
    : m_filePath(defaultCachePath())
    , m_maxEntries(maxEntries > 0 ? maxEntries : DefaultMaxEntries)
{
}

TranslationCache::TranslationCache(const QString &filePath, int maxEntries)
    : m_filePath(filePath)
    , m_maxEntries(maxEntries > 0 ? maxEntries : DefaultMaxEntries)
{
}

QString TranslationCache::normalizeKey(const QString &text)
{
    // simplified(): trims and collapses all whitespace runs to single spaces.
    return text.simplified().toLower();
}

bool TranslationCache::lookup(const QString &englishText, QString &translated)
{
    QElapsedTimer timer;
    timer.start();
    const QString key = normalizeKey(englishText);
    bool hit = false;
    {
        QMutexLocker locker(&m_mutex);
        const auto it = m_entries.constFind(key);
        hit = it != m_entries.constEnd();
        if (hit)
            translated = it.value();
        m_lastLookupUs = timer.nsecsElapsed() / 1000;
    }
    return hit;
}

void TranslationCache::insert(const QString &englishText, const QString &translatedText)
{
    const QString key = normalizeKey(englishText);
    if (key.isEmpty() || translatedText.isEmpty())
        return;
    QMutexLocker locker(&m_mutex);
    if (!m_entries.contains(key))
        m_insertionOrder.append(key);
    m_entries.insert(key, translatedText);
    while (m_entries.size() > m_maxEntries && !m_insertionOrder.isEmpty()) {
        const QString oldest = m_insertionOrder.takeFirst();
        m_entries.remove(oldest);
    }
}

void TranslationCache::clear()
{
    QMutexLocker locker(&m_mutex);
    m_entries.clear();
    m_insertionOrder.clear();
}

int TranslationCache::size() const
{
    QMutexLocker locker(&m_mutex);
    return m_entries.size();
}

qint64 TranslationCache::lastLookupUs() const
{
    QMutexLocker locker(&m_mutex);
    return m_lastLookupUs;
}

bool TranslationCache::load()
{
    QFile file(m_filePath);
    if (!file.exists())
        return true; // No persisted cache yet; not an error.
    if (!file.open(QIODevice::ReadOnly))
        return false;
    const QByteArray data = file.readAll();
    file.close();
    const QJsonDocument doc = QJsonDocument::fromJson(data);
    if (!doc.isObject())
        return false;
    const QJsonObject entries = doc.object().value(QStringLiteral("entries")).toObject();
    QMutexLocker locker(&m_mutex);
    m_entries.clear();
    m_insertionOrder.clear();
    for (auto it = entries.constBegin(); it != entries.constEnd(); ++it) {
        const QString key = normalizeKey(it.key());
        if (key.isEmpty())
            continue;
        if (!m_entries.contains(key))
            m_insertionOrder.append(key);
        m_entries.insert(key, it.value().toString());
    }
    while (m_entries.size() > m_maxEntries && !m_insertionOrder.isEmpty())
        m_entries.remove(m_insertionOrder.takeFirst());
    return true;
}

bool TranslationCache::save() const
{
    QJsonObject entries;
    {
        QMutexLocker locker(&m_mutex);
        for (auto it = m_entries.constBegin(); it != m_entries.constEnd(); ++it)
            entries.insert(it.key(), it.value());
    }
    QJsonObject root;
    root.insert(QStringLiteral("version"), 1);
    root.insert(QStringLiteral("entries"), entries);
    const QJsonDocument doc(root);

    const QFileInfo info(m_filePath);
    const QDir dir(info.absolutePath());
    if (!dir.mkpath(QStringLiteral(".")))
        return false;
    QFile file(m_filePath);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    file.write(doc.toJson(QJsonDocument::Compact));
    return file.flush();
}
