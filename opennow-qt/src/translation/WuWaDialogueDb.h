// WuWaDialogueDb: pre-translated Wuthering Waves dialogue lines.
//
// Same JSON format as TranslationCache ({"version":1,"entries":{en: vi}})
// with keys normalized the same way, so a DB hit and a cache hit are
// interchangeable. The DB is consulted BEFORE the memory cache: a curated
// line always wins over a machine-translated one.
//
// The shipped seed file (data/wuwa-dialogue-db.json) intentionally contains
// ZERO datamined lines: no game dialogue translations are invented here.
// It starts empty and fills only from lines actually seen (and verified)
// during live play. Do not hand-write dialogue into the seed file.
#pragma once

#include <QHash>
#include <QMutex>
#include <QString>

class WuWaDialogueDb
{
public:
    WuWaDialogueDb() = default;
    explicit WuWaDialogueDb(const QString &filePath);

    // Loads entries from `path` (JSON, same shape as the translation cache).
    // Returns false if the file exists but cannot be parsed; a missing file
    // leaves the DB empty and returns true.
    bool loadDialogueDb(const QString &path);
    bool loadDialogueDb(); // reloads from the path given at construction

    // Normalized lookup. Returns true on hit and fills `translated`.
    bool lookup(const QString &englishText, QString &translated) const;

    int size() const;
    QString filePath() const { return m_filePath; }

private:
    QString m_filePath;
    mutable QMutex m_mutex;
    QHash<QString, QString> m_entries;
};
