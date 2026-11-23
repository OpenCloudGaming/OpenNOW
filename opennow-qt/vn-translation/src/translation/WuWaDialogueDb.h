#pragma once

#include <QHash>
#include <QList>
#include <QMutex>
#include <QPair>
#include <QRegularExpression>
#include <QString>
#include <QStringList>

// Pre-translated Wuthering Waves dialogue & textmap database (EN -> VI).
//
// Features:
//   - Automatic RichText tag stripping (<color=...>, <size=...>, etc.).
//   - Automatic 2-state Viterbi shift repair for off-by-one chunk slices.
//   - Dynamic placeholder template matching ({q_count}, {0}, etc.).
//   - Exact, squashed-alphanumeric, first-two-words, and word-trigram fuzzy lookup.
//   - HUD / combat numeric noise filter (FPS, ping, UID, LV, single hotkeys).
//   - Fast flat binary cache (.bin) for low-RAM machines (HP 840 G1 8GB RAM).
//
// Thread-safe (internal QMutex).
class WuWaDialogueDb
{
public:
    WuWaDialogueDb() = default;
    explicit WuWaDialogueDb(const QString &filePath);

    bool loadDialogueDb(const QString &path);
    bool loadDialogueDb();

    // Merges one chunk file into the DB without clearing existing entries.
    bool loadChunkFile(const QString &path, bool rebuildIndexNow = true);
    void rebuildIndex();

    // Fast flat binary cache persistence (<15ms load for 220k entries).
    bool saveBinaryCache(const QString &binPath) const;
    bool loadBinaryCache(const QString &binPath);

    void clear();

    bool lookup(const QString &englishText, QString &translated) const;
    bool lookupFuzzy(const QString &englishText, QString &translated) const;
    int size() const;

    // Strips Unity/Unreal rich-text tags (<color=...>, </color>, <size=...>, </size>, etc.).
    static QString stripRichTextTags(const QString &text);

    // Returns true if the OCR'd string is combat/HUD noise (FPS, ms, UID, LV, pure numbers, 1-char hotkeys).
    static bool isHudNoise(const QString &text);

    // Repairs off-by-one shifted slices in an ordered list of (en, vi) pairs.
    static QList<QPair<QString, QString>> repairShiftedEntries(
        const QList<QPair<QString, QString>> &orderedEntries,
        int *repairedCountOut = nullptr);

    // Pure helpers exposed for unit testing.
    static double similarityRatio(const QString &a, const QString &b);
    static QString firstTwoWords(const QString &normalizedKey);

private:
    struct TemplateRule
    {
        QRegularExpression pattern;
        QStringList placeholderNames;
        QString viTemplate;
    };

    void insertCleanedEntryLocked(const QString &rawEn, const QString &rawVi);
    void rebuildIndexLocked();
    bool matchTemplateLocked(const QString &normalizedKey, QString &translated) const;

    QString m_filePath;
    mutable QMutex m_mutex;
    QHash<QString, QString> m_entries; // normalized key -> clean Vietnamese
    QHash<QString, QStringList> m_firstTwoWordsIndex; // firstTwoWords(key) -> keys
    QHash<QString, QStringList> m_lastWordIndex; // lastWord(key) -> keys (fallback when first word has OCR typo)
    QHash<QString, QString> m_squashedIndex; // alnum-only key -> normalized key
    QHash<QString, QList<TemplateRule>> m_templateIndex; // firstStaticWord -> template rules
};
