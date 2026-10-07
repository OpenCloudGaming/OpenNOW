#pragma once

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QVariantMap>

#include <functional>
#include <optional>

class CoreClient;
class NativeStreamRuntime;

class SourceBridge final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QVariantMap activeProfile READ activeProfile NOTIFY activeProfileChanged)

public:
    static constexpr int NativeProtocolVersion = 8;

    SourceBridge(CoreClient &core, NativeStreamRuntime &runtime, QObject *parent = nullptr);

    Q_INVOKABLE QString create(const QString &sourceId, const QJsonObject &intent);
    Q_INVOKABLE bool cancel(const QString &requestId);
    Q_INVOKABLE QString start(const QString &sourceId, const QJsonObject &session,
                              const QString &sessionHandle);
    Q_INVOKABLE bool openAuthorization(const QString &sourceId, const QString &openHandle);
    [[nodiscard]] QVariantMap activeProfile() const { return m_activeProfile; }
    static QVariantMap acceptedVideoProfile(const QJsonObject &lease);

signals:
    void created(const QString &requestId, const QJsonObject &result);
    void failed(const QString &requestId, const QString &code, const QString &message);
    void authorizationOpenFailed(const QString &sourceId, const QString &message);
    void activeProfileChanged();

private:
    struct Offer {
        QJsonObject offer;
        QJsonObject runtimeCapabilities;
    };
    using OfferHandler = std::function<void(std::optional<Offer> offer, const QString &code,
                                            const QString &message)>;
    struct Playback {
        QString startId;
        QString sourceId;
        QJsonObject session;
        QString prepareId;
        QString leaseId;
        QVariantMap profile;
        bool accepted = false;
        bool releasing = false;
        bool retired = false;
        bool releaseInFlight = false;
        int releaseAttempts = 0;
    };

    void requestOffer(OfferHandler handler);
    void prepare(const QString &startId, const QString &sessionHandle, const Offer &offer, int attempt);
    void cancelOffer(const QJsonObject &offer);
    void failStart(const QString &startId, const QString &code, const QString &message);
    void onNativeResponse(const QJsonObject &response);
    void onNativeEvent(const QJsonObject &event);
    void confirmRetirement(int attempt);
    void release();
    void reconcileMedia();
    void scheduleReconcile();
    bool mediaTransitionPending() const;
    void setActiveProfile(const QVariantMap &profile);

    CoreClient &m_core;
    NativeStreamRuntime &m_runtime;
    QHash<QString, QString> m_creates;
    std::optional<Playback> m_playback;
    QVariantMap m_activeProfile;
    quint64 m_nextId = 1;
    bool m_reconciling = false;
    bool m_reconcileAgain = false;
    int m_reconcileRetries = 0;
};
