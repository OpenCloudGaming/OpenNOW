#pragma once

#include <rhi/qrhi.h>

inline QRhiSwapChain::Format preferredHdrSwapChainFormat(bool outputReady,
    bool linearSupported, bool hdr10Supported)
{
    if (!outputReady) return QRhiSwapChain::SDR;
    if (linearSupported) return QRhiSwapChain::HDRExtendedSrgbLinear;
    return hdr10Supported ? QRhiSwapChain::HDR10 : QRhiSwapChain::SDR;
}

struct HdrSwapChainCreation
{
    QRhiSwapChain::Format format;
    bool created;
};

template<typename SwapChain>
void destroyHdrSwapChainPreservingProxy(SwapChain &swapChain)
{
    const auto proxy = swapChain.proxyData();
    swapChain.destroy();
    swapChain.setProxyData(proxy);
}

template<typename Create>
HdrSwapChainCreation createHdrSwapChainWithSdrFallback(QRhiSwapChain::Format requested,
    Create &&create)
{
    if (create(requested)) return {requested, true};
    if (requested != QRhiSwapChain::SDR && create(QRhiSwapChain::SDR))
        return {QRhiSwapChain::SDR, true};
    return {QRhiSwapChain::SDR, false};
}
