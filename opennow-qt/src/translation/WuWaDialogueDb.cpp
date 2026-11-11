#include "translation/WuWaDialogueDb.h"
#include "translation/TranslationCache.h"

#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMutexLocker>

WuWaDialogueDb::WuWaDialogueDb(const QString &filePath)
    : m_filePath(filePath)
{
}

bool WuWaDialogueDb::loadDialogueDb(const QString &path)
{
    m_filePath = path;
    return loadDialogueDb();
}

bool WuWaDialogueDb::loadDialogueDb()
{
    if (m_filePath.isEmpty())
        return true; // No DB configured; empty DB, not an error.
    QFile file(m_filePath);
    if (!file.exists())
        return true; // Seed file not installed yet; DB stays empty.
    if (!file.open(QIODevice::ReadOnly))
        return false;
    const QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
    file.close();
    if (!doc.isObject())
        return false;
    const QJsonObject entries = doc.object().value(QStringLiteral("entries")).toObject();
    QMutexLocker locker(&m_mutex);
    m_entries.clear();
    for (auto it = entries.constBegin(); it != entries.constEnd(); ++it) {
        const QString key = TranslationCache::normalizeKey(it.key());
        if (!key.isEmpty())
            m_entries.insert(key, it.value().toString());
    }
    return true;
}

bool WuWaDialogueDb::lookup(const QString &englishText, QString &translated) const
{
    const QString key = TranslationCache::normalizeKey(englishText);
    if (key.isEmpty())
        return false;
    QMutexLocker locker(&m_mutex);
    const auto it = m_entries.constFind(key);
    if (it == m_entries.constEnd())
        return false;
    translated = it.value();
    return true;
}

int WuWaDialogueDb::size() const
{
    QMutexLocker locker(&m_mutex);
    return m_entries.size();
}
