#pragma once

#include <QObject>
#include <QImage>
#include <QTimer>

#include <memory>

class QMediaPlayer;
class QVideoSink;

class ThumbnailGenerator final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    explicit ThumbnailGenerator(QObject *parent = nullptr);
    ~ThumbnailGenerator() override;

    [[nodiscard]] bool busy() const;
    Q_INVOKABLE bool regenerate(const QString &urlOrPath);

signals:
    void busyChanged();
    void finished(const QString &sourcePath, const QString &thumbnailUrl, bool ok,
                  const QString &message);

private:
    void ensureInitialized();
    [[nodiscard]] QString trustedSource(const QString &urlOrPath) const;
    void complete(bool ok, const QString &message, QImage image = QImage());

    std::unique_ptr<QMediaPlayer> m_player;
    std::unique_ptr<QVideoSink> m_sink;
    QTimer m_timeout;
    QString m_sourcePath;
    QString m_targetPath;
    bool m_busy = false;
};
