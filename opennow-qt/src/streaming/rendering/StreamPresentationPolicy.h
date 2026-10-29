#pragma once

#include <cstdint>

enum class StreamPresentationMode { Baseline, FixedRefresh, Vrr };

struct StreamPresentationInputs
{
    bool streaming = false;
    bool cloudGsync = false;
    bool active = false;
    bool exposed = false;
    bool fullscreen = false;
    bool tearingSupported = false;
    bool vrrPossible = false;
    bool vrrRequested = false;
};

constexpr StreamPresentationMode streamPresentationMode(const StreamPresentationInputs &input)
{
    if (!input.streaming) return StreamPresentationMode::Baseline;
    return input.cloudGsync && input.active && input.exposed && input.fullscreen
            && input.tearingSupported && input.vrrPossible && input.vrrRequested
        ? StreamPresentationMode::Vrr : StreamPresentationMode::FixedRefresh;
}

constexpr bool streamPresentationNoVsync(StreamPresentationMode mode, bool baselineNoVsync)
{
    return mode == StreamPresentationMode::Vrr
        || (mode == StreamPresentationMode::Baseline && baselineNoVsync);
}

struct StreamPresentationVrrPolicy
{
    bool possible;
    bool requested;
};

constexpr StreamPresentationVrrPolicy streamPresentationVrrPolicy(std::uint32_t flags)
{
    constexpr std::uint32_t enabled = 1u;
    constexpr std::uint32_t possible = 2u;
    constexpr std::uint32_t requested = 4u;
    return {bool(flags & (possible | enabled)), bool(flags & (requested | enabled))};
}

struct StreamPresentationRecreation
{
    bool created;
    bool applied;
};

template<typename SwapChain, typename Flags, typename Accept = bool (*)()>
StreamPresentationRecreation recreateStreamPresentationSwapChain(SwapChain &swapChain, Flags flags,
    Accept accept = +[] { return true; })
{
    const auto proxy = swapChain.proxyData();
    const auto previous = swapChain.flags();
    swapChain.destroy();
    swapChain.setProxyData(proxy);
    swapChain.setFlags(flags);
    if (swapChain.createOrResize() && accept()) return {true, true};
    swapChain.destroy();
    swapChain.setProxyData(proxy);
    swapChain.setFlags(previous);
    return {swapChain.createOrResize() && accept(), false};
}
