#include "app/platform/GraphicsDeviceSelection.h"

#include <QCryptographicHash>
#include <QQuickGraphicsDevice>
#include <QQuickWindow>
#include <QSet>
#include <QStringList>
#include <QVariantMap>

#include <algorithm>
#include <bit>
#include <iterator>
#include <utility>

#ifdef Q_OS_WIN
#include <windows.h>
#include <d3d11_1.h>
#include <dxgi1_6.h>
#include <wrl/client.h>
#endif

using namespace Qt::StringLiterals;

#ifdef Q_OS_WIN
namespace {
// Same decoder profiles as the Windows streamer's hardware_profiles / adapter index.
const GUID kProfileH264NoFgt = {
    0x1b81be68, 0xa0c7, 0x11d3, {0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5}};
const GUID kProfileH264Fgt = {
    0x1b81be69, 0xa0c7, 0x11d3, {0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5}};
const GUID kProfileHevcMain = {
    0x5b11d51b, 0x2f4c, 0x4452, {0xbc, 0xc3, 0x09, 0xf2, 0xa1, 0x16, 0x0c, 0xc0}};
const GUID kProfileHevcMain10 = {
    0x107af0e0, 0xef1a, 0x4d19, {0xab, 0xa8, 0x67, 0xa1, 0x63, 0x07, 0x3d, 0x13}};
const GUID kProfileAv1Profile0 = {
    0xb8be4ccb, 0xcf53, 0x46ba, {0x8d, 0x59, 0xd6, 0xb8, 0xa6, 0xda, 0x5d, 0x2a}};

bool decoderProfileSupports(ID3D11VideoDevice *video, const GUID &profile, DXGI_FORMAT format)
{
    BOOL supported = FALSE;
    if (FAILED(video->CheckVideoDecoderFormat(&profile, format, &supported)) || !supported)
        return false;
    const SIZE sizes[] = {{1920, 1080}, {1280, 720}};
    for (const auto &size : sizes) {
        D3D11_VIDEO_DECODER_DESC description{};
        description.Guid = profile;
        description.SampleWidth = UINT(size.cx);
        description.SampleHeight = UINT(size.cy);
        description.OutputFormat = format;
        UINT configurations = 0;
        if (SUCCEEDED(video->GetVideoDecoderConfigCount(&description, &configurations))
                && configurations > 0)
            return true;
    }
    return false;
}

void indexAdapterDecode(IDXGIAdapter *adapter, bool *h264, bool *h265, bool *av1)
{
    *h264 = *h265 = *av1 = false;
    if (!adapter) return;
    using Microsoft::WRL::ComPtr;
    const D3D_FEATURE_LEVEL levels[] = {
        D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0};
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    const auto created = D3D11CreateDevice(
        adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
        levels, UINT(std::size(levels)), D3D11_SDK_VERSION,
        device.GetAddressOf(), nullptr, context.GetAddressOf());
    if (FAILED(created) || !device) {
        qInfo("Graphics adapter decode index skipped device creation: 0x%08lx",
              static_cast<unsigned long>(created));
        return;
    }
    ComPtr<ID3D11VideoDevice> video;
    if (FAILED(device.As(&video)) || !video) {
        qInfo("Graphics adapter decode index has no D3D11 video device");
        return;
    }
    const UINT count = std::min(video->GetVideoDecoderProfileCount(), 64u);
    for (UINT profileIndex = 0; profileIndex < count; ++profileIndex) {
        GUID profile{};
        if (FAILED(video->GetVideoDecoderProfile(profileIndex, &profile))) continue;
        if (IsEqualGUID(profile, kProfileH264NoFgt) || IsEqualGUID(profile, kProfileH264Fgt)) {
            *h264 = *h264 || decoderProfileSupports(video.Get(), profile, DXGI_FORMAT_NV12);
        } else if (IsEqualGUID(profile, kProfileHevcMain)) {
            *h265 = *h265 || decoderProfileSupports(video.Get(), profile, DXGI_FORMAT_NV12);
        } else if (IsEqualGUID(profile, kProfileHevcMain10)) {
            *h265 = *h265 || decoderProfileSupports(video.Get(), profile, DXGI_FORMAT_P010);
        } else if (IsEqualGUID(profile, kProfileAv1Profile0)) {
            *av1 = *av1 || decoderProfileSupports(video.Get(), profile, DXGI_FORMAT_NV12)
                || decoderProfileSupports(video.Get(), profile, DXGI_FORMAT_P010);
        }
    }
}
}
#endif

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
        bool h264 = false;
        bool h265 = false;
        bool av1 = false;
        indexAdapterDecode(adapter.Get(), &h264, &h265, &av1);
        const auto name = QString::fromWCharArray(description.Description);
        qInfo("Graphics adapter %s decode index: h264=%d h265=%d av1=%d",
              qUtf8Printable(name), int(h264), int(h265), int(av1));
        adapters.append({id, name, luid, quint64(description.DedicatedVideoMemory), false,
                         h264, h265, av1});
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
    if (const auto *automatic = automaticAdapter())
        m_active = *automatic;
    if (m_requestedDeviceId.isEmpty()) {
        if (m_adapters.size() >= 2 && m_active.luid && m_active.luid != m_adapters.first().luid) {
            qInfo("Hybrid graphics: %s has no hardware decoder; Automatic selected %s",
                  qUtf8Printable(m_adapters.first().name), qUtf8Printable(m_active.name));
        }
        return;
    }
    for (const auto &adapter : std::as_const(m_adapters)) {
        if (adapter.id == m_requestedDeviceId) {
            m_active = adapter;
            return;
        }
    }
}

QVariantList GraphicsDeviceSelection::choices() const
{
    const auto *automatic = automaticAdapter();
    QVariantList result{QVariantMap{
        {u"value"_s, QString{}}, {u"label"_s, tr("Automatic")},
        {u"codecs"_s, automatic ? codecIds(*automatic) : QStringList{}},
        {u"detail"_s, automatic ? u"%1 · %2"_s.arg(automatic->name, codecSummary(*automatic))
                                : QString{}}}};
    for (const auto &adapter : m_adapters) {
        result.append(QVariantMap{
            {u"value"_s, adapter.id.isEmpty() ? u"unavailable:%1"_s.arg(result.size()) : adapter.id},
            {u"label"_s, adapter.name},
            {u"disabled"_s, adapter.id.isEmpty()},
            {u"codecs"_s, codecIds(adapter)},
            {u"detail"_s, adapterDetail(adapter)}});
    }
    return result;
}

const GraphicsDeviceSelection::Adapter *GraphicsDeviceSelection::automaticAdapter() const
{
    const Adapter *fallback = nullptr;
    for (const auto &adapter : m_adapters) {
        if (!fallback) fallback = &adapter;
        if (canDecode(adapter)) return &adapter;
    }
    return fallback;
}

bool GraphicsDeviceSelection::canDecode(const Adapter &adapter) const
{
    return adapter.h264 || adapter.h265 || adapter.av1;
}

QStringList GraphicsDeviceSelection::codecIds(const Adapter &adapter) const
{
    QStringList ids;
    if (adapter.h264) ids.append(u"h264"_s);
    if (adapter.h265) ids.append(u"h265"_s);
    if (adapter.av1) ids.append(u"av1"_s);
    return ids;
}

QString GraphicsDeviceSelection::codecSummary(const Adapter &adapter) const
{
    QStringList names;
    if (adapter.h264) names.append(tr("H.264"));
    if (adapter.h265) names.append(tr("H.265"));
    if (adapter.av1) names.append(tr("AV1"));
    return names.isEmpty() ? tr("No hardware decoder") : names.join(u", "_s);
}

QString GraphicsDeviceSelection::memorySummary(const Adapter &adapter) const
{
    if (!adapter.dedicatedMemoryBytes) return tr("Shared graphics memory");
    return tr("%1 GB dedicated memory").arg(
        double(adapter.dedicatedMemoryBytes) / (1024.0 * 1024.0 * 1024.0), 0, 'f', 1);
}

QString GraphicsDeviceSelection::adapterDetail(const Adapter &adapter) const
{
    if (adapter.id.isEmpty()) return tr("Device identity unavailable");
    return u"%1 · %2"_s.arg(codecSummary(adapter), memorySummary(adapter));
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
