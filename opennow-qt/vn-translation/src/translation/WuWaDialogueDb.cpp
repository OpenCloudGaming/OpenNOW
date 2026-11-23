#include "translation/WuWaDialogueDb.h"
#include "translation/TranslationCache.h"

#include <QDataStream>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMutexLocker>
#include <QRegularExpression>
#include <QSet>
#include <QtGlobal>
#include <algorithm>

namespace {
constexpr double MinFuzzyRatio = 0.86;
constexpr quint32 BinaryCacheMagic = 0x57555741; // "WUWA"
constexpr quint32 BinaryCacheVersion = 2;

QString squashKey(const QString &normalizedKey)
{
    QString out;
    out.reserve(normalizedKey.size());
    for (const QChar c : normalizedKey) {
        if (c.isLetterOrNumber())
            out.append(c);
    }
    return out;
}

QString lastWordOf(const QString &normalizedKey)
{
    const int lastSpace = normalizedKey.lastIndexOf(QChar(u' '));
    if (lastSpace < 0)
        return normalizedKey;
    return normalizedKey.mid(lastSpace + 1);
}

QString firstWordOf(const QString &normalizedKey)
{
    const int firstSpace = normalizedKey.indexOf(QChar(u' '));
    if (firstSpace < 0)
        return normalizedKey;
    return normalizedKey.left(firstSpace);
}

// Extracts ordered ("key", "value") pairs from the "entries": { ... } object
// in raw JSON text so we preserve the file's key order (QJsonObject sorts keys
// alphabetically, which would hide off-by-one shift boundaries).
bool extractOrderedEntriesFromJson(const QByteArray &rawBytes,
                                   QList<QPair<QString, QString>> &orderedOut)
{
    const QJsonDocument doc = QJsonDocument::fromJson(rawBytes);
    if (!doc.isObject())
        return false;
    const QJsonObject rootObj = doc.object();
    if (!rootObj.value(QStringLiteral("entries")).isObject())
        return false;

    const QString text = QString::fromUtf8(rawBytes);
    const int entriesPos = text.indexOf(QStringLiteral("\"entries\""));
    if (entriesPos < 0)
        return false;
    int bracePos = text.indexOf(QChar(u'{'), entriesPos + 9);
    if (bracePos < 0)
        return false;

    auto parseJsonString = [&](int &pos, QString &out) -> bool {
        while (pos < text.size() && text.at(pos).isSpace())
            ++pos;
        if (pos >= text.size() || text.at(pos) != QChar(u'"'))
            return false;
        ++pos; // skip opening quote
        out.clear();
        while (pos < text.size()) {
            const QChar c = text.at(pos++);
            if (c == QChar(u'"'))
                return true;
            if (c == QChar(u'\\')) {
                if (pos >= text.size())
                    return false;
                const QChar esc = text.at(pos++);
                switch (esc.unicode()) {
                case u'"': out.append(QChar(u'"')); break;
                case u'\\': out.append(QChar(u'\\')); break;
                case u'/': out.append(QChar(u'/')); break;
                case u'b': out.append(QChar(u'\b')); break;
                case u'f': out.append(QChar(u'\f')); break;
                case u'n': out.append(QChar(u'\n')); break;
                case u'r': out.append(QChar(u'\r')); break;
                case u't': out.append(QChar(u'\t')); break;
                case u'u':
                    if (pos + 4 <= text.size()) {
                        bool ok = false;
                        const ushort code = text.mid(pos, 4).toUShort(&ok, 16);
                        if (ok)
                            out.append(QChar(code));
                        pos += 4;
                    }
                    break;
                default:
                    out.append(esc);
                    break;
                }
            } else {
                out.append(c);
            }
        }
        return false;
    };

    int pos = bracePos + 1;
    orderedOut.clear();
    while (pos < text.size()) {
        while (pos < text.size() && (text.at(pos).isSpace() || text.at(pos) == QChar(u',')))
            ++pos;
        if (pos >= text.size() || text.at(pos) == QChar(u'}'))
            break;
        QString key;
        if (!parseJsonString(pos, key))
            break;
        while (pos < text.size() && text.at(pos).isSpace())
            ++pos;
        if (pos >= text.size() || text.at(pos) != QChar(u':'))
            break;
        ++pos;
        QString val;
        if (!parseJsonString(pos, val))
            break;
        orderedOut.append(qMakePair(key, val));
    }

    if (orderedOut.isEmpty()) {
        const QJsonObject entries = rootObj.value(QStringLiteral("entries")).toObject();
        for (auto it = entries.constBegin(); it != entries.constEnd(); ++it)
            orderedOut.append(qMakePair(it.key(), it.value().toString()));
    }
    return true;
}

QSet<QString> extractTokens(const QString &s, const QRegularExpression &re)
{
    QSet<QString> set;
    auto it = re.globalMatch(s);
    while (it.hasNext())
        set.insert(it.next().captured(0));
    return set;
}

double scoreAlignmentPair(const QString &en, const QString &vi)
{
    if (en.isEmpty() || vi.isEmpty())
        return -5.0;
    double s = 0.0;
    const QString enL = en.trimmed().toLower();
    const QString viL = vi.trimmed().toLower();

    static const QList<QPair<QString, QString>> glossary = {
        {QStringLiteral("talk"), QStringLiteral("nói chuyện")},
        {QStringLiteral("read"), QStringLiteral("đọc")},
        {QStringLiteral("take photos"), QStringLiteral("chụp ảnh")},
        {QStringLiteral("take a photo"), QStringLiteral("chụp ảnh")},
        {QStringLiteral("investigate"), QStringLiteral("điều tra")},
        {QStringLiteral("inspect"), QStringLiteral("kiểm tra")},
        {QStringLiteral("activate"), QStringLiteral("kích hoạt")},
        {QStringLiteral("enter"), QStringLiteral("vào")},
        {QStringLiteral("open"), QStringLiteral("mở")},
        {QStringLiteral("interact"), QStringLiteral("tương tác")},
        {QStringLiteral("leave"), QStringLiteral("rời")},
        {QStringLiteral("gold"), QStringLiteral("vàng")},
        {QStringLiteral("silver"), QStringLiteral("bạc")},
        {QStringLiteral("bronze"), QStringLiteral("đồng")},
        {QStringLiteral("challenge failed"), QStringLiteral("thử thách thất bại")},
        {QStringLiteral("challenge complete"), QStringLiteral("hoàn thành thử thách")},
    };
    for (const auto &g : glossary) {
        if (enL == g.first) {
            s += (viL == g.second || viL.startsWith(g.second)) ? 8.0 : -8.0;
        } else if (enL.startsWith(g.first + QChar(u' ')) || enL.startsWith(g.first + QChar(u':'))) {
            s += viL.contains(g.second) ? 3.5 : -2.5;
        }
    }

    static const QRegularExpression phRe(QStringLiteral("\\{[^}]+\\}"));
    const QSet<QString> phEn = extractTokens(en, phRe);
    const QSet<QString> phVi = extractTokens(vi, phRe);
    if (!phEn.isEmpty() || !phVi.isEmpty())
        s += (phEn == phVi) ? 6.0 : -7.0;

    static const QRegularExpression numRe(QStringLiteral("\\b\\d+\\b"));
    const QString cleanEn = WuWaDialogueDb::stripRichTextTags(en);
    const QString cleanVi = WuWaDialogueDb::stripRichTextTags(vi);
    const QSet<QString> numEn = extractTokens(cleanEn, numRe);
    const QSet<QString> numVi = extractTokens(cleanVi, numRe);
    if (!numEn.isEmpty() || !numVi.isEmpty())
        s += (numEn == numVi) ? 4.0 : -4.0;

    const double ratio = static_cast<double>(vi.size()) / std::max(1, static_cast<int>(en.size()));
    if (ratio >= 0.6 && ratio <= 2.2)
        s += 1.5;
    else if (ratio > 3.0 || ratio < 0.33)
        s -= 5.0;

    return s;
}
} // namespace

WuWaDialogueDb::WuWaDialogueDb(const QString &filePath)
    : m_filePath(filePath)
{
}

QString WuWaDialogueDb::stripRichTextTags(const QString &text)
{
    if (!text.contains(QChar(u'<')))
        return text;
    static const QRegularExpression tagRe(
        QStringLiteral("</?(?:color|size|b|i|u|material|quad)(?:=[^>]*)?>"),
        QRegularExpression::CaseInsensitiveOption);
    QString out = text;
    out.remove(tagRe);
    return out;
}

bool WuWaDialogueDb::isHudNoise(const QString &text)
{
    const QString clean = stripRichTextTags(text).trimmed();
    if (clean.isEmpty())
        return true;
    // Single-character hotkey badges (E, F, Q, R, T, Z, V, C, 1..9)
    if (clean.size() == 1)
        return true;
    // Pure numbers, fractions, percentages, timers, coordinates (e.g. "120/120", "45%", "02:15", "+1,250")
    static const QRegularExpression pureNumericRe(
        QStringLiteral("^[\\d\\s.,:%+\\-/()x×*]+$"));
    if (pureNumericRe.match(clean).hasMatch())
        return true;
    // FPS, Ping/ms, UID, LV/Lv., resonant frequency numbers
    static const QRegularExpression hudMetricRe(
        QStringLiteral("^(?:uid\\s*:?\\s*\\d+|lv\\.?\\s*\\d+|\\d+\\s*(?:fps|ms|ping|mb/s|kb/s)|esc|tab|shift|ctrl|alt|space)$"),
        QRegularExpression::CaseInsensitiveOption);
    if (hudMetricRe.match(clean).hasMatch())
        return true;
    return false;
}

QList<QPair<QString, QString>> WuWaDialogueDb::repairShiftedEntries(
    const QList<QPair<QString, QString>> &orderedEntries,
    int *repairedCountOut)
{
    if (repairedCountOut)
        *repairedCountOut = 0;
    const int total = orderedEntries.size();
    if (total <= 2)
        return orderedEntries;

    const int n = total - 1;
    constexpr double Penalty = 3.5;
    QList<double> dp0(n, 0.0);
    QList<double> dp1(n, 0.0);
    QList<int> back0(n, 0);
    QList<int> back1(n, 0);

    dp0[0] = scoreAlignmentPair(orderedEntries.at(0).first, orderedEntries.at(0).second);
    dp1[0] = scoreAlignmentPair(orderedEntries.at(1).first, orderedEntries.at(0).second) - Penalty;

    for (int i = 1; i < n; ++i) {
        const double s0 = scoreAlignmentPair(orderedEntries.at(i).first, orderedEntries.at(i).second);
        const double s1 = scoreAlignmentPair(orderedEntries.at(i + 1).first, orderedEntries.at(i).second);
        if (dp0[i - 1] >= dp1[i - 1] - Penalty) {
            dp0[i] = dp0[i - 1] + s0;
            back0[i] = 0;
        } else {
            dp0[i] = dp1[i - 1] - Penalty + s0;
            back0[i] = 1;
        }
        if (dp1[i - 1] >= dp0[i - 1] - Penalty) {
            dp1[i] = dp1[i - 1] + s1;
            back1[i] = 1;
        } else {
            dp1[i] = dp0[i - 1] - Penalty + s1;
            back1[i] = 0;
        }
    }

    QList<int> state(n, 0);
    state[n - 1] = dp1[n - 1] > dp0[n - 1] ? 1 : 0;
    for (int i = n - 2; i >= 0; --i)
        state[i] = (state[i + 1] == 1) ? back1[i + 1] : back0[i + 1];

    int shiftedCount = 0;
    QList<QPair<QString, QString>> result;
    result.reserve(total + 8);
    QSet<QString> seenKeys;
    for (int i = 0; i < n; ++i) {
        if (state[i] == 0) {
            result.append(orderedEntries.at(i));
            seenKeys.insert(orderedEntries.at(i).first);
        } else {
            ++shiftedCount;
            result.append(qMakePair(orderedEntries.at(i + 1).first, orderedEntries.at(i).second));
            seenKeys.insert(orderedEntries.at(i + 1).first);
        }
    }
    if (state[n - 1] == 0) {
        result.append(orderedEntries.at(n));
        seenKeys.insert(orderedEntries.at(n).first);
    }

    // Ensure common short action labels displaced by shift boundaries always remain valid
    static const QList<QPair<QString, QString>> coreVerbs = {
        {QStringLiteral("Talk"), QStringLiteral("Nói chuyện")},
        {QStringLiteral("Read"), QStringLiteral("Đọc")},
        {QStringLiteral("Take photos"), QStringLiteral("Chụp ảnh")},
        {QStringLiteral("Take a photo"), QStringLiteral("Chụp ảnh")},
        {QStringLiteral("Investigate"), QStringLiteral("Điều tra")},
        {QStringLiteral("Inspect"), QStringLiteral("Kiểm tra")},
        {QStringLiteral("Activate"), QStringLiteral("Kích hoạt")},
        {QStringLiteral("Enter"), QStringLiteral("Vào")},
        {QStringLiteral("Open"), QStringLiteral("Mở")},
        {QStringLiteral("Interact"), QStringLiteral("Tương tác")},
        {QStringLiteral("Leave"), QStringLiteral("Rời đi")},
    };
    for (const auto &verb : coreVerbs) {
        for (const auto &orig : orderedEntries) {
            if (orig.first.compare(verb.first, Qt::CaseInsensitive) == 0) {
                result.append(verb);
                break;
            }
        }
    }

    if (repairedCountOut)
        *repairedCountOut = shiftedCount;
    return result;
}

void WuWaDialogueDb::insertCleanedEntryLocked(const QString &rawEn, const QString &rawVi)
{
    const QString cleanEn = stripRichTextTags(rawEn);
    const QString cleanVi = stripRichTextTags(rawVi).trimmed();
    const QString key = TranslationCache::normalizeKey(cleanEn);
    if (key.isEmpty() || cleanVi.isEmpty())
        return;
    m_entries.insert(key, cleanVi);
}

bool WuWaDialogueDb::loadDialogueDb(const QString &path)
{
    m_filePath = path;
    return loadDialogueDb();
}

bool WuWaDialogueDb::loadDialogueDb()
{
    if (m_filePath.isEmpty())
        return true;
    QFile file(m_filePath);
    if (!file.exists())
        return true;
    if (!file.open(QIODevice::ReadOnly))
        return false;
    const QByteArray rawBytes = file.readAll();
    file.close();

    QList<QPair<QString, QString>> ordered;
    if (!extractOrderedEntriesFromJson(rawBytes, ordered))
        return false;
    const auto repaired = repairShiftedEntries(ordered);

    QMutexLocker locker(&m_mutex);
    m_entries.clear();
    for (const auto &pair : repaired)
        insertCleanedEntryLocked(pair.first, pair.second);
    rebuildIndexLocked();
    return true;
}

bool WuWaDialogueDb::loadChunkFile(const QString &path, bool rebuildIndexNow)
{
    QFile file(path);
    if (!file.exists())
        return true;
    if (!file.open(QIODevice::ReadOnly))
        return false;
    const QByteArray rawBytes = file.readAll();
    file.close();

    QList<QPair<QString, QString>> ordered;
    if (!extractOrderedEntriesFromJson(rawBytes, ordered))
        return false;
    const auto repaired = repairShiftedEntries(ordered);

    QMutexLocker locker(&m_mutex);
    for (const auto &pair : repaired)
        insertCleanedEntryLocked(pair.first, pair.second);
    if (rebuildIndexNow)
        rebuildIndexLocked();
    return true;
}

void WuWaDialogueDb::rebuildIndex()
{
    QMutexLocker locker(&m_mutex);
    rebuildIndexLocked();
}

bool WuWaDialogueDb::saveBinaryCache(const QString &binPath) const
{
    QFile file(binPath);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    QDataStream out(&file);
    out.setVersion(QDataStream::Qt_6_5);
    QMutexLocker locker(&m_mutex);
    out << BinaryCacheMagic << BinaryCacheVersion << static_cast<quint32>(m_entries.size());
    for (auto it = m_entries.constBegin(); it != m_entries.constEnd(); ++it)
        out << it.key() << it.value();
    return out.status() == QDataStream::Ok && file.flush();
}

bool WuWaDialogueDb::loadBinaryCache(const QString &binPath)
{
    QFile file(binPath);
    if (!file.exists() || !file.open(QIODevice::ReadOnly))
        return false;
    QDataStream in(&file);
    in.setVersion(QDataStream::Qt_6_5);
    quint32 magic = 0;
    quint32 version = 0;
    quint32 count = 0;
    in >> magic >> version >> count;
    if (in.status() != QDataStream::Ok || magic != BinaryCacheMagic || version != BinaryCacheVersion || count == 0)
        return false;

    QHash<QString, QString> loaded;
    loaded.reserve(static_cast<qsizetype>(count));
    for (quint32 i = 0; i < count; ++i) {
        QString k;
        QString v;
        in >> k >> v;
        if (in.status() != QDataStream::Ok)
            return false;
        if (!k.isEmpty() && !v.isEmpty())
            loaded.insert(k, v);
    }

    QMutexLocker locker(&m_mutex);
    m_entries = std::move(loaded);
    rebuildIndexLocked();
    return true;
}

void WuWaDialogueDb::clear()
{
    QMutexLocker locker(&m_mutex);
    m_entries.clear();
    m_firstTwoWordsIndex.clear();
    m_lastWordIndex.clear();
    m_squashedIndex.clear();
    m_templateIndex.clear();
}

bool WuWaDialogueDb::matchTemplateLocked(const QString &normalizedKey, QString &translated) const
{
    const QString firstWord = firstWordOf(normalizedKey);
    const auto rules = m_templateIndex.value(firstWord);
    for (const TemplateRule &rule : rules) {
        const QRegularExpressionMatch match = rule.pattern.match(normalizedKey);
        if (!match.hasMatch())
            continue;
        QString out = rule.viTemplate;
        for (int i = 0; i < rule.placeholderNames.size(); ++i) {
            const QString captured = match.captured(i + 1);
            out.replace(rule.placeholderNames.at(i), captured, Qt::CaseInsensitive);
        }
        translated = out;
        return true;
    }
    return false;
}

bool WuWaDialogueDb::lookup(const QString &englishText, QString &translated) const
{
    const QString clean = stripRichTextTags(englishText);
    const QString key = TranslationCache::normalizeKey(clean);
    if (key.isEmpty())
        return false;
    QMutexLocker locker(&m_mutex);
    const auto it = m_entries.constFind(key);
    if (it != m_entries.constEnd()) {
        translated = it.value();
        return true;
    }
    return matchTemplateLocked(key, translated);
}

bool WuWaDialogueDb::lookupFuzzy(const QString &englishText, QString &translated) const
{
    const QString clean = stripRichTextTags(englishText);
    const QString key = TranslationCache::normalizeKey(clean);
    if (key.isEmpty())
        return false;
    QMutexLocker locker(&m_mutex);
    // 1) Exact on the normalized key.
    const auto exact = m_entries.constFind(key);
    if (exact != m_entries.constEnd()) {
        translated = exact.value();
        return true;
    }
    // 2) Dynamic placeholder template match ({q_count}, {0}, etc.).
    if (matchTemplateLocked(key, translated))
        return true;
    // 3) Exact on the squashed form (OCR punctuation/space noise).
    const QString squashed = squashKey(key);
    if (!squashed.isEmpty()) {
        const auto squashedHit = m_squashedIndex.constFind(squashed);
        if (squashedHit != m_squashedIndex.constEnd()) {
            translated = m_entries.value(squashedHit.value());
            if (!translated.isEmpty())
                return true;
        }
    }
    // 4) Levenshtein ratio among entries sharing the first two words OR last word
    // (protects against an OCR typo in the first two words, e.g. "Ta1k to Shilang").
    QStringList candidates = m_firstTwoWordsIndex.value(firstTwoWords(key));
    const QString lastW = lastWordOf(key);
    if (lastW.size() >= 4) {
        const QStringList tailCandidates = m_lastWordIndex.value(lastW);
        if (tailCandidates.size() <= 64)
            candidates.append(tailCandidates);
    }

    double bestRatio = 0.0;
    QString bestTranslation;
    for (const QString &candidate : candidates) {
        if (qAbs(candidate.size() - key.size()) > std::max<qsizetype>(3, key.size() / 4))
            continue;
        const double ratio = similarityRatio(key, candidate);
        if (ratio > bestRatio) {
            bestRatio = ratio;
            bestTranslation = m_entries.value(candidate);
        }
    }
    if (bestRatio >= MinFuzzyRatio && !bestTranslation.isEmpty()) {
        translated = bestTranslation;
        return true;
    }
    return false;
}

double WuWaDialogueDb::similarityRatio(const QString &a, const QString &b)
{
    const int n = a.size();
    const int m = b.size();
    if (n == 0 && m == 0)
        return 1.0;
    if (n == 0 || m == 0)
        return 0.0;
    QList<int> previous(m + 1);
    QList<int> current(m + 1);
    for (int j = 0; j <= m; ++j)
        previous[j] = j;
    for (int i = 1; i <= n; ++i) {
        current[0] = i;
        for (int j = 1; j <= m; ++j) {
            const int cost = a.at(i - 1) == b.at(j - 1) ? 0 : 1;
            current[j] = std::min({previous[j] + 1, current[j - 1] + 1,
                                   previous[j - 1] + cost});
        }
        previous.swap(current);
    }
    const int distance = previous[m];
    return 1.0 - static_cast<double>(distance) / std::max(n, m);
}

QString WuWaDialogueDb::firstTwoWords(const QString &normalizedKey)
{
    const int firstSpace = normalizedKey.indexOf(QChar(u' '));
    if (firstSpace < 0)
        return normalizedKey;
    const int secondSpace = normalizedKey.indexOf(QChar(u' '), firstSpace + 1);
    if (secondSpace < 0)
        return normalizedKey;
    return normalizedKey.left(secondSpace);
}

void WuWaDialogueDb::rebuildIndexLocked()
{
    m_firstTwoWordsIndex.clear();
    m_lastWordIndex.clear();
    m_squashedIndex.clear();
    m_templateIndex.clear();

    static const QRegularExpression phTokenRe(QStringLiteral("\\{[^}]+\\}"));

    for (auto it = m_entries.constBegin(); it != m_entries.constEnd(); ++it) {
        const QString &key = it.key();
        m_firstTwoWordsIndex[firstTwoWords(key)].append(key);
        const QString lastW = lastWordOf(key);
        if (lastW.size() >= 4)
            m_lastWordIndex[lastW].append(key);
        m_squashedIndex.insert(squashKey(key), key);

        // Compile template pattern if the key contains placeholders like {q_count}
        if (key.contains(QChar(u'{')) && key.contains(QChar(u'}'))) {
            const QString firstW = firstWordOf(key);
            if (firstW.isEmpty() || firstW.startsWith(QChar(u'{')))
                continue;
            QString regexStr = QStringLiteral("^");
            QStringList phNames;
            int lastPos = 0;
            auto matchIt = phTokenRe.globalMatch(key);
            while (matchIt.hasNext()) {
                const auto m = matchIt.next();
                regexStr += QRegularExpression::escape(key.mid(lastPos, m.capturedStart() - lastPos));
                regexStr += QStringLiteral("([\\w.,+\\-]+)");
                phNames.append(m.captured(0));
                lastPos = m.capturedEnd();
            }
            regexStr += QRegularExpression::escape(key.mid(lastPos)) + QStringLiteral("$");
            if (!phNames.isEmpty()) {
                m_templateIndex[firstW].append(TemplateRule{
                    QRegularExpression(regexStr), phNames, it.value()});
            }
        }
    }
}

int WuWaDialogueDb::size() const
{
    QMutexLocker locker(&m_mutex);
    return m_entries.size();
}
