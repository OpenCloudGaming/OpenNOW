#pragma once

#include <QtCore/QList>
#include <QtCore/QHash>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QSet>
#include <QtGui/QColor>
#include <QtGui/QImage>
#include <QtGui/QPixmap>
#include <QtWidgets/QWidget>

#include <functional>

class QNetworkAccessManager;
class QNetworkReply;

namespace OpnQt {

struct CatalogGame {
    QString title;
    QString store;
    QString subtitle;
    QStringList tags;
    QColor accent;
    QString imageUrl;
    QString heroImageUrl;
    QString description;
    QString launchAppId;
    QString internalTitle;
    QString selectedStore;
};

struct CatalogFolder {
    QString title;
    QString subtitle;
    QList<CatalogGame> games;
};

class CatalogView final : public QWidget {
public:
    explicit CatalogView(QWidget *parent = nullptr);

    void setAccountName(const QString &name);
    void setGames(const QList<CatalogGame> &games);
    void setLoading(const QString &message);
    void setError(const QString &message);
    void setError(const QString &title, const QString &message);
    void setStreamFrame(const QImage &frame);
    void setPreviewData();
    void setLaunchRequestedCallback(std::function<void(const CatalogGame &)> callback);
    void setStreamInputCallbacks(std::function<void(quint16, quint16, quint16, bool)> keyCallback,
                                 std::function<void(qint16, qint16)> mouseMoveCallback,
                                 std::function<void(quint8, bool)> mouseButtonCallback,
                                 std::function<void(qint16)> mouseWheelCallback);

protected:
    void showEvent(QShowEvent *event) override;
    void hideEvent(QHideEvent *event) override;
    void paintEvent(QPaintEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;

private:
    void moveFolder(int delta);
    void moveGame(int delta);
    void populatePreviewData();
    void drawBackground(QPainter &painter);
    void drawHeader(QPainter &painter);
    void drawFolderRail(QPainter &painter);
    void drawFolder(QPainter &painter, const CatalogFolder &folder, const QRectF &rect, bool selected, int folderIndex);
    void drawGameColumn(QPainter &painter);
    void drawGameCard(QPainter &painter, const CatalogGame &game, const QRectF &rect, bool selected, int gameIndex);
    void drawDetailPanel(QPainter &painter);
    void drawStreamFrame(QPainter &painter);
    void drawFooter(QPainter &painter);
    void requestImage(const QString &url);
    const QPixmap *cachedImage(const QString &url) const;

    QList<CatalogFolder> m_folders;
    QString m_accountName = QStringLiteral("Player");
    QString m_overlayMessage;
    QString m_overlayTitle;
    bool m_loading = false;
    bool m_error = false;
    int m_selectedFolder = 0;
    int m_selectedGame = 0;
    QString m_statusText = QStringLiteral("Controller navigation only");
    QNetworkAccessManager *m_network = nullptr;
    QHash<QString, QPixmap> m_imageCache;
    QSet<QString> m_pendingImages;
    QImage m_streamFrame;
    QPointF m_lastMousePosition;
    qreal m_backgroundPhase = 0.0;
    std::function<void(const CatalogGame &)> m_onLaunchRequested;
    std::function<void(quint16 keycode, quint16 scancode, quint16 modifiers, bool down)> m_onStreamKey;
    std::function<void(qint16 dx, qint16 dy)> m_onStreamMouseMove;
    std::function<void(quint8 button, bool down)> m_onStreamMouseButton;
    std::function<void(qint16 delta)> m_onStreamMouseWheel;
};

} // namespace OpnQt
