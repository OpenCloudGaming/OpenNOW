#include "app/SourceBridge.h"

#include "core/CoreClient.h"
#include "streaming/NativeStreamRuntime.h"

#include <QDesktopServices>
#include <QRegularExpression>
#include <QMetaObject>
#include <QPointer>
#include <QTimer>
#include <QUrl>

#include <cmath>

using namespace Qt::StringLiterals;

namespace {
constexpr int SourceRequestTimeoutMs = 60'000;
constexpr int RetirementCheckIntervalMs = 500;
constexpr int MaximumRetirementChecks = 20;
constexpr int ReconcileRetryMs = 250;
constexpr int ReleaseRetryMs = 1'000;
constexpr int PrepareRetryMs = 200;
constexpr int MaximumPrepareAttempts = 25;
constexpr int MaximumReleaseAttempts = 30;
constexpr int MaximumReconcileRetries = 40;
}

SourceBridge::SourceBridge(CoreClient &core, NativeStreamRuntime &runtime, QObject *parent)
    : QObject(parent)
    , m_core(core)
    , m_runtime(runtime)
{
    connect(&m_runtime, &NativeStreamRuntime::responseReceived, this, &SourceBridge::onNativeResponse);
    connect(&m_runtime, &NativeStreamRuntime::eventReceived, this, &SourceBridge::onNativeEvent);
    connect(&m_core, &CoreClient::stateChanged, this, [this] {
        if (m_core.state() != u"ready"_s) return;
        m_reconcileRetries = 0;
        if (m_playback && m_playback->retired) {
            m_playback->releaseAttempts = 0;
            release();
        }
        reconcileMedia();
    });
    connect(&m_runtime, &NativeStreamRuntime::runningChanged, this, [this] {
        if (!m_playback) return;
        if (m_runtime.running()) {
            if (m_playback->releasing && !m_playback->retired) confirmRetirement(0);
            return;
        }
        setActiveProfile({});
        if (m_playback->accepted) {
            m_playback->releasing = true;
            return;
        }
        if (!m_playback->prepareId.isEmpty()) m_core.settleReceipt(m_playback->prepareId, false);
        failStart(m_playback->startId, u"runtime-stopped"_s, u"The embedded media runtime stopped"_s);
    });
}

QString SourceBridge::create(const QString &sourceId, const QJsonObject &intent)
{
    if (sourceId.isEmpty() || intent.isEmpty()) return {};
    const auto requestId = u"source-create-%1"_s.arg(m_nextId++);
    m_creates.insert(requestId, {});
    const QPointer guard(this);
    requestOffer([guard, requestId, sourceId, intent](std::optional<Offer> offer, const QString &code,
                                                      const QString &message) {
        if (!guard) return;
        if (!guard->m_creates.contains(requestId)) {
            if (offer) guard->cancelOffer(offer->offer);
            return;
        }
        if (!offer) {
            guard->m_creates.remove(requestId);
            emit guard->failed(requestId, code, message);
            return;
        }
        const QJsonObject params{{u"sourceId"_s, sourceId}, {u"request"_s, intent},
                                 {u"offer"_s, offer->offer},
                                 {u"runtimeCapabilities"_s, offer->runtimeCapabilities}};
        guard->m_creates[requestId].offer = offer->offer;
        const auto coreId = guard->m_core.requestPrivate(
            u"sources.session.create"_s, params,
            [guard, requestId](bool ok, const QJsonObject &result, const QString &code, const QString &message) {
                if (!guard || !guard->m_creates.contains(requestId)) return;
                guard->cancelOffer(guard->m_creates.take(requestId).offer);
                if (ok)
                    emit guard->created(requestId, result);
                else
                    emit guard->failed(requestId, code, message);
            },
            SourceRequestTimeoutMs);
        if (coreId.isEmpty()) {
            guard->m_creates.remove(requestId);
            guard->cancelOffer(offer->offer);
            emit guard->failed(requestId, u"core_not_ready"_s, u"The core is not ready"_s);
            return;
        }
        guard->m_creates[requestId].coreId = coreId;
    });
    return requestId;
}

bool SourceBridge::cancel(const QString &requestId)
{
    if (const auto create = m_creates.find(requestId); create != m_creates.end()) {
        const auto pending = *create;
        m_creates.erase(create);
        cancelOffer(pending.offer);
        if (!pending.coreId.isEmpty()) m_core.cancel(pending.coreId);
        emit failed(requestId, u"cancelled"_s, u"Request cancelled"_s);
        return true;
    }
    if (!m_playback || m_playback->startId != requestId || !m_playback->leaseId.isEmpty())
        return false;
    const auto playback = std::exchange(m_playback, std::nullopt);
    cancelOffer(playback->offer);
    if (!playback->prepareId.isEmpty()) m_core.cancel(playback->prepareId);
    emit failed(requestId, u"cancelled"_s, u"Request cancelled"_s);
    return true;
}

QString SourceBridge::start(const QString &sourceId, const QJsonObject &session,
                              const QString &sessionHandle)
{
    if (m_playback || sourceId.isEmpty() || session.isEmpty() || sessionHandle.isEmpty()) return {};
    const auto startId = u"source-start-%1"_s.arg(m_nextId++);
    m_playback.emplace();
    m_playback->startId = startId;
    m_playback->sourceId = sourceId;
    m_playback->session = session;
    const QPointer guard(this);
    requestOffer([guard, startId, sessionHandle](std::optional<Offer> offer, const QString &code,
                                                  const QString &message) {
        if (!guard) return;
        if (!guard->m_playback || guard->m_playback->startId != startId) {
            if (offer) guard->cancelOffer(offer->offer);
            return;
        }
        if (!offer) {
            guard->failStart(startId, code, message);
            return;
        }
        guard->m_playback->offer = offer->offer;
        guard->prepare(startId, sessionHandle, *offer, 0);
    });
    return startId;
}

void SourceBridge::prepare(const QString &startId, const QString &sessionHandle, const Offer &offer, int attempt)
{
    if (!m_playback || m_playback->startId != startId) return;
    const QPointer guard(this);
    const QJsonObject params{{u"sessionHandle"_s, sessionHandle}, {u"offer"_s, offer.offer},
                             {u"runtimeCapabilities"_s, offer.runtimeCapabilities}};
    const auto prepareId = m_core.requestPrivate(
        u"streamer.source.prepare"_s, params,
        [guard, startId, sessionHandle, offer, attempt](bool ok, const QJsonObject &lease, const QString &code,
                                                        const QString &message) {
            if (!guard) return;
            auto &playback = guard->m_playback;
            if (!playback || playback->startId != startId) return;
            if (!ok && code == u"session_update_busy"_s && attempt + 1 < MaximumPrepareAttempts) {
                playback->prepareId.clear();
                QTimer::singleShot(PrepareRetryMs, guard, [guard, startId, sessionHandle, offer, attempt] {
                    if (guard) guard->prepare(startId, sessionHandle, offer, attempt + 1);
                });
                return;
            }
            if (!ok) {
                guard->failStart(startId, code, message);
                return;
            }
            const auto leaseId = lease.value(u"leaseId"_s).toString();
            if (leaseId.isEmpty() || lease.value(u"sourceId"_s).toString() != playback->sourceId
                    || lease.value(u"session"_s).toObject() != playback->session) {
                guard->m_core.settleReceipt(playback->prepareId, false);
                guard->failStart(startId, u"invalid_source_lease"_s,
                                 u"The core prepared media for a different session"_s);
                return;
            }
            playback->leaseId = leaseId;
            playback->profile = acceptedVideoProfile(lease);
            const QJsonObject command{{u"id"_s, startId}, {u"type"_s, u"start"_s},
                                      {u"protocolVersion"_s, NativeProtocolVersion},
                                      {u"context"_s, QJsonObject{{u"lease"_s, lease}}}};
            if (!guard->m_runtime.send(command)) {
                guard->m_core.settleReceipt(playback->prepareId, false);
                guard->failStart(startId, u"native_start_failed"_s, guard->m_runtime.lastError());
            }
        },
        SourceRequestTimeoutMs);
    if (prepareId.isEmpty()) {
        failStart(startId, u"core_not_ready"_s, u"The core is not ready"_s);
        return;
    }
    m_playback->prepareId = prepareId;
}

bool SourceBridge::openAuthorization(const QString &sourceId, const QString &openHandle)
{
    if (sourceId.isEmpty() || openHandle.isEmpty()) return false;
    const QPointer guard(this);
    const auto id = m_core.requestPrivate(
        u"sources.auth.open"_s, QJsonObject{{u"sourceId"_s, sourceId}, {u"openHandle"_s, openHandle}},
        [guard, sourceId](bool ok, const QJsonObject &result, const QString &, const QString &message) {
            if (!guard) return;
            if (!ok) {
                emit guard->authorizationOpenFailed(sourceId, message);
                return;
            }
            const QUrl url(result.value(u"url"_s).toString(), QUrl::StrictMode);
            if (!url.isValid() || url.scheme() != u"https"_s || url.host().isEmpty() || !url.userInfo().isEmpty()
                    || !QDesktopServices::openUrl(url))
                emit guard->authorizationOpenFailed(sourceId, u"OpenNOW could not open the sign-in page in your browser."_s);
        });
    return !id.isEmpty();
}

void SourceBridge::requestOffer(OfferHandler handler)
{
    const QPointer guard(this);
    const auto fail = [guard, handler](const QString &code, const QString &message) {
        QMetaObject::invokeMethod(guard, [handler, code, message] { handler(std::nullopt, code, message); },
                                  Qt::QueuedConnection);
    };
    const auto policyId = m_core.requestPrivate(
        u"streamer.source.policy"_s, {},
        [guard, handler, fail](bool ok, const QJsonObject &result, const QString &code, const QString &message) {
            if (!guard) return;
            if (!ok) {
                handler(std::nullopt, code, message);
                return;
            }
            auto &runtime = guard->m_runtime;
            if (!runtime.running() && !runtime.start()) {
                fail(u"native_unavailable"_s, runtime.lastError());
                return;
            }
            const QJsonObject command{
                {u"id"_s, u"source-offer-%1"_s.arg(guard->m_nextId++)}, {u"type"_s, u"media-offer"_s},
                {u"protocolVersion"_s, NativeProtocolVersion},
                {u"context"_s, QJsonObject{{u"localPolicy"_s, result.value(u"localPolicy"_s).toObject()}}}};
            const bool sent = runtime.sendPrivate(command, [guard, handler](const QJsonObject &response) {
                if (!guard) return;
                const auto offer = response.value(u"offer"_s).toObject();
                if (response.value(u"type"_s).toString() != u"media-offer"_s || offer.isEmpty()) {
                    handler(std::nullopt, response.value(u"code"_s).toString(u"media_offer_failed"_s),
                            response.value(u"message"_s).toString(u"The media runtime could not offer playback"_s));
                    return;
                }
                handler(Offer{offer, response.value(u"runtimeCapabilities"_s).toObject()}, {}, {});
            });
            if (!sent) fail(u"native_unavailable"_s, runtime.lastError());
        });
    if (policyId.isEmpty()) fail(u"core_not_ready"_s, u"The core is not ready"_s);
}

void SourceBridge::cancelOffer(const QJsonObject &offer)
{
    const auto offerId = offer.value(u"offerId"_s).toString();
    if (offerId.isEmpty() || !m_runtime.running()) return;
    m_runtime.sendPrivate(QJsonObject{{u"id"_s, u"source-offer-cancel-%1"_s.arg(m_nextId++)},
                                      {u"type"_s, u"media-cancel-offer"_s},
                                      {u"offerId"_s, offerId}},
                          [](const QJsonObject &) {});
}

void SourceBridge::failStart(QString startId, const QString &code, const QString &message)
{
    if (m_activeProfile.value(u"startId"_s).toString() == startId) setActiveProfile({});
    if (m_playback && m_playback->startId == startId) {
        cancelOffer(m_playback->offer);
        m_playback.reset();
    }
    QMetaObject::invokeMethod(this, [this, startId, code, message] { emit failed(startId, code, message); },
                              Qt::QueuedConnection);
}

void SourceBridge::onNativeResponse(const QJsonObject &response)
{
    if (!m_playback || m_playback->accepted || m_playback->leaseId.isEmpty()
            || response.value(u"id"_s).toString() != m_playback->startId)
        return;
    if (response.value(u"type"_s).toString() == u"ok"_s
            && response.value(u"leaseId"_s).toString() == m_playback->leaseId) {
        m_playback->accepted = true;
        m_playback->offer = {};
        m_core.settleReceipt(m_playback->prepareId, true);
        if (!m_playback->profile.isEmpty()) {
            auto profile = m_playback->profile;
            profile.insert(u"sourceId"_s, m_playback->sourceId);
            profile.insert(u"startId"_s, m_playback->startId);
            setActiveProfile(profile);
        }
        return;
    }
    m_core.settleReceipt(m_playback->prepareId, false);
    const bool mismatchedLease = response.value(u"type"_s).toString() == u"ok"_s;
    failStart(m_playback->startId,
              mismatchedLease ? u"invalid_source_lease"_s
                              : response.value(u"code"_s).toString(u"native_start_failed"_s),
              mismatchedLease ? u"The media runtime accepted a different session lease"_s
                              : response.value(u"message"_s).toString(u"The media runtime rejected playback"_s));
}

void SourceBridge::onNativeEvent(const QJsonObject &event)
{
    if (event.value(u"type"_s).toString() == u"status"_s && event.value(u"status"_s).toString() == u"stopped"_s) {
        m_reconcileRetries = 0;
        QMetaObject::invokeMethod(this, &SourceBridge::reconcileMedia, Qt::QueuedConnection);
    }
    if (!m_playback || !m_playback->accepted || m_playback->releasing
            || event.value(u"type"_s).toString() != u"status"_s
            || event.value(u"status"_s).toString() != u"stopped"_s
            || event.value(u"startId"_s).toString() != m_playback->startId
            || event.value(u"leaseId"_s).toString() != m_playback->leaseId)
        return;
    m_playback->releasing = true;
    setActiveProfile({});
    confirmRetirement(0);
}

void SourceBridge::confirmRetirement(int attempt)
{
    if (!m_playback) return;
    const auto leaseId = m_playback->leaseId;
    const QPointer guard(this);
    m_runtime.sendPrivate(
        QJsonObject{{u"id"_s, u"source-status-%1"_s.arg(m_nextId++)}, {u"type"_s, u"media-status"_s}},
        [guard, leaseId, attempt](const QJsonObject &response) {
            if (!guard || !guard->m_playback || guard->m_playback->leaseId != leaseId) return;
            if (response.value(u"type"_s).toString() == u"media-status"_s
                    && response.value(u"nativeIdle"_s).toBool(false)) {
                guard->m_playback->retired = true;
                guard->release();
                return;
            }
            if (attempt + 1 < MaximumRetirementChecks)
                QTimer::singleShot(RetirementCheckIntervalMs, guard, [guard, attempt] {
                    if (guard) guard->confirmRetirement(attempt + 1);
                });
        });
}

void SourceBridge::release()
{
    if (!m_playback || !m_playback->retired || m_playback->releaseInFlight) return;
    const auto leaseId = m_playback->leaseId;
    const QPointer guard(this);
    m_playback->releaseInFlight = true;
    const auto id = m_core.requestPrivate(
        u"streamer.source.release"_s,
        QJsonObject{{u"sourceId"_s, m_playback->sourceId}, {u"session"_s, m_playback->session},
                    {u"leaseId"_s, leaseId}},
        [guard, leaseId](bool ok, const QJsonObject &, const QString &code, const QString &) {
            if (!guard || !guard->m_playback || guard->m_playback->leaseId != leaseId) return;
            auto &playback = *guard->m_playback;
            playback.releaseInFlight = false;
            if (ok || code == u"session_owner_mismatch"_s || code == u"invalid_params"_s) {
                guard->m_playback.reset();
                return;
            }
            if (++playback.releaseAttempts < MaximumReleaseAttempts)
                QTimer::singleShot(ReleaseRetryMs, guard, [guard] { if (guard) guard->release(); });
        });
    if (id.isEmpty()) m_playback->releaseInFlight = false;
}

bool SourceBridge::mediaTransitionPending() const
{
    return m_core.mediaPreparationPending() || m_runtime.startPending() || (m_playback && !m_playback->accepted);
}

void SourceBridge::scheduleReconcile()
{
    if (++m_reconcileRetries > MaximumReconcileRetries) return;
    QTimer::singleShot(ReconcileRetryMs, this, &SourceBridge::reconcileMedia);
}

void SourceBridge::reconcileMedia()
{
    if (m_reconciling) {
        m_reconcileAgain = true;
        return;
    }
    if (m_core.state() != u"ready"_s || !m_runtime.running()) return;
    if (mediaTransitionPending()) {
        scheduleReconcile();
        return;
    }
    const auto epoch = m_core.mediaEpoch() + m_runtime.startEpoch();
    m_reconciling = true;
    const QPointer guard(this);
    const auto retry = [guard] {
        if (!guard) return;
        guard->m_reconciling = false;
        guard->scheduleReconcile();
    };
    const auto observed = [guard, epoch, retry](bool ok, const QJsonObject &observation, const QString &, const QString &) {
        if (!guard) return;
        if (!ok || !observation.contains(u"mediaRevision"_s)) {
            retry();
            return;
        }
        const auto revision = observation.value(u"mediaRevision"_s);
        const bool sent = guard->m_runtime.sendPrivate(
            QJsonObject{{u"id"_s, u"source-reconcile-%1"_s.arg(guard->m_nextId++)}, {u"type"_s, u"media-status"_s}},
            [guard, epoch, revision, retry](const QJsonObject &response) {
                if (!guard) return;
                if (response.value(u"type"_s).toString() != u"media-status"_s
                        || guard->mediaTransitionPending()
                        || guard->m_core.mediaEpoch() + guard->m_runtime.startEpoch() != epoch) {
                    retry();
                    return;
                }
                auto status = response;
                status.remove(u"id"_s);
                status.remove(u"type"_s);
                const auto id = guard->m_core.requestPrivate(
                    u"streamer.source.reconcile"_s, QJsonObject{{u"mediaRevision"_s, revision}, {u"status"_s, status}},
                    [guard, retry](bool ok, const QJsonObject &, const QString &, const QString &) {
                        if (!guard) return;
                        if (!ok) {
                            retry();
                            return;
                        }
                        guard->m_reconciling = false;
                        guard->m_reconcileRetries = 0;
                        if (std::exchange(guard->m_reconcileAgain, false)) guard->reconcileMedia();
                    });
                if (id.isEmpty()) retry();
            });
        if (!sent) retry();
    };
    if (m_core.requestPrivate(u"streamer.source.observe"_s, {}, observed).isEmpty()) m_reconciling = false;
}

void SourceBridge::setActiveProfile(const QVariantMap &profile)
{
    if (m_activeProfile == profile) return;
    m_activeProfile = profile;
    emit activeProfileChanged();
}

QVariantMap SourceBridge::acceptedVideoProfile(const QJsonObject &lease)
{
    const auto video = lease.value(u"media"_s).toObject().value(u"prepared"_s).toObject()
        .value(u"accepted"_s).toObject().value(u"video"_s).toObject();
    const auto bounded = [&video](const QString &key, int minimum, int maximum) -> std::optional<int> {
        const auto value = video.value(key);
        if (!value.isDouble()) return std::nullopt;
        const auto number = value.toDouble();
        if (number != std::floor(number) || number < minimum || number > maximum) return std::nullopt;
        return static_cast<int>(number);
    };
    static const QHash<QString, QString> codecs{{u"h264-annex-b"_s, u"h264"_s}, {u"hevc-annex-b"_s, u"h265"_s},
                                                {u"av1-obu"_s, u"av1"_s}};
    const auto width = bounded(u"width"_s, 1, 16384);
    const auto height = bounded(u"height"_s, 1, 16384);
    const auto codec = codecs.value(video.value(u"encoding"_s).toString());
    if (!width || !height || codec.isEmpty()) return {};
    QVariantMap profile{{u"width"_s, *width}, {u"height"_s, *height}, {u"codec"_s, codec}};
    if (const auto fps = bounded(u"fps"_s, 0, 1000)) profile.insert(u"fps"_s, *fps);
    const auto bitDepth = bounded(u"bitDepth"_s, 8, 10);
    const auto chroma = video.value(u"chroma"_s).toString();
    if (bitDepth && (*bitDepth == 8 || *bitDepth == 10) && (chroma == u"yuv420"_s || chroma == u"yuv444"_s)) {
        profile.insert(u"bitDepth"_s, *bitDepth);
        profile.insert(u"colorQuality"_s, u"%1bit_%2"_s.arg(*bitDepth).arg(chroma.mid(3)));
    }
    static const QRegularExpression token(u"^[a-z0-9-]{1,32}$"_s);
    const auto color = video.value(u"color"_s).toObject();
    for (const auto &key : {u"range"_s, u"primaries"_s, u"transfer"_s, u"matrix"_s}) {
        const auto value = color.value(key).toString();
        if (token.match(value).hasMatch()) profile.insert(u"color"_s + key.left(1).toUpper() + key.mid(1), value);
    }
    return profile;
}
