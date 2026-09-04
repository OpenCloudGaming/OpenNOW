#include "ThumbnailGenerator.h"

#include <QDir>
#include <QFileInfo>
#include <QImage>
#include <QMediaPlayer>
#include <QSaveFile>
#include <QStandardPaths>
#include <QUrl>
#include <QVideoFrame>
#include <QVideoSink>

#include <utility>

using namespace Qt::StringLiterals;

ThumbnailGenerator::ThumbnailGenerator(QObject *parent)
    : QObject(parent)
{
    m_timeout.setSingleShot(true);
    m_timeout.setInterval(10'000);
    connect(&m_timeout, &QTimer::timeout, this, [this] {
        complete(false, u"Thumbnail decoding timed out"_s);
    });
}

ThumbnailGenerator::~ThumbnailGenerator() = default;

void ThumbnailGenerator::ensureInitialized()
{
    if (m_player) return;
    m_sink = std::make_unique<QVideoSink>();
    m_player = std::make_unique<QMediaPlayer>();
    m_player->setVideoSink(m_sink.get());
    connect(m_sink.get(), &QVideoSink::videoFrameChanged, this, [this](const QVideoFrame &frame) {
        if (!m_busy || !frame.isValid()) return;
        const auto image = frame.toImage();
        if (image.isNull()) return;
        complete(true, u"Thumbnail regenerated"_s, image);
    });
    connect(m_player.get(), &QMediaPlayer::errorOccurred, this,
            [this](QMediaPlayer::Error, const QString &) {
                if (m_busy) complete(false, u"The video could not be decoded"_s);
            });
    connect(m_player.get(), &QMediaPlayer::durationChanged, this, [this](qint64 duration) {
        if (m_busy && duration > 500) m_player->setPosition(qMin<qint64>(2'000, duration / 8));
    });
}

bool ThumbnailGenerator::busy() const
{
    return m_busy;
}

bool ThumbnailGenerator::regenerate(const QString &urlOrPath)
{
    if (m_busy) return false;
    const auto source = trustedSource(urlOrPath);
    if (source.isEmpty()) return false;
    ensureInitialized();
    const QFileInfo info(source);
    m_sourcePath = source;
    m_targetPath = info.absolutePath() + u'/' + info.completeBaseName() + u"-thumb.jpg"_s;
    m_busy = true;
    emit busyChanged();
    m_player->setSource(QUrl::fromLocalFile(source));
    m_timeout.start();
    m_player->play();
    return true;
}

QString ThumbnailGenerator::trustedSource(const QString &urlOrPath) const
{
    const QUrl url(urlOrPath);
    const QFileInfo source(url.isLocalFile() ? url.toLocalFile() : urlOrPath);
    if (!source.isAbsolute() || !source.isFile() || source.size() <= 0
            || source.size() > 16LL * 1024 * 1024 * 1024) {
        return {};
    }
    const auto extension = source.suffix().toLower();
    if (!QStringList{u"mp4"_s, u"webm"_s, u"mkv"_s, u"mov"_s, u"avi"_s}.contains(extension)) {
        return {};
    }
    const auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    if (pictures.isEmpty()) return {};
    QDir recordings(QDir(pictures).filePath(u"OpenNOW/Recordings"_s));
    if (!recordings.exists()
            || QDir::cleanPath(source.absolutePath()) != QDir::cleanPath(recordings.absolutePath())) {
        return {};
    }
    return source.canonicalFilePath();
}

void ThumbnailGenerator::complete(bool ok, const QString &message, QImage image)
{
    if (!m_busy) return;
    m_timeout.stop();
    if (m_player) {
        m_player->stop();
        m_player->setSource({});
    }
    auto saved = ok && !image.isNull();
    if (saved) {
        auto thumbnail = std::move(image);
        if (thumbnail.width() > 960 || thumbnail.height() > 540) {
            thumbnail = thumbnail.scaled(960, 540, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        }
        QSaveFile output(m_targetPath);
        saved = output.open(QIODevice::WriteOnly)
            && thumbnail.save(&output, "JPG", 88)
            && output.commit();
        if (!saved) output.cancelWriting();
    }
    const auto source = m_sourcePath;
    const auto thumbnailUrl = saved ? QUrl::fromLocalFile(m_targetPath).toString() : QString{};
    m_sourcePath.clear();
    m_targetPath.clear();
    m_busy = false;
    emit busyChanged();
    emit finished(source, thumbnailUrl, saved,
                  saved ? message : u"Could not regenerate the thumbnail"_s);
}
