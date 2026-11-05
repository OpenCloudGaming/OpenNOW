#include "streaming/rendering/FlyHighNetwork.h"

#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>

#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStandardPaths>

#include <array>

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <d3d11.h>
#include <dxgi1_2.h>
#endif

namespace {
QString modelDirectory()
{
    const auto usable = [](const QString &dir) {
        return QFileInfo::exists(dir + QStringLiteral("/graph.json"))
            && QFileInfo::exists(dir + QStringLiteral("/flyhighfg.json"))
            && QFileInfo::exists(dir + QStringLiteral("/flyhighfg.bin"));
    };
    const QStringList directories = {
        qEnvironmentVariable("OPENNOW_FLYHIGH_DIR"),
        QCoreApplication::applicationDirPath() + QStringLiteral("/flyhigh"),
        QStringLiteral("E:/flyhigh-engine/models")};
    for (const QString &dir : directories)
        if (!dir.isEmpty() && usable(dir)) return dir;
    return {};
}

#if defined(Q_OS_WIN)
struct FhEngineOptionsRaw {
    uint32_t backend;
    uint32_t tensorCores;
    uint32_t autotune;
    const char *adapter;
    uint32_t precision;
    const char *tuneCache;
    uint32_t compactMemory;
    const char *dxcPath;
};

struct FhSessionOptionsRaw {
    uint32_t width, height;
    uint32_t internalW, internalH;
    uint32_t k;
    uint32_t repair;
    uint32_t render;
    uint32_t keepStatic;
    uint32_t staticFrames;
    uint32_t staticTolerance;
    uint32_t fastRender;
    uint32_t linearT;
    uint32_t keepMaps;
    uint32_t protectionMask;
    const char *tune;
};

struct Api {
    HMODULE library = nullptr;
    const char *(*lastError)() = nullptr;
    FhEngineOptionsRaw (*engineOptionsDefault)() = nullptr;
    void *(*engineCreate)(const void *options) = nullptr;
    void (*engineDestroy)(void *engine) = nullptr;
    int (*engineInfo)(const void *engine, char *buf, size_t cap) = nullptr;
    void *(*modelLoad)(const char *dir) = nullptr;
    void (*modelDestroy)(void *model) = nullptr;
    FhSessionOptionsRaw (*sessionOptionsDefault)(uint32_t width, uint32_t height) = nullptr;
    void *(*sessionCreate)(const void *engine, const void *model, const void *options) = nullptr;
    void (*sessionDestroy)(void *session) = nullptr;
    int (*importD3d11)(void *session, const void *engine, intptr_t handle, uint32_t width, uint32_t height) = nullptr;
    int (*addOutput)(void *session) = nullptr;
    int (*pair)(const void *session, uint32_t a, uint32_t b) = nullptr;
    int (*mid)(const void *session, float t, uint32_t out) = nullptr;
    int (*pairTimed)(const void *session, uint32_t a, uint32_t b, double *ms) = nullptr;
    int (*midTimed)(const void *session, float t, uint32_t out, double *ms) = nullptr;
    int (*report)(const void *session, char *buf, size_t cap) = nullptr;
    int (*copyOutput)(const void *session, const void *engine, uint32_t out, intptr_t handle, uint32_t width, uint32_t height) = nullptr;
    void (*releaseTarget)(intptr_t handle) = nullptr;

    bool load(QString *error)
    {
        if (library) return true;
        const QStringList files = {
            QCoreApplication::applicationDirPath() + QStringLiteral("/flyhigh.dll"),
            QStringLiteral("E:/flyhigh-engine/target/release/flyhigh.dll")};
        for (const QString &file : files) {
            if (!QFileInfo::exists(file)) continue;
            library = LoadLibraryW(reinterpret_cast<LPCWSTR>(file.utf16()));
            if (library) break;
        }
        if (!library) {
            *error = QStringLiteral("FlyHigh engine DLL was not found");
            return false;
        }
        const auto sym = [&](const char *name) { return GetProcAddress(library, name); };
        lastError = reinterpret_cast<decltype(lastError)>(sym("fh_last_error"));
        engineOptionsDefault = reinterpret_cast<decltype(engineOptionsDefault)>(sym("fh_engine_options_default"));
        engineCreate = reinterpret_cast<decltype(engineCreate)>(sym("fh_engine_create"));
        engineDestroy = reinterpret_cast<decltype(engineDestroy)>(sym("fh_engine_destroy"));
        engineInfo = reinterpret_cast<decltype(engineInfo)>(sym("fh_engine_info"));
        modelLoad = reinterpret_cast<decltype(modelLoad)>(sym("fh_model_load"));
        modelDestroy = reinterpret_cast<decltype(modelDestroy)>(sym("fh_model_destroy"));
        sessionOptionsDefault = reinterpret_cast<decltype(sessionOptionsDefault)>(sym("fh_session_options_default"));
        sessionCreate = reinterpret_cast<decltype(sessionCreate)>(sym("fh_session_create"));
        sessionDestroy = reinterpret_cast<decltype(sessionDestroy)>(sym("fh_session_destroy"));
        importD3d11 = reinterpret_cast<decltype(importD3d11)>(sym("fh_session_import_d3d11"));
        addOutput = reinterpret_cast<decltype(addOutput)>(sym("fh_session_add_output"));
        pair = reinterpret_cast<decltype(pair)>(sym("fh_session_pair"));
        mid = reinterpret_cast<decltype(mid)>(sym("fh_session_mid"));
        pairTimed = reinterpret_cast<decltype(pairTimed)>(sym("fh_session_pair_timed"));
        midTimed = reinterpret_cast<decltype(midTimed)>(sym("fh_session_mid_timed"));
        report = reinterpret_cast<decltype(report)>(sym("fh_session_report_json"));
        copyOutput = reinterpret_cast<decltype(copyOutput)>(sym("fh_copy_output_d3d11"));
        releaseTarget = reinterpret_cast<decltype(releaseTarget)>(sym("fh_release_d3d11_target"));
        if (!lastError || !engineOptionsDefault || !engineCreate || !modelLoad || !sessionOptionsDefault
            || !sessionCreate || !sessionDestroy || !importD3d11 || !addOutput || !pair || !mid
            || !copyOutput || !releaseTarget) {
            *error = QStringLiteral("FlyHigh engine DLL is missing entry points");
            return false;
        }
        return true;
    }

    QString failed() const
    {
        const char *text = lastError ? lastError() : nullptr;
        return text && *text ? QString::fromUtf8(text) : QStringLiteral("FlyHigh engine call failed");
    }
};

Api &api()
{
    static Api instance;
    return instance;
}

struct SharedTex {
    ID3D11Texture2D *texture = nullptr;
    HANDLE handle = nullptr;
    std::unique_ptr<QRhiTexture> wrapped;

    void reset(void (*releaseTarget)(intptr_t))
    {
        wrapped.reset();
        if (handle && releaseTarget) releaseTarget(reinterpret_cast<intptr_t>(handle));
        if (handle) {
            CloseHandle(handle);
            handle = nullptr;
        }
        if (texture) {
            texture->Release();
            texture = nullptr;
        }
    }
};
#endif
}

struct FlyHighNetwork::State {
#if defined(Q_OS_WIN)
    QRhi *rhi = nullptr;
    ID3D11Device *device = nullptr;
    ID3D11DeviceContext *context = nullptr;
    ID3D11Query *flush = nullptr;
    void *engine = nullptr;
    void *model = nullptr;
    void *session = nullptr;
    QByteArray modelDir;
    QByteArray tuneCache;
    QByteArray adapter;
    QSize size;
    QSize network;
    int generated = 0;
    double gpuMs = 0;
    std::array<SharedTex, 2> inputs;
    std::array<SharedTex, FlyHighNetwork::MaximumFrames> outputs;
    std::array<int, 2> inputSlot{{-1, -1}};
    std::array<int, FlyHighNetwork::MaximumFrames> outputSlot{{-1, -1, -1}};

    ~State() { resetSession(); releaseDevice(); }

    void resetSession()
    {
        if (session) api().sessionDestroy(session);
        session = nullptr;
        for (auto &tex : inputs) tex.reset(api().releaseTarget);
        for (auto &tex : outputs) tex.reset(api().releaseTarget);
        inputSlot.fill(-1);
        outputSlot.fill(-1);
        generated = 0;
        network = {};
    }

    void releaseDevice()
    {
        resetSession();
        if (model) api().modelDestroy(model);
        model = nullptr;
        if (engine) api().engineDestroy(engine);
        engine = nullptr;
        if (flush) flush->Release();
        if (context) context->Release();
        if (device) device->Release();
        flush = nullptr;
        context = nullptr;
        device = nullptr;
        rhi = nullptr;
    }

    bool waitCopy(QString *error)
    {
        context->End(flush);
        context->Flush();
        for (int spin = 0; spin < 1000000; ++spin) {
            BOOL done = FALSE;
            const HRESULT hr = context->GetData(flush, &done, sizeof(done), 0);
            if (hr == S_OK && done) return true;
            if (FAILED(hr)) {
                *error = QStringLiteral("FlyHigh could not wait for the source copy");
                return false;
            }
        }
        *error = QStringLiteral("FlyHigh source copy did not finish");
        return false;
    }

    bool makeShared(SharedTex *slot, bool target, QString *error)
    {
        D3D11_TEXTURE2D_DESC desc{};
        desc.Width = UINT(size.width());
        desc.Height = UINT(size.height());
        desc.MipLevels = 1;
        desc.ArraySize = 1;
        desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        desc.SampleDesc.Count = 1;
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        if (target) desc.BindFlags |= D3D11_BIND_RENDER_TARGET;
        desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_NTHANDLE;
        if (FAILED(device->CreateTexture2D(&desc, nullptr, &slot->texture))) {
            *error = QStringLiteral("FlyHigh could not allocate a shared texture");
            return false;
        }
        IDXGIResource1 *resource = nullptr;
        if (FAILED(slot->texture->QueryInterface(IID_PPV_ARGS(&resource)))) {
            *error = QStringLiteral("FlyHigh shared texture has no NT handle");
            return false;
        }
        const HRESULT opened = resource->CreateSharedHandle(
            nullptr, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE, nullptr, &slot->handle);
        resource->Release();
        if (FAILED(opened) || !slot->handle) {
            *error = QStringLiteral("FlyHigh could not share a texture with the engine");
            return false;
        }
        if (!target) return true;
        // The shared texture is only the Vulkan bridge. Presenting it lets the desktop
        // compositor and the NVIDIA overlay write into the picture the user sees.
        slot->wrapped.reset(rhi->newTexture(QRhiTexture::RGBA8, size, 1,
                                            QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
        if (!slot->wrapped->create()) {
            *error = QStringLiteral("FlyHigh could not allocate a generated frame");
            return false;
        }
        return true;
    }

    bool publishOutput(QRhiCommandBuffer *cb, int index, QString *error)
    {
        auto *shown = reinterpret_cast<ID3D11Texture2D *>(
            quintptr(outputs[size_t(index)].wrapped->nativeTexture().object));
        if (!shown) {
            *error = QStringLiteral("FlyHigh generated frame has no D3D11 texture");
            return false;
        }
        cb->beginExternal();
        context->CopyResource(shown, outputs[size_t(index)].texture);
        const bool copied = waitCopy(error);
        cb->endExternal();
        return copied;
    }

    bool buildSession(QString *error)
    {
        resetSession();
        for (auto &tex : inputs)
            if (!makeShared(&tex, false, error)) return false;
        for (auto &tex : outputs)
            if (!makeShared(&tex, true, error)) return false;
        FhSessionOptionsRaw options = api().sessionOptionsDefault(
            uint32_t(size.width()), uint32_t(size.height()));
        options.keepStatic = 1;
        session = api().sessionCreate(engine, model, &options);
        if (!session) {
            *error = api().failed();
            return false;
        }
        for (int i = 0; i < 2; ++i) {
            inputSlot[size_t(i)] = api().importD3d11(session, engine,
                reinterpret_cast<intptr_t>(inputs[size_t(i)].handle),
                uint32_t(size.width()), uint32_t(size.height()));
            if (inputSlot[size_t(i)] < 0) {
                *error = api().failed();
                return false;
            }
        }
        for (int i = 0; i < FlyHighNetwork::MaximumFrames; ++i) {
            outputSlot[size_t(i)] = api().addOutput(session);
            if (outputSlot[size_t(i)] < 0) {
                *error = api().failed();
                return false;
            }
        }
        char report[1024];
        if (api().report(session, report, sizeof(report)) > 0) {
            const auto json = QJsonDocument::fromJson(QByteArray(report)).object();
            network = QSize(json.value(QStringLiteral("internal_width")).toInt(),
                            json.value(QStringLiteral("internal_height")).toInt());
        }
        if (network.isEmpty()) network = size;
        return true;
    }
#else
    ~State() = default;
#endif
};

FlyHighNetwork::FlyHighNetwork() = default;
FlyHighNetwork::~FlyHighNetwork() = default;

bool FlyHighNetwork::enabled()
{
    const QString flag = qEnvironmentVariable("OPENNOW_FLYHIGH");
    if (flag == QLatin1String("0")
        || qEnvironmentVariable("OPENNOW_FRAME_GENERATION_ENGINE") == QLatin1String("motion"))
        return false;
    const bool app = QFileInfo(QCoreApplication::applicationFilePath()).completeBaseName()
        .compare(QLatin1String("OpenNOW"), Qt::CaseInsensitive) == 0;
    return (app || flag == QLatin1String("1")) && !weightsPath().isEmpty();
}

QString FlyHighNetwork::weightsPath()
{
    const QString dir = modelDirectory();
    return dir.isEmpty() ? QString() : dir + QStringLiteral("/flyhighfg.bin");
}

bool FlyHighNetwork::initialize(QRhi *rhi)
{
    release();
    m_error.clear();
#if !defined(Q_OS_WIN)
    Q_UNUSED(rhi);
    m_error = QStringLiteral("FlyHigh engine bridge is Windows only");
    return false;
#else
    if (!rhi || rhi->backend() != QRhi::D3D11) {
        m_error = QStringLiteral("FlyHigh engine needs the D3D11 presenter");
        return false;
    }
    if (!api().load(&m_error)) return false;
    const auto *handles = static_cast<const QRhiD3D11NativeHandles *>(rhi->nativeHandles());
    if (!handles || !handles->dev || !handles->context) {
        m_error = QStringLiteral("FlyHigh could not see the D3D11 device");
        return false;
    }
    m_state = std::make_unique<State>();
    auto &s = *m_state;
    s.rhi = rhi;
    s.device = static_cast<ID3D11Device *>(handles->dev);
    s.context = static_cast<ID3D11DeviceContext *>(handles->context);
    s.device->AddRef();
    s.context->AddRef();
    D3D11_QUERY_DESC query{D3D11_QUERY_EVENT, 0};
    if (FAILED(s.device->CreateQuery(&query, &s.flush))) {
        m_error = QStringLiteral("FlyHigh could not create a D3D11 fence");
        release();
        return false;
    }
    s.modelDir = modelDirectory().toUtf8();
    s.tuneCache = (QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
                   + QStringLiteral("/flyhigh-tune")).toUtf8();
    QDir().mkpath(QString::fromUtf8(s.tuneCache));
    const QString deviceName = QString::fromUtf8(rhi->driverInfo().deviceName);
    if (deviceName.contains(QStringLiteral("4060"))) s.adapter = "4060";
    FhEngineOptionsRaw options = api().engineOptionsDefault();
    options.backend = 1;
    options.adapter = s.adapter.isEmpty() ? nullptr : s.adapter.constData();
    options.tuneCache = s.tuneCache.constData();
    s.engine = api().engineCreate(&options);
    if (!s.engine) {
        m_error = api().failed();
        release();
        return false;
    }
    s.model = api().modelLoad(s.modelDir.constData());
    if (!s.model) {
        m_error = api().failed();
        release();
        return false;
    }
    return true;
#endif
}

bool FlyHighNetwork::pair(QRhiCommandBuffer *cb, QRhiTexture *previous, QRhiTexture *current, int count)
{
    m_error.clear();
#if !defined(Q_OS_WIN)
    Q_UNUSED(cb); Q_UNUSED(previous); Q_UNUSED(current); Q_UNUSED(count);
    m_error = QStringLiteral("FlyHigh engine bridge is Windows only");
    return false;
#else
    if (!m_state || !cb || !previous || !current || count < 1 || count > MaximumFrames) {
        m_error = QStringLiteral("FlyHigh pair is missing a frame");
        return false;
    }
    if (previous->format() != QRhiTexture::RGBA8 || current->format() != QRhiTexture::RGBA8
        || previous->pixelSize() != current->pixelSize()) {
        m_error = QStringLiteral("FlyHigh engine takes RGBA8 frames");
        return false;
    }
    auto &s = *m_state;
    s.gpuMs = 0;
    if (s.size != previous->pixelSize() && !s.size.isEmpty()) s.resetSession();
    s.size = previous->pixelSize();
    if (!s.session && !s.buildSession(&m_error)) return false;
    auto *sourceA = reinterpret_cast<ID3D11Texture2D *>(quintptr(previous->nativeTexture().object));
    auto *sourceB = reinterpret_cast<ID3D11Texture2D *>(quintptr(current->nativeTexture().object));
    if (!sourceA || !sourceB) {
        m_error = QStringLiteral("FlyHigh source frame has no D3D11 texture");
        return false;
    }
    cb->beginExternal();
    // The history copy is still bound as a render target. D3D11 drops CopyResource
    // from a bound target, so the engine would interpolate the previous pair.
    s.context->OMSetRenderTargets(0, nullptr, nullptr);
    s.context->CopyResource(s.inputs[0].texture, sourceA);
    s.context->CopyResource(s.inputs[1].texture, sourceB);
    const bool copied = s.waitCopy(&m_error);
    cb->endExternal();
    if (!copied) return false;
    QElapsedTimer timer;
    timer.start();
    if (api().pair(s.session, uint32_t(s.inputSlot[0]), uint32_t(s.inputSlot[1])) != 0) {
        m_error = api().failed();
        return false;
    }
    for (int index = 0; index < count; ++index) {
        const float phase = float(index + 1) / float(count + 1);
        if (api().mid(s.session, phase, uint32_t(s.outputSlot[size_t(index)])) != 0
            || api().copyOutput(s.session, s.engine, uint32_t(s.outputSlot[size_t(index)]),
                                reinterpret_cast<intptr_t>(s.outputs[size_t(index)].handle),
                                uint32_t(s.size.width()), uint32_t(s.size.height())) != 0) {
            m_error = api().failed();
            return false;
        }
        if (!s.publishOutput(cb, index, &m_error)) return false;
    }
    s.gpuMs = double(timer.nsecsElapsed()) / 1.0e6;
    s.generated = count;
    return true;
#endif
}

bool FlyHighNetwork::frame(QRhiCommandBuffer *, int index)
{
#if defined(Q_OS_WIN)
    return m_state && index >= 1 && index <= m_state->generated;
#else
    Q_UNUSED(index);
    return false;
#endif
}

QRhiTexture *FlyHighNetwork::output(int index) const
{
#if defined(Q_OS_WIN)
    if (!m_state || index < 1 || index > m_state->generated) return nullptr;
    return m_state->outputs[size_t(index - 1)].wrapped.get();
#else
    Q_UNUSED(index);
    return nullptr;
#endif
}

int FlyHighNetwork::count() const
{
#if defined(Q_OS_WIN)
    return m_state ? m_state->generated : 0;
#else
    return 0;
#endif
}

QSize FlyHighNetwork::networkSize() const
{
#if defined(Q_OS_WIN)
    return m_state ? m_state->network : QSize();
#else
    return {};
#endif
}

double FlyHighNetwork::lastGpuMs() const
{
#if defined(Q_OS_WIN)
    return m_state ? m_state->gpuMs : 0;
#else
    return 0;
#endif
}

QString FlyHighNetwork::error() const { return m_error; }

void FlyHighNetwork::release()
{
    m_state.reset();
}
