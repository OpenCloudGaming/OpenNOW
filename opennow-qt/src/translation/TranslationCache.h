// TranslationCache: normalized EN -> VI in-memory cache, persisted as JSON.
//
// Thread-safe (QMutex). Keys are normalized with normalizeKey() (trim,
// collapse internal whitespace, case-insensitive) so the same English line
// OCR'd with different casing/spacing still hits the cache.
#pragma once

#include <QHash>
#include <QList>
#include <QMutex>
#include <QString>

class TranslationCache
{
public:
    static constexpr int DefaultMaxEntries = 5000;

    explicit TranslationCache(int maxEntries = DefaultMaxEntries);
    explicit TranslationCache(const QString &filePath, int maxEntries = DefaultMaxEntries);

    // Normalizes a source string into a cache key: trimmed, internal
    // whitespace collapsed to single spaces, lower-cased.
    static QString normalizeKey(const QString &text);

    // Returns true on hit and fills `translated`. Records lookup timing in
    // lastLookupUs().
    bool lookup(const QString &englishText, QString &translated);
    void insert(const QString &englishText, const QString &translatedText);
    void clear();

    int size() const;
    int maxEntries() const { return m_maxEntries; }
    QString filePath() const { return m_filePath; }
    qint64 lastLookupUs() const;

    // Loads persisted entries from filePath() (missing file = empty cache).
    // Returns false only if the file exists but cannot be parsed.
    bool load();
    bool save() const;

private:
    QString m_filePath;
    int m_maxEntries;
    mutable QMutex m_mutex;
    QHash<QString, QString> m_entries;
    QList<QString> m_insertionOrder; // oldest first; drives max-size eviction
    qint64 m_lastLookupUs = 0;
};
