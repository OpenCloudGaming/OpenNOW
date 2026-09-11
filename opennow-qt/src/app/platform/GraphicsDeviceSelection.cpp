#include "app/platform/GraphicsDeviceSelection.h"

#include <QCryptographicHash>
#include <QQuickGraphicsDevice>
#include <QQuickWindow>
#include <QSet>
#include <QVariantMap>

#include <bit>
#include <utility>

#ifdef Q_OS_WIN
#include <windows.h>
#include <dxgi1_6.h>
#include <wrl/client.h>
#endif

using namespace Qt::StringLiterals;

QList<GraphicsDeviceSelection::Adapter> GraphicsDeviceSelection::detectAdapters()
{
    QList<Adapter> adapters;
#ifdef Q_OS_WIN
    using Microsoft::WRL::ComPtr;
    ComPtr<IDXGIFactory1> factory;
    if (FAILED(CreateDXGIFactory1(IID_PPV_ARGS(&factory)))) {
        qWarning("Could not enumerate graphics adapters; using the default device");
        return adapters;
    }
    ComPtr<IDXGIFactory6> preferredFactory;
    factory.As(&preferredFactory);
    for (UINT index = 0; index < 64; ++index) {
        ComPtr<IDXGIAdapter1> adapter;
        const auto result = preferredFactory
            ? preferredFactory->EnumAdapterByGpuPreference(index, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                                                           IID_PPV_ARGS(&adapter))
            : factory->EnumAdapters1(index, &adapter);
        if (result == DXGI_ERROR_NOT_FOUND) break;
        if (FAILED(result)) {
            qWarning("Could not enumerate graphics adapter %u", index);
            break;
        }
        DXGI_ADAPTER_DESC1 description{};
        if (FAILED(adapter->GetDesc1(&description))) continue;
        if (description.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE | DXGI_ADAPTER_FLAG_REMOTE)) continue;
        const auto luid = (quint64(quint32(description.AdapterLuid.HighPart)) << 32)
            | description.AdapterLuid.LowPart;
        if (!luid) continue;
        DISPLAYCONFIG_ADAPTER_NAME identity{};
        identity.header.type = DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME;
        identity.header.size = sizeof(identity);
        identity.header.adapterId = description.AdapterLuid;
        QString id;
        if (DisplayConfigGetDeviceInfo(&identity.header) == ERROR_SUCCESS) {
            const auto path = QString::fromWCharArray(identity.adapterDevicePath).toCaseFolded();
            if (!path.isEmpty()) {
                id = u"win-pnp-sha256:"_s + QString::fromLatin1(QCryptographicHash::hash(
                    path.toUtf8(), QCryptographicHash::Sha256).toHex());
            }
        }
        adapters.append({id, QString::fromWCharArray(description.Description), luid,
                         quint64(description.DedicatedVideoMemory), false});
    }
#endif
    return adapters;
}

GraphicsDeviceSelection::GraphicsDeviceSelection(QList<Adapter> adapters, QString requestedDeviceId,
                                               QObject *parent)
    : QObject(parent), m_requestedDeviceId(std::move(requestedDeviceId))
{
    QSet<quint64> seen;
    for (auto &adapter : adapters) {
        if (adapter.software || !adapter.luid || seen.contains(adapter.luid)) continue;
        seen.insert(adapter.luid);
        m_adapters.append(std::move(adapter));
    }
    if (m_adapters.isEmpty()) return;
    m_active = m_adapters.first();
    if (m_requestedDeviceId.isEmpty()) return;
    for (const auto &adapter : std::as_const(m_adapters)) {
        if (adapter.id == m_requestedDeviceId) {
            m_active = adapter;
            return;
        }
    }
}

QVariantList GraphicsDeviceSelection::choices() const
{
    QVariantList result{QVariantMap{
        {u"value"_s, QString{}}, {u"label"_s, tr("Automatic")},
        {u"detail"_s, m_adapters.isEmpty() ? QString{} : m_adapters.first().name}}};
    for (const auto &adapter : m_adapters) {
        result.append(QVariantMap{
            {u"value"_s, adapter.id.isEmpty() ? u"unavailable:%1"_s.arg(result.size()) : adapter.id},
            {u"label"_s, adapter.name},
            {u"disabled"_s, adapter.id.isEmpty()},
            {u"detail"_s, adapter.id.isEmpty() ? tr("Device identity unavailable")
                : adapter.dedicatedMemoryBytes ? tr("%1 GB dedicated memory").arg(
                      double(adapter.dedicatedMemoryBytes) / (1024.0 * 1024.0 * 1024.0), 0, 'f', 1)
                : tr("Shared graphics memory")}});
    }
    return result;
}

bool GraphicsDeviceSelection::savedDeviceUnavailable() const
{
    return !m_requestedDeviceId.isEmpty() && m_requestedDeviceId != m_active.id;
}

bool GraphicsDeviceSelection::applyTo(QQuickWindow *window) const
{
    if (!m_active.luid) return true;
#ifdef Q_OS_WIN
    if (!window || window->isVisible() || window->isSceneGraphInitialized()) return false;
    window->setGraphicsDevice(QQuickGraphicsDevice::fromAdapter(
        quint32(m_active.luid), std::bit_cast<qint32>(quint32(m_active.luid >> 32))));
    qInfo("Selected graphics adapter: %s (LUID %016llx)", qUtf8Printable(m_active.name),
          static_cast<unsigned long long>(m_active.luid));
    if (savedDeviceUnavailable()) qWarning("Saved graphics adapter is unavailable; using Automatic");
    return true;
#else
    Q_UNUSED(window);
    return false;
#endif
}
